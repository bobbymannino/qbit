//! The client's requests and how it reads the answers, against a mock qBittorrent.

use qbittorrent::{Client, Error, MainData};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string, body_string_contains, header, method, path, query_param},
};

/// A mock server and a client pointed at it.
#[expect(clippy::unwrap_used, reason = "the mock server's URI is always a valid host")]
async fn setup() -> (MockServer, Client) {
    let server = MockServer::start().await;
    let client = Client::new(&server.uri()).unwrap();
    (server, client)
}

/// Answer `verb` requests to `route` with `status`, expecting exactly one.
async fn expect(server: &MockServer, verb: &str, route: &str, status: u16) {
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(ResponseTemplate::new(status))
        .expect(1)
        .mount(server)
        .await;
}

#[test]
fn rejects_hosts_that_arent_http() {
    assert!(matches!(Client::new("ftp://example.com"), Err(Error::InvalidHost(_))));
    assert!(matches!(Client::new("not a url"), Err(Error::InvalidHost(_))));
    assert!(Client::new("https://example.com").is_ok());
}

#[tokio::test]
async fn logs_in_with_the_credentials_and_a_matching_referer() {
    let (server, client) = setup().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/auth/login"))
        .and(header("referer", format!("{}/", server.uri())))
        .and(body_string("username=admin&password=p%40ss+word"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Ok."))
        .expect(1)
        .mount(&server)
        .await;

    client.login("admin", "p@ss word").await.unwrap();
}

#[tokio::test]
async fn keeps_the_hosts_sub_path() {
    let server = MockServer::start().await;
    let client = Client::new(&format!("{}/qbt", server.uri())).unwrap();
    Mock::given(method("POST"))
        .and(path("/qbt/api/v2/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Ok."))
        .expect(1)
        .mount(&server)
        .await;

    client.login("admin", "secret").await.unwrap();
}

#[tokio::test]
async fn reports_wrong_credentials() {
    let (server, client) = setup().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Fails."))
        .mount(&server)
        .await;

    let result = client.login("admin", "wrong").await;
    assert!(matches!(result, Err(Error::InvalidCredentials)), "{result:?}");
}

#[tokio::test]
async fn reports_a_banned_ip() {
    let (server, client) = setup().await;
    expect(&server, "POST", "/api/v2/auth/login", 403).await;

    let result = client.login("admin", "wrong").await;
    assert!(matches!(result, Err(Error::Banned)), "{result:?}");
}

#[tokio::test]
async fn reports_other_failed_logins_by_status() {
    let (server, client) = setup().await;
    expect(&server, "POST", "/api/v2/auth/login", 500).await;

    let result = client.login("admin", "secret").await;
    assert!(
        matches!(result, Err(Error::UnexpectedStatus(status)) if status == 500),
        "{result:?}"
    );
}

#[tokio::test]
async fn reports_an_expired_session() {
    let (server, client) = setup().await;
    expect(&server, "GET", "/api/v2/torrents/info", 403).await;

    let result = client.torrents().await;
    assert!(matches!(result, Err(Error::NotLoggedIn)), "{result:?}");
}

#[tokio::test]
async fn starts_and_stops_torrents_by_hash() {
    let (server, client) = setup().await;
    for action in ["start", "stop"] {
        Mock::given(method("POST"))
            .and(path(format!("/api/v2/torrents/{action}")))
            .and(body_string("hashes=a%7Cb"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
    }

    client.start(&["a", "b"]).await.unwrap();
    client.stop(&["a", "b"]).await.unwrap();
}

#[tokio::test]
async fn falls_back_to_qbittorrent_4s_resume_and_pause() {
    let (server, client) = setup().await;
    expect(&server, "POST", "/api/v2/torrents/start", 404).await;
    expect(&server, "POST", "/api/v2/torrents/resume", 200).await;
    expect(&server, "POST", "/api/v2/torrents/stop", 404).await;
    expect(&server, "POST", "/api/v2/torrents/pause", 200).await;

    client.start_all().await.unwrap();
    client.stop_all().await.unwrap();
}

#[tokio::test]
async fn deletes_torrents_with_or_without_their_files() {
    let (server, client) = setup().await;
    for delete_files in ["true", "false"] {
        Mock::given(method("POST"))
            .and(path("/api/v2/torrents/delete"))
            .and(body_string(format!("hashes=a&deleteFiles={delete_files}")))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
    }

    client.delete(&["a"], true).await.unwrap();
    client.delete(&["a"], false).await.unwrap();
}

#[tokio::test]
async fn limits_torrents_speeds() {
    let (server, client) = setup().await;
    for action in ["setDownloadLimit", "setUploadLimit"] {
        Mock::given(method("POST"))
            .and(path(format!("/api/v2/torrents/{action}")))
            .and(body_string("hashes=a%7Cb&limit=1024"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
    }

    client.set_download_limit(&["a", "b"], 1024).await.unwrap();
    client.set_upload_limit(&["a", "b"], 1024).await.unwrap();
}

#[tokio::test]
async fn limits_global_speeds() {
    let (server, client) = setup().await;
    for action in ["setDownloadLimit", "setUploadLimit"] {
        Mock::given(method("POST"))
            .and(path(format!("/api/v2/transfer/{action}")))
            .and(body_string("limit=2048"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
    }

    client.set_global_download_limit(2048).await.unwrap();
    client.set_global_upload_limit(2048).await.unwrap();
}

#[tokio::test]
async fn reports_an_expired_session_when_limiting_global_speeds() {
    let (server, client) = setup().await;
    expect(&server, "POST", "/api/v2/transfer/setDownloadLimit", 403).await;

    let result = client.set_global_download_limit(0).await;
    assert!(matches!(result, Err(Error::NotLoggedIn)), "{result:?}");
}

#[tokio::test]
async fn reads_global_speeds_and_limits() {
    let (server, client) = setup().await;
    Mock::given(method("GET"))
        .and(path("/api/v2/transfer/info"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "dl_info_speed": 1,
            "up_info_speed": 2,
            "dl_info_data": 3,
            "up_info_data": 4,
            "dl_rate_limit": 5,
            "up_rate_limit": 6,
            "connection_status": "connected",
        })))
        .mount(&server)
        .await;

    let transfer = client.transfer_info().await.unwrap();
    assert_eq!(transfer.dl_info_speed, 1);
    assert_eq!(transfer.up_info_data, 4);
    assert_eq!(transfer.dl_rate_limit, 5);
    assert_eq!(transfer.up_rate_limit, 6);
}

#[tokio::test]
async fn syncs_from_the_last_response_id() {
    let (server, client) = setup().await;
    Mock::given(method("GET"))
        .and(path("/api/v2/sync/maindata"))
        .and(query_param("rid", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rid": 7,
            "full_update": true,
            "server_state": {
                "dl_info_speed": 1,
                "up_info_speed": 2,
                "dl_info_data": 3,
                "up_info_data": 4,
            },
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v2/sync/maindata"))
        .and(query_param("rid", "7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "rid": 8 })))
        .expect(1)
        .mount(&server)
        .await;

    let mut data = MainData::new();
    data.update(&client).await.unwrap();
    data.update(&client).await.unwrap();
    assert_eq!(data.transfer().map(|transfer| transfer.dl_info_speed), Some(1));
}

#[tokio::test]
async fn sends_the_session_cookie_after_logging_in() {
    let (server, client) = setup().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/auth/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("set-cookie", "SID=abc123; path=/")
                .set_body_string("Ok."),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v2/torrents/info"))
        .and(header("cookie", "SID=abc123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(1)
        .mount(&server)
        .await;

    client.login("admin", "secret").await.unwrap();
    assert_eq!(client.torrents().await.unwrap(), []);
}

#[tokio::test]
async fn sends_limits_as_whole_bytes() {
    let (server, client) = setup().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/transfer/setUploadLimit"))
        .and(body_string_contains(format!("limit={}", u64::MAX)))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    client.set_global_upload_limit(u64::MAX).await.unwrap();
}
