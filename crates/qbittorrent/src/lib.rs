//! A client for the qBittorrent Web API.
//!
//! Requests run on Tokio (via `reqwest`), so call them from inside a Tokio runtime.

mod auth;
mod client;
mod error;
mod sync;
mod torrents;
mod transfer;

pub use client::Client;
pub use error::Error;
pub use sync::MainData;
pub use torrents::{Torrent, TorrentState};
pub use transfer::TransferInfo;
