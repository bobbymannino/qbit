use std::fmt;

use reqwest::StatusCode;

/// Errors returned by the qBittorrent client.
#[derive(Debug)]
pub enum Error {
    /// The host is not a valid `http` or `https` URL.
    InvalidHost(String),
    /// The username or password is wrong.
    InvalidCredentials,
    /// qBittorrent banned this IP after too many failed logins.
    Banned,
    /// The request needs a session; log in first (or log in again if it expired).
    NotLoggedIn,
    /// qBittorrent answered with an unexpected status code.
    UnexpectedStatus(StatusCode),
    /// The request failed to send or the response could not be read.
    Http(reqwest::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHost(reason) => write!(f, "invalid host: {reason}"),
            Self::InvalidCredentials => f.write_str("invalid username or password"),
            Self::Banned => f.write_str("this IP is banned after too many failed logins"),
            Self::NotLoggedIn => f.write_str("not logged in"),
            Self::UnexpectedStatus(status) => write!(f, "unexpected response: {status}"),
            Self::Http(err) => write!(f, "request failed: {err}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(err) => Some(err),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::Http(err)
    }
}
