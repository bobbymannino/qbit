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
}
