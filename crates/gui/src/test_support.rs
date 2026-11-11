//! Helpers shared by tests.

use qbittorrent::Torrent;
use serde_json::json;

/// A torrent named `name` in `state`, with `hash` and everything else zeroed.
pub fn torrent(hash: &str, name: &str, state: &str) -> Torrent {
    serde_json::from_value(json!({
        "hash": hash,
        "name": name,
        "size": 0,
        "progress": 0.0,
        "state": state,
        "dlspeed": 0,
        "upspeed": 0,
        "eta": 8_640_000,
        "ratio": 0.0,
        "num_seeds": 0,
        "num_leechs": 0,
        "added_on": 0,
    }))
    .expect("a valid torrent")
}
