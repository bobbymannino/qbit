use qbittorrent::Client;

use crate::credentials::Credentials;

/// Log in to qBittorrent with `credentials`.
///
/// Must run on the Tokio runtime.
///
/// # Errors
///
/// Returns a user-facing message if the host is invalid or the login fails.
pub async fn login(credentials: Credentials) -> Result<Client, String> {
    let client = Client::new(&credentials.host).map_err(|err| err.to_string())?;
    client
        .login(&credentials.username, &credentials.password)
        .await
        .map_err(|err| err.to_string())?;

    Ok(client)
}

/// Accept `minipc:8081` as well as `http://minipc:8081`, defaulting to `http`.
pub fn normalize_host(host: &str) -> String {
    let host = host.trim().trim_end_matches('/');
    if host.contains("://") {
        host.to_owned()
    } else {
        format!("http://{host}")
    }
}
