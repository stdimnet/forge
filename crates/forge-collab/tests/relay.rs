//! Code With Me through an `iroh-relay` that runs inside the test process on the loopback
//! interface (self-signed certificate, no internet): a guest without any direct address
//! reaches the host through the relay, the connection moves to the direct path once holes
//! are punched, and a relay token is enforced.

use std::sync::Arc;
use std::time::Duration;

use forge_backend::{Backend, LocalBackend};
use forge_collab::excludes::Excludes;
use forge_collab::guest::GuestSession;
use forge_collab::host::{DevAutoAccept, HostConfig, HostSession};
use forge_collab::link::SessionLink;
use forge_collab::permissions::Permissions;
use forge_collab::relay::{normalize_relay_url, PathKind, RelayOptions};
use forge_collab::transport::net::{ListenOptions, NetTransport};
use forge_collab::transport::relay::NetConfig;
use forge_collab::transport::Transport;
use iroh::test_utils::run_relay_server_with_access;
use iroh::tls::CaTlsConfig;
use iroh_relay::server::{Access, AccessControl, AllowAll, ClientRequest};

const TOKEN: &str = "team-token";

#[derive(Debug)]
struct TokenAccess;

impl AccessControl for TokenAccess {
    async fn on_connect(&self, request: &ClientRequest) -> Access {
        if request.auth_token().as_deref() == Some(TOKEN) {
            Access::Allow
        } else {
            Access::Deny {
                reason: Some("a relay token is required".into()),
            }
        }
    }
}

fn config(url: &str, token: Option<&str>) -> NetConfig {
    NetConfig {
        relay: RelayOptions::host(Some(url.into()), token.map(Into::into), false, false),
        ca_tls: Some(CaTlsConfig::insecure_skip_verify()),
    }
}

fn loopback() -> ListenOptions {
    ListenOptions {
        address: Some("127.0.0.1".parse().unwrap()),
        port: 0,
    }
}

fn host_config() -> HostConfig {
    HostConfig {
        host_name: "Robert".into(),
        machine: None,
        permissions: Permissions::EDIT_FILES,
        excludes: Excludes::defaults(),
        protected: Excludes::protected_defaults(),
        max_file_size: 1 << 20,
        notice: None,
    }
}

fn start_host(transport: Arc<NetTransport>, dir: &std::path::Path) -> Arc<HostSession> {
    let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(dir).unwrap());
    HostSession::start(
        transport,
        backend,
        host_config(),
        Arc::new(DevAutoAccept),
        Arc::new(|_| {}),
    )
}

