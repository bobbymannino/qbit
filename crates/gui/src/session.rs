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

#[cfg(test)]
mod tests {
    use super::normalize_host;

    #[test]
    fn defaults_to_http() {
        assert_eq!(normalize_host("minipc:8081"), "http://minipc:8081");
    }

    #[test]
    fn keeps_a_given_scheme() {
        assert_eq!(normalize_host("https://example.com"), "https://example.com");
        assert_eq!(normalize_host("http://10.0.0.2:8080"), "http://10.0.0.2:8080");
    }

    #[test]
    fn trims_spaces_and_trailing_slashes() {
        assert_eq!(normalize_host("  minipc:8081/ "), "http://minipc:8081");
        assert_eq!(normalize_host("http://host/qbt//"), "http://host/qbt");
    }
}
