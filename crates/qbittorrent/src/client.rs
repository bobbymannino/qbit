use reqwest::Url;

use crate::Error;

/// A client for the qBittorrent Web API.
///
/// Holds the session cookie, so log in once with [`Client::login`] and reuse the
/// same client for later requests.
#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) http: reqwest::Client,
    pub(crate) base: Url,
}

impl Client {
    /// Create a client for the qBittorrent Web UI at `host`, e.g. `http://localhost:8080`.
    ///
    /// # Errors
    ///
    /// - [`Error::InvalidHost`] if `host` is not a valid `http` or `https` URL.
    /// - [`Error::Http`] if the HTTP client could not be built.
    pub fn new(host: &str) -> Result<Self, Error> {
        let mut base = Url::parse(host).map_err(|err| Error::InvalidHost(err.to_string()))?;
        if !matches!(base.scheme(), "http" | "https") {
            return Err(Error::InvalidHost(format!(
                "unsupported scheme `{}`",
                base.scheme()
            )));
        }

        // Keep any sub-path (e.g. behind a reverse proxy) when joining API paths.
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }

        let http = reqwest::Client::builder().cookie_store(true).build()?;

        Ok(Self { http, base })
    }

    /// Resolve an API path, like `api/v2/auth/login`, against the host.
    pub(crate) fn endpoint(&self, path: &str) -> Result<Url, Error> {
        self.base
            .join(path)
            .map_err(|err| Error::InvalidHost(err.to_string()))
    }
}
