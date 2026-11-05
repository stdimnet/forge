//! A whole session over the real transport (iroh/QUIC), bound to the loopback interface so
//! nothing leaves the machine.

use std::sync::{Arc, Mutex};

use forge_backend::{Backend, LocalBackend};
use forge_collab::excludes::Excludes;
use forge_collab::guest::{GuestEvent, GuestSession};
use forge_collab::host::{DevAutoAccept, HostConfig, HostSession};
use forge_collab::link::SessionLink;
use forge_collab::permissions::Permissions;
use forge_collab::transport::net::{ListenOptions, NetTransport};
use yrs::updates::decoder::Decode;
use yrs::{Doc, GetString, OffsetKind, Options, ReadTxn, Text, Transact, Update};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_edits_and_saves_over_quic() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Order.php"), "<?php\n").unwrap();
    let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(dir.path()).unwrap());
    let listen = ListenOptions {
        address: Some("127.0.0.1".parse().unwrap()),
        port: 0,
    };
    let host = HostSession::start(
        Arc::new(NetTransport::listen(listen).await.unwrap()),
        backend,
        HostConfig {
            host_name: "Robert".into(),
            machine: None,
            permissions: Permissions::EDIT_FILES,
            excludes: Excludes::defaults(),
            protected: Excludes::protected_defaults(),
            max_file_size: 1 << 20,
            notice: None,
        },
        Arc::new(DevAutoAccept),
        Arc::new(|_| {}),
    );
    let doc = host.open_doc("Order.php", None).await.unwrap().doc;
    // The link survives its text form.
    let link = SessionLink::parse(&host.link().to_url()).unwrap();
    let updates = Arc::new(Mutex::new(0));
    let seen = updates.clone();
    let guest = GuestSession::join(
        Arc::new(NetTransport::dialer().await.unwrap()),
        &link,
        "Alex",
        Arc::new(move |e| {
            if let GuestEvent::DocUpdate { .. } = e {
                *seen.lock().unwrap() += 1;
            }
        }),
    )
    .await
    .unwrap();
    assert_eq!(guest.participants().len(), 2);
    let opened = guest.open_doc("Order.php").await.unwrap();
    let replica = Doc::with_options(Options {
        offset_kind: OffsetKind::Utf16,
        ..Options::default()
    });
    let text = replica.get_or_insert_text("content");
    replica
        .transact_mut()
        .apply_update(Update::decode_v1(&opened.update).unwrap())
        .unwrap();
    let before = replica.transact().state_vector();
    text.insert(&mut replica.transact_mut(), 0, "// hi\n");
    guest.update(doc, replica.transact().encode_state_as_update_v1(&before));
    guest.save(doc).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("Order.php")).unwrap(),
        "// hi\n<?php\n"
    );
    assert_eq!(text.get_string(&replica.transact()), "// hi\n<?php\n");
    host.end("The host ended the session.").await;
}

/// A file over the co-editing limit (several MB) reaches a guest over QUIC as one view-only
/// document, and the connection stays up afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_large_view_only_file_crosses_quic_without_dropping_the_connection() {
    let dir = tempfile::tempdir().unwrap();
    let log: String = (0..140_000)
        .map(|i| format!("[2026-11-05 10:{:02}:00] local.INFO: line {i}\n", i % 60))
        .collect();
    std::fs::write(dir.path().join("laravel.log"), &log).unwrap();
    let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(dir.path()).unwrap());
    let listen = ListenOptions {
        address: Some("127.0.0.1".parse().unwrap()),
        port: 0,
    };
    let host = HostSession::start(
        Arc::new(NetTransport::listen(listen).await.unwrap()),
        backend,
        HostConfig {
            host_name: "Robert".into(),
            machine: None,
            permissions: Permissions::FULL_ACCESS,
            excludes: Excludes::defaults(),
            protected: Excludes::protected_defaults(),
            max_file_size: 5 << 20,
            notice: None,
        },
        Arc::new(DevAutoAccept),
        Arc::new(|_| {}),
    );
    let link = SessionLink::parse(&host.link().to_url()).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = events.clone();
    let guest = GuestSession::join(
        Arc::new(NetTransport::dialer().await.unwrap()),
        &link,
        "Alex",
        Arc::new(move |e| seen.lock().unwrap().push(format!("{e:?}"))),
    )
    .await
    .unwrap();
    let started = std::time::Instant::now();
    let opened = guest.open_doc("laravel.log").await.unwrap();
    eprintln!(
        "opened {} bytes in {:?}",
        opened.update.len(),
        started.elapsed()
    );
    assert!(opened.read_only);
    assert!(opened.view.is_some());
    for _ in 0..5 {
        guest.ping().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    let seen: Vec<String> = events.lock().unwrap().clone();
    assert!(
        !seen
            .iter()
            .any(|e| e.contains("Reconnect") || e.contains("Lost")),
        "{seen:?}"
    );
    host.end("The host ended the session.").await;
}

/// A message over the host's frame limit (here an edit of several MB) is dropped on the guest
/// with a warning; it must not end the connection, or the guest reconnects, resynchronizes,
/// sends it again and loops.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_oversized_guest_message_does_not_drop_the_connection() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Order.php"), "<?php\n").unwrap();
    let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(dir.path()).unwrap());
    let listen = ListenOptions {
        address: Some("127.0.0.1".parse().unwrap()),
        port: 0,
    };
    let host = HostSession::start(
        Arc::new(NetTransport::listen(listen).await.unwrap()),
        backend,
        HostConfig {
            host_name: "Robert".into(),
            machine: None,
            permissions: Permissions::FULL_ACCESS,
            excludes: Excludes::defaults(),
            protected: Excludes::protected_defaults(),
            max_file_size: 5 << 20,
            notice: None,
        },
        Arc::new(DevAutoAccept),
        Arc::new(|_| {}),
    );
    let link = SessionLink::parse(&host.link().to_url()).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = events.clone();
    let guest = GuestSession::join(
        Arc::new(NetTransport::dialer().await.unwrap()),
        &link,
        "Alex",
        Arc::new(move |e| seen.lock().unwrap().push(format!("{e:?}"))),
    )
    .await
    .unwrap();
    let opened = guest.open_doc("Order.php").await.unwrap();
    guest.update(opened.doc, vec![0u8; 5 << 20]);
    for _ in 0..4 {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        guest.ping().await.unwrap();
    }
    let seen: Vec<String> = events.lock().unwrap().clone();
    assert!(
        !seen
            .iter()
            .any(|e| e.contains("Reconnect") || e.contains("Lost")),
        "{seen:?}"
    );
    host.end("The host ended the session.").await;
}
