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
