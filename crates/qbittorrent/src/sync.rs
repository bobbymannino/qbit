use std::collections::HashMap;

use reqwest::{StatusCode, header};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{Client, Error, Torrent, TransferInfo};

/// Everything qBittorrent knows, kept up to date with `GET /api/v2/sync/maindata`.
///
/// Each [`MainData::update`] asks only for what changed since the last one, so after the
/// first call a refresh carries a few changed fields instead of every torrent in full.
#[derive(Debug, Clone, Default)]
pub struct MainData {
    /// The response id to send next; `0` asks for everything.
    rid: i64,
    /// Every torrent's fields, by hash, merged from the partial updates.
    torrents: HashMap<String, Map<String, Value>>,
    /// Global stats, merged from the partial updates.
    server_state: Map<String, Value>,
}

/// One `sync/maindata` response. Fields that didn't change are left out.
#[derive(Deserialize)]
struct Response {
    rid: i64,
    #[serde(default)]
    full_update: bool,
    #[serde(default)]
    torrents: HashMap<String, Map<String, Value>>,
    #[serde(default)]
    torrents_removed: Vec<String>,
    #[serde(default)]
    server_state: Option<Map<String, Value>>,
}

impl MainData {
    /// Start empty; the first [`MainData::update`] fetches everything.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Fetch what changed since the last update and merge it in.
    ///
    /// # Errors
    ///
    /// - [`Error::NotLoggedIn`] if there is no valid session.
    /// - [`Error::UnexpectedStatus`] for any other non-success response.
    /// - [`Error::Http`] if the request fails or the response isn't valid JSON.
    pub async fn update(&mut self, client: &Client) -> Result<(), Error> {
        let mut url = client.endpoint("api/v2/sync/maindata")?;
        url.query_pairs_mut().append_pair("rid", &self.rid.to_string());

        let res = client
            .http
            .get(url)
            .header(header::REFERER, client.base.as_str())
            .send()
            .await?;

        let update: Response = match res.status() {
            StatusCode::FORBIDDEN => return Err(Error::NotLoggedIn),
            status if !status.is_success() => return Err(Error::UnexpectedStatus(status)),
            _ => res.json().await?,
        };

        self.apply(update);
        Ok(())
    }

    fn apply(&mut self, update: Response) {
        if update.full_update {
            self.torrents.clear();
            self.server_state.clear();
        }
        self.rid = update.rid;

        for (hash, fields) in update.torrents {
            self.torrents.entry(hash).or_default().extend(fields);
        }
        for hash in &update.torrents_removed {
            self.torrents.remove(hash);
        }
        if let Some(fields) = update.server_state {
            self.server_state.extend(fields);
        }
    }

    /// Every torrent, oldest first.
    ///
    /// A torrent whose fields don't form a complete [`Torrent`] is left out.
    #[must_use]
    pub fn torrents(&self) -> Vec<Torrent> {
        let mut torrents: Vec<Torrent> = self
            .torrents
            .iter()
            .filter_map(|(hash, fields)| {
                // The hash is the map key, not one of the fields.
                let mut fields = fields.clone();
                fields.insert("hash".to_owned(), Value::String(hash.clone()));
                serde_json::from_value(Value::Object(fields)).ok()
            })
            .collect();

        // The map has no order; keep rows from shuffling between refreshes.
        torrents.sort_by(|a, b| a.added_on.cmp(&b.added_on).then_with(|| a.hash.cmp(&b.hash)));
        torrents
    }

    /// Global download and upload speeds, once the first update has arrived.
    #[must_use]
    pub fn transfer(&self) -> Option<TransferInfo> {
        serde_json::from_value(Value::Object(self.server_state.clone())).ok()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{MainData, Response};

    /// A torrent's full set of required fields.
    fn torrent(name: &str, added_on: i64) -> Value {
        json!({
            "name": name,
            "size": 0,
            "progress": 0.0,
            "state": "downloading",
            "dlspeed": 0,
            "upspeed": 0,
            "eta": 0,
            "ratio": 0.0,
            "num_seeds": 0,
            "num_leechs": 0,
            "added_on": added_on,
        })
    }

    fn apply(data: &mut MainData, update: Value) {
        data.apply(serde_json::from_value::<Response>(update).unwrap());
    }

    fn names(data: &MainData) -> Vec<String> {
        data.torrents().into_iter().map(|torrent| torrent.name).collect()
    }

    #[test]
    fn starts_by_asking_for_everything() {
        let data = MainData::new();
        assert_eq!(data.rid, 0);
        assert_eq!(names(&data), [""; 0]);
        assert_eq!(data.transfer(), None);
    }

    #[test]
    fn lists_torrents_oldest_first_with_their_hash() {
        let mut data = MainData::new();
        apply(
            &mut data,
            json!({
                "rid": 1,
                "full_update": true,
                "torrents": { "new": torrent("New", 200), "old": torrent("Old", 100) },
            }),
        );

        let torrents = data.torrents();
        assert_eq!(data.rid, 1);
        assert_eq!(torrents.len(), 2);
        assert_eq!(torrents[0].hash, "old");
        assert_eq!(torrents[1].hash, "new");
    }

    #[test]
    fn merges_changed_fields_into_known_torrents() {
        let mut data = MainData::new();
        apply(&mut data, json!({ "rid": 1, "torrents": { "a": torrent("A", 0) } }));
        apply(&mut data, json!({ "rid": 2, "torrents": { "a": { "dlspeed": 512 } } }));

        let torrents = data.torrents();
        assert_eq!(data.rid, 2);
        assert_eq!(torrents[0].name, "A");
        assert_eq!(torrents[0].dlspeed, 512);
    }

    #[test]
    fn drops_removed_torrents() {
        let mut data = MainData::new();
        apply(
            &mut data,
            json!({ "rid": 1, "torrents": { "a": torrent("A", 0), "b": torrent("B", 1) } }),
        );
        apply(&mut data, json!({ "rid": 2, "torrents_removed": ["a"] }));
        assert_eq!(names(&data), ["B"]);
    }

    #[test]
    fn replaces_everything_on_a_full_update() {
        let mut data = MainData::new();
        apply(&mut data, json!({ "rid": 1, "torrents": { "a": torrent("A", 0) } }));
        apply(
            &mut data,
            json!({ "rid": 2, "full_update": true, "torrents": { "b": torrent("B", 0) } }),
        );
        assert_eq!(names(&data), ["B"]);
    }

    #[test]
    fn leaves_out_torrents_missing_fields() {
        let mut data = MainData::new();
        apply(
            &mut data,
            json!({ "rid": 1, "torrents": { "a": torrent("A", 0), "b": { "name": "B" } } }),
        );
        assert_eq!(names(&data), ["A"]);
    }

    #[test]
    fn merges_global_stats_and_limits() {
        let mut data = MainData::new();
        apply(
            &mut data,
            json!({
                "rid": 1,
                "server_state": {
                    "dl_info_speed": 10,
                    "up_info_speed": 20,
                    "dl_info_data": 30,
                    "up_info_data": 40,
                    "dl_rate_limit": 1024,
                    "up_rate_limit": 0,
                },
            }),
        );
        apply(&mut data, json!({ "rid": 2, "server_state": { "dl_info_speed": 99 } }));

        let transfer = data.transfer().unwrap();
        assert_eq!(transfer.dl_info_speed, 99);
        assert_eq!(transfer.up_info_speed, 20);
        assert_eq!(transfer.dl_rate_limit, 1024);
        assert_eq!(transfer.up_rate_limit, 0);
    }
}
