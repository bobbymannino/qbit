use reqwest::{StatusCode, header};

use crate::{Client, Error};

/// Body qBittorrent returns with a `200 OK` when the credentials are wrong.
const LOGIN_FAILED_BODY: &str = "Fails.";

impl Client {
    /// Log in to the qBittorrent Web UI.
    ///
    /// On success the `SID` session cookie is kept by this client and sent with
    /// every later request.
    ///
    /// # Errors
    ///
    /// - [`Error::InvalidCredentials`] if the username or password is wrong.
    /// - [`Error::Banned`] if qBittorrent has banned this IP after too many failed logins.
    /// - [`Error::UnexpectedStatus`] for any other non-success response.
    /// - [`Error::Http`] if the request could not be sent or the response could not be read.
    pub async fn login(&self, username: &str, password: &str) -> Result<(), Error> {
        let url = self.endpoint("api/v2/auth/login")?;

        let res = self
            .http
            .post(url)
            // qBittorrent's CSRF protection rejects a Referer/Origin that doesn't match the host.
            .header(header::REFERER, self.base.as_str())
            .form(&[("username", username), ("password", password)])
            .send()
            .await?;

        let status = res.status();
        if status == StatusCode::FORBIDDEN {
            return Err(Error::Banned);
        }
        if !status.is_success() {
            return Err(Error::UnexpectedStatus(status));
        }

        let body = res.text().await?;
        if body.trim() == LOGIN_FAILED_BODY {
            return Err(Error::InvalidCredentials);
        }

        Ok(())
    }
}
