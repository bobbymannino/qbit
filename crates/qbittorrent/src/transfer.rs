use reqwest::{StatusCode, header};
use serde::Deserialize;

use crate::{Client, Error};

/// Global transfer stats, as returned by `GET /api/v2/transfer/info`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TransferInfo {
    /// Total download speed in bytes per second.
    pub dl_info_speed: u64,
    /// Total upload speed in bytes per second.
    pub up_info_speed: u64,
    /// Bytes downloaded this session.
    pub dl_info_data: u64,
    /// Bytes uploaded this session.
    pub up_info_data: u64,
    /// Global download speed limit in bytes per second; `0` means unlimited.
    #[serde(default)]
    pub dl_rate_limit: i64,
    /// Global upload speed limit in bytes per second; `0` means unlimited.
    #[serde(default)]
    pub up_rate_limit: i64,
}

impl Client {
    /// Fetch the global download and upload speeds.
    ///
    /// # Errors
    ///
    /// - [`Error::NotLoggedIn`] if there is no valid session.
    /// - [`Error::UnexpectedStatus`] for any other non-success response.
    /// - [`Error::Http`] if the request fails or the response isn't valid JSON.
    pub async fn transfer_info(&self) -> Result<TransferInfo, Error> {
        let url = self.endpoint("api/v2/transfer/info")?;

        let res = self
            .http
            .get(url)
            .header(header::REFERER, self.base.as_str())
            .send()
            .await?;

        match res.status() {
            StatusCode::FORBIDDEN => Err(Error::NotLoggedIn),
            status if !status.is_success() => Err(Error::UnexpectedStatus(status)),
            _ => Ok(res.json().await?),
        }
    }

    /// Limit the total download speed of every torrent to `limit` bytes per second;
    /// `0` removes the limit.
    ///
    /// # Errors
    ///
    /// - [`Error::NotLoggedIn`] if there is no valid session.
    /// - [`Error::UnexpectedStatus`] for any other non-success response.
    /// - [`Error::Http`] if the request fails.
    pub async fn set_global_download_limit(&self, limit: u64) -> Result<(), Error> {
        self.post_global_limit("setDownloadLimit", limit).await
    }

    /// Limit the total upload speed of every torrent to `limit` bytes per second;
    /// `0` removes the limit.
    ///
    /// # Errors
    ///
    /// Same as [`Client::set_global_download_limit`].
    pub async fn set_global_upload_limit(&self, limit: u64) -> Result<(), Error> {
        self.post_global_limit("setUploadLimit", limit).await
    }

    /// POST `limit` to `api/v2/transfer/{action}`.
    async fn post_global_limit(&self, action: &str, limit: u64) -> Result<(), Error> {
        let url = self.endpoint(&format!("api/v2/transfer/{action}"))?;
        let limit = limit.to_string();

        let res = self
            .http
            .post(url)
            .header(header::REFERER, self.base.as_str())
            .form(&[("limit", limit.as_str())])
            .send()
            .await?;

        match res.status() {
            StatusCode::FORBIDDEN => Err(Error::NotLoggedIn),
            status if !status.is_success() => Err(Error::UnexpectedStatus(status)),
            _ => Ok(()),
        }
    }
}