/// The guest's transport, as the app builds it from a link.
async fn guest_transport(link: &SessionLink, token: Option<&str>) -> NetTransport {
    let own = RelayOptions::host(link.relay.clone(), token.map(Into::into), false, false);
    let relay = RelayOptions::guest(link.relay.as_deref(), &own);
    NetTransport::dialer_with(NetConfig {
        relay,
        ca_tls: Some(CaTlsConfig::insecure_skip_verify()),
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_without_direct_addresses_connects_through_the_relay() {
    let (_map, relay_url, _server) = run_relay_server_with_access(true, Arc::new(AllowAll))
        .await
        .unwrap();
    let relay = normalize_relay_url(relay_url.as_str()).unwrap().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Order.php"), "<?php\n").unwrap();

    let transport = Arc::new(
        NetTransport::listen_with(loopback(), config(&relay, None))
            .await
            .unwrap(),
    );
    assert_eq!(transport.relay_url().as_deref(), Some(relay.as_str()));
    assert!(transport.relay_status().unwrap().connected);
    let host = start_host(transport.clone(), dir.path());
    host.open_doc("Order.php", None).await.unwrap();

    // The link names the relay, and its direct addresses are blocked (dropped).
    let url = host.link().to_url();
    let mut link = SessionLink::parse(&url).unwrap();
    assert_eq!(link.relay.as_deref(), Some(relay.as_str()), "{url}");
    link.addrs.clear();

    let guest_t = Arc::new(guest_transport(&link, None).await);
    let guest = GuestSession::join(guest_t.clone(), &link, "Alex", Arc::new(|_| {}))
        .await
        .unwrap();
    assert_eq!(guest.participants().len(), 2);
    assert_eq!(guest.open_doc("Order.php").await.unwrap().path, "Order.php");

    // The host and the guest both see the connection (relayed first, direct once the
    // hole is punched).
    let host_path = transport.path(&guest_t.local_key()).expect("a host path");
    let guest_path = guest_t.path(&transport.local_key()).expect("a guest path");
    assert!(matches!(host_path.kind, PathKind::Relay | PathKind::Direct));
    assert!(matches!(
        guest_path.kind,
        PathKind::Relay | PathKind::Direct
    ));
    let paths = host.connection_paths();
    assert_eq!(paths.len(), 1);
    // The path may have moved to the direct one in between.
    assert!(paths[0].0 == forge_collab::presence::ParticipantId(1));
    assert!(guest.connection_path().is_some());
    host.end("The host ended the session.").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_connection_moves_to_the_direct_path_after_hole_punching() {
    let (_map, relay_url, _server) = run_relay_server_with_access(true, Arc::new(AllowAll))
        .await
        .unwrap();
    let relay = normalize_relay_url(relay_url.as_str()).unwrap().unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Order.php"), "<?php\n").unwrap();
    let transport = Arc::new(
        NetTransport::listen_with(loopback(), config(&relay, None))
            .await
            .unwrap(),
    );
    let host = start_host(transport.clone(), dir.path());
    let mut link = SessionLink::parse(&host.link().to_url()).unwrap();
    link.addrs.clear();
    let guest_t = Arc::new(guest_transport(&link, None).await);
    let _guest = GuestSession::join(guest_t.clone(), &link, "Alex", Arc::new(|_| {}))
        .await
        .unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let kind = loop {
        let kind = guest_t.path(&transport.local_key()).map(|p| p.kind);
        if kind == Some(PathKind::Direct) || tokio::time::Instant::now() > deadline {
            break kind;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(kind, Some(PathKind::Direct), "no hole was punched");
    host.end("The host ended the session.").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_relay_token_is_enforced() {
    let (_map, relay_url, _server) = run_relay_server_with_access(false, Arc::new(TokenAccess))
        .await
        .unwrap();
    let relay = normalize_relay_url(relay_url.as_str()).unwrap().unwrap();

    // No token, or a wrong one: the host learns why right away.
    for token in [None, Some("wrong")] {
        let started = tokio::time::Instant::now();
        let err = NetTransport::listen_with(loopback(), config(&relay, token))
            .await
            .err()
            .expect("the relay refuses the host");
        let text = err.to_string();
        assert!(text.contains("Cannot reach the relay server"), "{text}");
        assert!(text.contains("a relay token is required"), "{text}");
        assert!(started.elapsed() < Duration::from_secs(6), "{text}");
    }

    // With the token, host and guest connect.
    let transport = Arc::new(
        NetTransport::listen_with(loopback(), config(&relay, Some(TOKEN)))
            .await
            .unwrap(),
    );
    let dir = tempfile::tempdir().unwrap();
    let host = start_host(transport.clone(), dir.path());
    let mut link = SessionLink::parse(&host.link().to_url()).unwrap();
    link.addrs.clear();
    let guest_t = Arc::new(guest_transport(&link, Some(TOKEN)).await);
    GuestSession::join(guest_t, &link, "Alex", Arc::new(|_| {}))
        .await
        .map(|_| ())
        .unwrap();
    host.end("The host ended the session.").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_relay_can_be_tested_from_the_settings() {
    let (_map, relay_url, _server) = run_relay_server_with_access(false, Arc::new(TokenAccess))
        .await
        .unwrap();
    let relay = normalize_relay_url(relay_url.as_str()).unwrap().unwrap();
    let ok = NetTransport::check_relay(config(&relay, Some(TOKEN)))
        .await
        .unwrap();
    assert!(ok.connected);
    assert_eq!(ok.url.trim_end_matches('/'), relay);
    let err = NetTransport::check_relay(config(&relay, None))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("a relay token is required"), "{err}");
    let none = NetTransport::check_relay(NetConfig::default()).await;
    assert!(none.is_err());
}
