//! Tests of the feed against a local HTTP server (no network access).

use super::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Canned answers by `"METHOD /path"`: status, extra headers and body.
type Routes = HashMap<String, (u16, Vec<(&'static str, String)>, String)>;

/// A local HTTP/1.1 server. `routes` gets the server's base URL, so the feed can point its
/// downloads back at the same server.
async fn serve(routes: impl FnOnce(&str) -> Routes) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let routes = Arc::new(routes(&base));
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let routes = routes.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match socket.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&buf).to_string();
                let key = head
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .split(' ')
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ");
                let (status, headers, body) =
                    routes
                        .get(&key)
                        .cloned()
                        .unwrap_or((404, vec![], String::new()));
                let mut out = format!("HTTP/1.1 {status} X\r\nConnection: close\r\n");
                if !headers.iter().any(|(k, _)| *k == "Content-Length") {
                    out.push_str(&format!("Content-Length: {}\r\n", body.len()));
                }
                for (k, v) in &headers {
                    out.push_str(&format!("{k}: {v}\r\n"));
                }
                out.push_str("\r\n");
                if !key.starts_with("HEAD ") {
                    out.push_str(&body);
                }
                let _ = socket.write_all(out.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    base
}

fn client() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

fn feed_json(version: &str, base: &str) -> String {
    serde_json::json!({
        "version": version,
        "notes": "Faster indexing.",
        "pub_date": "2026-11-02T10:00:00Z",
        "platforms": {
            "linux-x86_64": { "signature": "sig", "url": format!("{base}/Forge_amd64.AppImage") },
            "linux-x86_64-deb": { "signature": "sig-deb", "url": format!("{base}/Forge_amd64.deb") },
        }
    })
    .to_string()
}

fn route(routes: &mut Routes, key: &str, status: u16, body: String) {
    routes.insert(key.to_string(), (status, vec![], body));
}

/// A server with `latest.json` announcing `version` and a 94 MB AppImage.
async fn release_server(version: &'static str) -> String {
    serve(|base| {
        let mut routes = Routes::new();
        route(
            &mut routes,
            "GET /latest.json",
            200,
            feed_json(version, base),
        );
        routes.insert(
            "HEAD /Forge_amd64.AppImage".into(),
            (
                200,
                vec![("Content-Length", (94 * 1024 * 1024).to_string())],
                String::new(),
            ),
        );
        routes
    })
    .await
}

fn targets() -> Vec<String> {
    vec!["linux-x86_64-appimage".into(), "linux-x86_64".into()]
}

async fn run_check(
    base: &str,
    current: &str,
    state: &UpdateState,
    manual: bool,
) -> Result<CheckOutcome> {
    let current = Version::parse(current).unwrap();
    let endpoint = format!("{base}/latest.json");
    let targets = targets();
    let req = CheckRequest {
        endpoint: &endpoint,
        current: &current,
        channel: Channel::Stable,
        state,
        manual,
        now: 1_800_000_000,
        targets: &targets,
    };
    check(&client(), &req).await
}

#[test]
fn picks_the_most_specific_platform() {
    let feed = Feed::parse(&feed_json("0.2.0", "https://x.test")).unwrap();
    let (key, p) = feed.platform(&targets()).unwrap();
    assert_eq!(key, "linux-x86_64");
    assert!(p.url.ends_with(".AppImage"));
    let deb = vec!["linux-x86_64-deb".to_string(), "linux-x86_64".to_string()];
    assert_eq!(feed.platform(&deb).unwrap().1.signature, "sig-deb");
    assert!(feed.platform(&["windows-x86_64".to_string()]).is_none());
    assert!(Feed::parse("{\"notes\": 1}").is_err());
}

#[test]
fn summaries_mention_size_and_simulation() {
    let s = summary("0.1.0", Some(94 * 1024 * 1024), false);
    assert_eq!(
        s,
        "You have 0.1.0. The 94 MB download is checked against Forge's signing key."
    );
    assert!(summary("0.1.0", None, true).contains("Simulated update"));
    let offer = UpdateOffer::new("0.2.0", "0.1.0", Some("  ".into()), None, None, false);
    assert_eq!(offer.notes, None);
}

#[tokio::test]
async fn offers_a_newer_release_with_its_size() {
    let base = release_server("v0.2.0").await;
    let outcome = run_check(&base, "0.1.0", &UpdateState::default(), false)
        .await
        .unwrap();
    let CheckOutcome::Available { offer } = outcome else {
        panic!("expected an offer, got {outcome:?}");
    };
    assert_eq!(offer.version, "0.2.0");
    assert_eq!(offer.current_version, "0.1.0");
    assert_eq!(offer.size, Some(94 * 1024 * 1024));
    assert_eq!(offer.notes.as_deref(), Some("Faster indexing."));
    assert!(offer.summary.contains("94 MB"));
    assert!(!offer.simulated);
}

#[tokio::test]
async fn up_to_date_skipped_and_no_content() {
    let base = release_server("0.2.0").await;
    let state = UpdateState::default();
    assert_eq!(
        run_check(&base, "0.2.0", &state, false).await.unwrap(),
        CheckOutcome::UpToDate
    );
    let mut skipped = UpdateState::default();
    skipped.skip("0.2.0");
    assert_eq!(
        run_check(&base, "0.1.0", &skipped, false).await.unwrap(),
        CheckOutcome::NotOffered {
            version: "0.2.0".into(),
            verdict: Verdict::Skipped
        }
    );
    // A manual check offers the skipped version anyway.
    assert!(matches!(
        run_check(&base, "0.1.0", &skipped, true).await.unwrap(),
        CheckOutcome::Available { .. }
    ));

    let empty = serve(|_| {
        let mut r = Routes::new();
        route(&mut r, "GET /latest.json", 204, String::new());
        r
    })
    .await;
    assert_eq!(
        run_check(&empty, "0.1.0", &state, false).await.unwrap(),
        CheckOutcome::UpToDate
    );
}

#[tokio::test]
async fn errors_from_the_server_and_the_feed() {
    let state = UpdateState::default();
    let missing = serve(|_| Routes::new()).await;
    assert!(matches!(
        run_check(&missing, "0.1.0", &state, false).await,
        Err(UpdateError::Status(404))
    ));
    let broken = serve(|_| {
        let mut r = Routes::new();
        route(&mut r, "GET /latest.json", 200, "<html>".into());
        r
    })
    .await;
    assert!(matches!(
        run_check(&broken, "0.1.0", &state, false).await,
        Err(UpdateError::Feed(_))
    ));
    let windows_only = serve(|base| {
        let feed = serde_json::json!({
            "version": "0.2.0",
            "platforms": { "windows-x86_64": { "signature": "s", "url": format!("{base}/x.msi") } }
        });
        let mut r = Routes::new();
        route(&mut r, "GET /latest.json", 200, feed.to_string());
        r
    })
    .await;
    assert!(matches!(
        run_check(&windows_only, "0.1.0", &state, false).await,
        Err(UpdateError::NoPlatform(_))
    ));
    // Nothing listens on this port any more.
    let closed = format!("http://127.0.0.1:{}", forge_ports::free_port().unwrap());
    assert!(matches!(
        run_check(&closed, "0.1.0", &state, false).await,
        Err(UpdateError::Network(_))
    ));
}

#[tokio::test]
async fn size_is_optional() {
    let base = serve(|base| {
        let mut r = Routes::new();
        route(&mut r, "GET /latest.json", 200, feed_json("0.2.0", base));
        r
    })
    .await;
    let outcome = run_check(&base, "0.1.0", &UpdateState::default(), false)
        .await
        .unwrap();
    let CheckOutcome::Available { offer } = outcome else {
        panic!("expected an offer");
    };
    assert_eq!(offer.size, None);
    assert!(!offer.summary.contains("MB"));
}
