use std::env;

use qbittorrent::Client;

/// Log in to qBittorrent with the credentials from the environment (or `.env`).
///
/// Must run on the Tokio runtime.
///
/// # Errors
///
/// Returns a user-facing message if a variable is missing or the login fails.
pub async fn login_from_env() -> Result<Client, String> {
    let host = var("QBITTORRENT_HOST")?;
    let username = var("QBITTORRENT_USERNAME")?;
    let password = var("QBITTORRENT_PASSWORD")?;

    let client = Client::new(&host).map_err(|err| err.to_string())?;
    client
        .login(&username, &password)
        .await
        .map_err(|err| err.to_string())?;

    Ok(client)
}

/// Read a required environment variable.
fn var(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("{name} is not set"))
}
