//! Preferences kept between launches, in a JSON file in the system's config folder.
//!
//! That's `~/Library/Application Support/qbit` on macOS, `%APPDATA%\qbit` on Windows,
//! and `$XDG_CONFIG_HOME/qbit` (usually `~/.config/qbit`) on Linux.

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::consts::APP_ID;

/// The preferences file, inside the app's config folder.
const FILE_NAME: &str = "prefs.json";

/// What's remembered between launches.
///
/// Fields missing from the file (e.g. one saved by an older version) take their default.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// The torrent table's columns in the order the user arranged them, by key.
    pub column_order: Vec<String>,
}

impl Prefs {
    /// Read the saved preferences, or the defaults if there are none or they can't be read.
    pub fn load() -> Self {
        let Some(path) = path() else { return Self::default() };
        match fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_else(|err| {
                eprintln!("Ignoring unreadable preferences in {}: {err}", path.display());
                Self::default()
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(err) => {
                eprintln!("Couldn't read preferences from {}: {err}", path.display());
                Self::default()
            }
        }
    }

    /// Change the saved preferences with `change`, keeping whatever else is saved.
    pub fn update(change: impl FnOnce(&mut Self)) {
        let mut prefs = Self::load();
        change(&mut prefs);
        if let Err(message) = prefs.save() {
            eprintln!("{message}");
        }
    }

    /// Write these preferences, replacing any saved before.
    ///
    /// # Errors
    ///
    /// Returns a message if the file can't be written.
    fn save(&self) -> Result<(), String> {
        let path = path().ok_or("Couldn't find a folder to save preferences in")?;
        let json = serde_json::to_string_pretty(self).map_err(|err| format!("Couldn't encode preferences: {err}"))?;
        let fail = |err: std::io::Error| format!("Couldn't save preferences to {}: {err}", path.display());

        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(fail)?;
        }
        // Written beside the file and renamed over it, so a crash can't leave half a file.
        let partial = path.with_extension("json.partial");
        fs::write(&partial, json).map_err(fail)?;
        fs::rename(&partial, &path).map_err(fail)
    }
}

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(APP_ID).join(FILE_NAME))
}
