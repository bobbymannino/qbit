use keyring::Entry;
use serde::{Deserialize, Serialize};

use crate::consts::APP_ID;

/// The keychain account the credentials are stored under (the service is [`APP_ID`]).
const ACCOUNT: &str = "qbittorrent";

/// How to reach and log in to a qBittorrent Web UI.
///
/// Stored as one JSON secret in the OS credential store: Keychain on macOS,
/// Credential Manager on Windows, Secret Service on Linux.
///
/// Deliberately not `Debug`, so the password can't end up in logs.
#[derive(Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub host: String,
    pub username: String,
    pub password: String,
}

impl Credentials {
    /// Read the saved credentials, or `None` if none are saved.
    ///
    /// # Errors
    ///
    /// Returns a message if the credential store can't be read or holds bad data.
    pub fn load() -> Result<Option<Self>, String> {
        let secret = match entry()?.get_password() {
            Ok(secret) => secret,
            Err(keyring::Error::NoEntry) => return Ok(None),
            Err(err) => return Err(format!("Couldn't read saved login: {err}")),
        };

        serde_json::from_str(&secret)
            .map(Some)
            .map_err(|err| format!("Saved login is unreadable: {err}"))
    }

    /// Save these credentials, replacing any saved before.
    ///
    /// # Errors
    ///
    /// Returns a message if the credential store can't be written.
    pub fn save(&self) -> Result<(), String> {
        let secret = serde_json::to_string(self)
            .map_err(|err| format!("Couldn't encode login: {err}"))?;

        entry()?
            .set_password(&secret)
            .map_err(|err| format!("Couldn't save login: {err}"))
    }

    /// Delete the saved credentials. Succeeds if none were saved.
    ///
    /// # Errors
    ///
    /// Returns a message if the credential store can't be written.
    pub fn clear() -> Result<(), String> {
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(format!("Couldn't remove saved login: {err}")),
        }
    }
}

fn entry() -> Result<Entry, String> {
    Entry::new(APP_ID, ACCOUNT).map_err(|err| format!("Credential store unavailable: {err}"))
}
