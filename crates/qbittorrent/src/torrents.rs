use std::fmt;

use reqwest::{StatusCode, header};
use serde::Deserialize;

use crate::{Client, Error};

/// A torrent, as returned by `GET /api/v2/torrents/info`.
///
/// Only the commonly used fields are kept; qBittorrent sends many more.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Torrent {
    /// The torrent's info hash.
    pub hash: String,
    /// Display name.
    pub name: String,
    /// Total size of the files selected for download, in bytes.
    pub size: u64,
    /// Download progress, from `0.0` to `1.0`.
    pub progress: f64,
    /// Current state.
    pub state: TorrentState,
    /// Download speed in bytes per second.
    pub dlspeed: u64,
    /// Upload speed in bytes per second.
    pub upspeed: u64,
    /// Estimated seconds until complete (`8640000` means infinite).
    pub eta: i64,
    /// Share ratio.
    pub ratio: f64,
    /// Seeds connected to.
    pub num_seeds: i64,
    /// Leechers connected to.
    pub num_leechs: i64,
    /// Category, or empty if none.
    #[serde(default)]
    pub category: String,
    /// Where the torrent's data is saved.
    #[serde(default)]
    pub save_path: String,
    /// Unix timestamp of when the torrent was added.
    pub added_on: i64,
}

/// A torrent's state, as reported by qBittorrent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TorrentState {
    Error,
    MissingFiles,
    Uploading,
    /// Seeding is paused (qBittorrent 4.x).
    #[serde(rename = "pausedUP")]
    PausedUp,
    /// Seeding is stopped (qBittorrent 5.x).
    #[serde(rename = "stoppedUP")]
    StoppedUp,
    #[serde(rename = "queuedUP")]
    QueuedUp,
    #[serde(rename = "stalledUP")]
    StalledUp,
    #[serde(rename = "checkingUP")]
    CheckingUp,
    #[serde(rename = "forcedUP")]
    ForcedUp,
    Allocating,
    Downloading,
    #[serde(rename = "metaDL")]
    MetaDl,
    #[serde(rename = "forcedMetaDL")]
    ForcedMetaDl,
    /// Downloading is paused (qBittorrent 4.x).
    #[serde(rename = "pausedDL")]
    PausedDl,
    /// Downloading is stopped (qBittorrent 5.x).
    #[serde(rename = "stoppedDL")]
    StoppedDl,
    #[serde(rename = "queuedDL")]
    QueuedDl,
    #[serde(rename = "stalledDL")]
    StalledDl,
    #[serde(rename = "checkingDL")]
    CheckingDl,
    #[serde(rename = "forcedDL")]
    ForcedDl,
    CheckingResumeData,
    Moving,
    /// Any state this client doesn't know about yet.
    #[serde(other)]
    Unknown,
}

impl TorrentState {
    /// Whether the torrent is downloading, or waiting to (queued, stalled,
    /// fetching metadata, checking or allocating), rather than stopped or seeding.
    #[must_use]
    pub const fn is_downloading(self) -> bool {
        matches!(
            self,
            Self::Downloading
                | Self::ForcedDl
                | Self::MetaDl
                | Self::ForcedMetaDl
                | Self::QueuedDl
                | Self::StalledDl
                | Self::CheckingDl
                | Self::Allocating
        )
    }
}

impl fmt::Display for TorrentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Error => "Error",
            Self::MissingFiles => "Missing files",
            Self::Uploading | Self::ForcedUp => "Seeding",
            Self::PausedUp | Self::StoppedUp => "Completed",
            Self::QueuedUp => "Queued for seeding",
            Self::StalledUp => "Seeding (stalled)",
            Self::CheckingUp | Self::CheckingDl => "Checking",
            Self::Allocating => "Allocating",
            Self::Downloading | Self::ForcedDl => "Downloading",
            Self::MetaDl | Self::ForcedMetaDl => "Fetching metadata",
            Self::PausedDl | Self::StoppedDl => "Stopped",
            Self::QueuedDl => "Queued",
            Self::StalledDl => "Stalled",
            Self::CheckingResumeData => "Checking resume data",
            Self::Moving => "Moving",
            Self::Unknown => "Unknown",
        })
    }
}

impl Client {
    /// List every torrent.
    ///
    /// Requires a successful [`Client::login`] first.
    ///
    /// # Errors
    ///
    /// - [`Error::NotLoggedIn`] if there is no valid session.
    /// - [`Error::UnexpectedStatus`] for any other non-success response.
    /// - [`Error::Http`] if the request fails or the response isn't valid torrent JSON.
    pub async fn torrents(&self) -> Result<Vec<Torrent>, Error> {
        let url = self.endpoint("api/v2/torrents/info")?;

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
