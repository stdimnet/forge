//! A data source behind an SSH bastion: an in-process russh server forwards `direct-tcpip`
//! channels to the ClickHouse HTTP mock. Covers the host key confirmation, the password in
//! the keychain (`<id>#ssh`), a saved SSH configuration and the tree label.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_db::ssh::ForgeSsh;
use forge_db::{DataSource, DbService, Driver, MemoryStore, SecretStore, SecretUpdate, SshTunnel};
use forge_remote::{AuthMethod, SshConfiguration, SshConfigurations};
use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::PrivateKey;
use russh::server::{self, Auth, Msg, Session};
use russh::Channel;

#[derive(Clone, Default)]
struct Bastion {
    forwarded: Arc<Mutex<Vec<String>>>,
}

impl server::Handler for Bastion {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if user == "deploy" && password == "tunnel-pw" {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let target = format!("{host_to_connect}:{port_to_connect}");
        self.forwarded.lock().unwrap().push(target.clone());
        match tokio::net::TcpStream::connect(&target).await {
            Ok(mut socket) => {
                reply.accept().await;
                tokio::spawn(async move {
                    let mut stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
                });
            }
            Err(_) => reply.reject(russh::ChannelOpenFailure::ConnectFailed).await,
        }
        Ok(())
    }
}

async fn start_bastion() -> (u16, Bastion) {
    let key = PrivateKey::from(Ed25519Keypair::from_seed(&[9u8; 32]));
    let config = Arc::new(server::Config {
        keys: vec![key],
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let host = Bastion::default();
    let h = host.clone();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let config = config.clone();
            let handler = h.clone();
            tokio::spawn(async move {
                if let Ok(running) = server::run_stream(config, socket, handler).await {
                    let _ = running.await;
                }
            });
        }
    });
    (port, host)
}

fn tunnelled(db_port: u16, tunnel: SshTunnel) -> DataSource {
    let mut ds = DataSource {
        name: "analytics (prod)".into(),
        driver: Driver::Clickhouse,
        // As the bastion sees it.
        host: "127.0.0.1".into(),
        port: Some(db_port),
        database: "shop".into(),
        user: "default".into(),
        scope: forge_db::Scope::Global,
        ..Default::default()
    };
    ds.options.connect_timeout_secs = 3;
    ds.options.ssh = tunnel;
    ds
}

#[tokio::test]
async fn connects_through_an_ad_hoc_ssh_tunnel() {
    let (db_port, requests) = common::clickhouse::start().await;
    let (ssh_port, bastion) = start_bastion().await;
    let home = tempfile::tempdir().unwrap();
    let store = Arc::new(MemoryStore::default());
    let svc = DbService::new(None, store.clone()).with_ssh(Arc::new(ForgeSsh::with(
        home.path().to_path_buf(),
        SshConfigurations::default(),
    )));
    let ds = tunnelled(
        db_port,
        SshTunnel {
            enabled: true,
            host: "127.0.0.1".into(),
            port: ssh_port,
            user: "deploy".into(),
            auth: AuthMethod::Password,
            ..Default::default()
        },
    );
    let secrets = || SecretUpdate {
        ssh: Some("tunnel-pw".into()),
        ssl_key: None,
    };
    // First contact: the bastion's key is unknown.
    let t = svc
        .test_with(&ds, Some("secret".into()), secrets(), None)
        .await;
    assert!(!t.ok);
    let key = t.host_key.expect("host key to confirm");
    assert!(t.error.unwrap().contains("cannot be established"));
    // The user trusts it: known_hosts is updated and the connection works.
    let t = svc
        .test_with(
            &ds,
            Some("secret".into()),
            secrets(),
            Some(key.fingerprint.clone()),
        )
        .await;
    assert!(t.ok, "{t:?}");
    assert_eq!(t.server_version.as_deref(), Some("ClickHouse 24.3.1.1"));
    let known = std::fs::read_to_string(home.path().join(".ssh/known_hosts")).unwrap();
    assert!(known.contains("ssh-ed25519"));

    let info = svc
        .save_with(ds, Some("secret".into()), secrets())
        .await
        .unwrap();
    assert!(info.has_ssh_secret);
    assert_eq!(
        info.via.as_deref(),
        Some(format!("deploy@127.0.0.1:{ssh_port}").as_str())
    );
    let id = info.source.id.clone();
    assert_eq!(
        store.get(&format!("{id}#ssh")).unwrap().as_deref(),
        Some("tunnel-pw")
    );
    let s = svc.introspect(&id, "shop").await.unwrap();
    assert!(s.tables.iter().any(|t| t.name == "events"));
    assert!(bastion
        .forwarded
        .lock()
        .unwrap()
        .iter()
        .all(|t| t == &format!("127.0.0.1:{db_port}")));
    assert!(!requests.lock().unwrap().is_empty());

    // A wrong tunnel password is reported as such.
    let mut bad = svc.get(&id).unwrap();
    bad.id = String::new();
    let t = svc
        .test_with(
            &bad,
            Some("secret".into()),
            SecretUpdate {
                ssh: Some("wrong".into()),
                ssl_key: None,
            },
            None,
        )
        .await;
    assert!(!t.ok);
    assert!(t.error.unwrap().starts_with("SSH tunnel:"));

    svc.delete(&id).await.unwrap();
    assert_eq!(store.get(&format!("{id}#ssh")).unwrap(), None);
}

#[tokio::test]
async fn uses_a_saved_ssh_configuration() {
    let (db_port, _) = common::clickhouse::start().await;
    let (ssh_port, _) = start_bastion().await;
    let home = tempfile::tempdir().unwrap();
    let configs = SshConfigurations {
        configurations: vec![SshConfiguration {
            id: "bastion".into(),
            host: "127.0.0.1".into(),
            port: ssh_port,
            user: "deploy".into(),
            auth: AuthMethod::Password,
            key_file: None,
            dont_save_secret: false,
            name: Some("Prod bastion".into()),
            parse_config: false,
        }],
    };
    let svc = DbService::new(None, Arc::new(MemoryStore::default()))
        .with_ssh(Arc::new(ForgeSsh::with(home.path().to_path_buf(), configs)));
    let ds = tunnelled(
        db_port,
        SshTunnel {
            enabled: true,
            configuration_id: Some("bastion".into()),
            ..Default::default()
        },
    );
    let first = svc
        .test_with(
            &ds,
            Some("secret".into()),
            SecretUpdate {
                ssh: Some("tunnel-pw".into()),
                ssl_key: None,
            },
            None,
        )
        .await;
    let fp = first.host_key.unwrap().fingerprint;
    let t = svc
        .test_with(
            &ds,
            Some("secret".into()),
            SecretUpdate {
                ssh: Some("tunnel-pw".into()),
                ssl_key: None,
            },
            Some(fp),
        )
        .await;
    assert!(t.ok, "{t:?}");
    let info = svc.save(ds, Some("secret".into())).await.unwrap();
    assert_eq!(info.via.as_deref(), Some("Prod bastion"));
}
