//! The Forge editor bridge client against a fake Unity peer on loopback: the handshake with
//! the token of `Library/ForgeBridge.json`, commands and their answers, state changes,
//! console messages, "open file" requests and the end of the connection. No Unity needed.

use std::time::Duration;

use dotnetforge_gamedev::unity::bridge::{
    parse_instance, BridgeClient, BridgeEvent, Command, LogLevel, PlayMode,
};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

const TOKEN: &str = "3f9a1c";

/// A Unity editor with the bridge package, as far as the protocol goes. It records every
/// message it receives and closes after `close_after` commands.
async fn fake_unity(listener: TcpListener, close_after: usize) -> Vec<Value> {
    let (stream, _) = listener.accept().await.unwrap();
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    let mut seen = Vec::new();
    let send = |v: Value| format!("{v}\n");
    let hello: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    seen.push(hello.clone());
    if hello["token"] != TOKEN {
        let _ = write
            .write_all(send(json!({"type": "error", "message": "wrong token"})).as_bytes())
            .await;
        return seen;
    }
    write
        .write_all(
            send(json!({"type": "welcome", "protocol": 1, "unityVersion": "2022.3.10f1",
                "projectPath": "/home/sam/Shooter", "pid": 4242, "playMode": "edit", "compiling": false}))
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut commands = 0;
    while let Ok(Some(line)) = lines.next_line().await {
        let msg: Value = serde_json::from_str(&line).unwrap();
        seen.push(msg.clone());
        let id = msg["id"].clone();
        let mut out = String::new();
        match msg["type"].as_str().unwrap() {
            "play" => {
                out += &send(json!({"type": "state", "playMode": "playing", "compiling": false}));
                out += &send(json!({"type": "log", "level": "exception",
                    "message": "NullReferenceException: Object reference not set to an instance of an object",
                    "stackTrace": "Shooter.Player.Update () (at Assets/Scripts/Player.cs:30)\n", "time": "10:00:01"}));
                out += &send(json!({"type": "ack", "id": id, "ok": true, "message": ""}));
            }
            "pause" => {
                out += &send(json!({"type": "state", "playMode": "paused", "compiling": false}));
                out += &send(json!({"type": "ack", "id": id, "ok": true}));
            }
            "stop" => {
                out += &send(json!({"type": "state", "playMode": "edit", "compiling": false}));
                out += &send(json!({"type": "ack", "id": id, "ok": true}));
            }
            "refresh" => {
                out += &send(json!({"type": "state", "playMode": "edit", "compiling": true}));
                out += &send(json!({"type": "ack", "id": id, "ok": true}));
                out += &send(json!({"type": "log", "level": "error",
                    "message": "Assets/Scripts/Enemy.cs(8,13): error CS0103: The name 'speed' does not exist in the current context",
                    "stackTrace": ""}));
                out += &send(
                    json!({"type": "openFile", "path": "/home/sam/Shooter/Assets/Scripts/Enemy.cs", "line": 8, "column": 13}),
                );
            }
            "syncSolution" => {
                out += &send(json!({"type": "ack", "id": id, "ok": false,
                    "message": "Install com.unity.ide.visualstudio (Window > Package Manager) to generate project files."}));
            }
            _ => {
                out += &send(
                    json!({"type": "ack", "id": id, "ok": false, "message": "unknown command"}),
                )
            }
        }
        // Something from a newer protocol is ignored by Forge.
        out += &send(json!({"type": "telemetry", "x": 1}));
        write.write_all(out.as_bytes()).await.unwrap();
        commands += 1;
        if commands == close_after {
            break;
        }
    }
    seen
}

async fn next(events: &mut mpsc::UnboundedReceiver<BridgeEvent>) -> BridgeEvent {
    tokio::time::timeout(Duration::from_secs(10), events.recv())
        .await
        .expect("an event")
        .expect("the channel is open")
}

async fn listen() -> (TcpListener, String) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap().to_string();
    (l, addr)
}

#[tokio::test]
async fn plays_streams_logs_and_opens_files() {
    let (listener, addr) = listen().await;
    let unity = tokio::spawn(fake_unity(listener, 5));
    let instance = parse_instance(&format!(
        r#"{{"port": {}, "pid": 4242, "token": "{TOKEN}", "protocol": 1}}"#,
        addr.rsplit(':').next().unwrap()
    ))
    .unwrap();
    let (client, mut events) = BridgeClient::connect(
        &format!("127.0.0.1:{}", instance.port),
        &instance.token,
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert_eq!(client.welcome.unity_version, "2022.3.10f1");
    assert_eq!(client.welcome.pid, 4242);
    assert_eq!(client.welcome.play_mode, PlayMode::Edit);

    let ack = client.send(Command::Play).await.unwrap();
    assert!(ack.ok);
    assert_eq!(
        next(&mut events).await,
        BridgeEvent::State {
            play_mode: PlayMode::Playing,
            compiling: false
        }
    );
    let BridgeEvent::Log { entry } = next(&mut events).await else {
        panic!("a log")
    };
    assert_eq!(entry.level, LogLevel::Exception);
    assert_eq!(entry.time.as_deref(), Some("10:00:01"));
    let loc = entry.location.unwrap();
    assert_eq!(
        (loc.path.as_str(), loc.line),
        ("Assets/Scripts/Player.cs", 30)
    );

    assert!(client.send(Command::Pause).await.unwrap().ok);
    assert!(matches!(
        next(&mut events).await,
        BridgeEvent::State {
            play_mode: PlayMode::Paused,
            ..
        }
    ));
    assert!(client.send(Command::Stop).await.unwrap().ok);
    assert!(matches!(
        next(&mut events).await,
        BridgeEvent::State {
            play_mode: PlayMode::Edit,
            ..
        }
    ));

    assert!(client.send(Command::Refresh).await.unwrap().ok);
    assert_eq!(
        next(&mut events).await,
        BridgeEvent::State {
            play_mode: PlayMode::Edit,
            compiling: true
        }
    );
    let BridgeEvent::Log { entry } = next(&mut events).await else {
        panic!("a compiler error")
    };
    let loc = entry.location.unwrap();
    assert_eq!(
        (loc.path.as_str(), loc.line, loc.column),
        ("Assets/Scripts/Enemy.cs", 8, Some(13))
    );
    assert_eq!(
        next(&mut events).await,
        BridgeEvent::OpenFile {
            path: "/home/sam/Shooter/Assets/Scripts/Enemy.cs".into(),
            line: 8,
            column: 13
        }
    );

    let ack = client.send(Command::SyncSolution).await.unwrap();
    assert!(!ack.ok);
    assert!(ack.message.unwrap().contains("com.unity.ide.visualstudio"));

    // The fake editor quits after five commands.
    let BridgeEvent::Disconnected { reason } = next(&mut events).await else {
        panic!("disconnected")
    };
    assert!(reason.contains("closed"), "{reason}");
    let err = client.send(Command::Play).await.unwrap_err();
    assert!(err.contains("Unity"), "{err}");

    let seen = unity.await.unwrap();
    assert_eq!(seen[0]["type"], "hello");
    assert_eq!(seen[0]["client"], "Forge");
    assert_eq!(seen[0]["protocol"], 1);
    let kinds: Vec<&str> = seen[1..]
        .iter()
        .map(|m| m["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        vec!["play", "pause", "stop", "refresh", "syncSolution"]
    );
    // Every command carries its own id.
    let mut ids: Vec<u64> = seen[1..]
        .iter()
        .map(|m| m["id"].as_u64().unwrap())
        .collect();
    ids.dedup();
    assert_eq!(ids.len(), 5);
}

#[tokio::test]
async fn refuses_a_wrong_token_and_missing_editors() {
    let (listener, addr) = listen().await;
    let unity = tokio::spawn(fake_unity(listener, 1));
    let err = BridgeClient::connect(&addr, "guess", Duration::from_secs(5))
        .await
        .err()
        .unwrap();
    assert!(
        err.contains("refused") && err.contains("wrong token"),
        "{err}"
    );
    assert_eq!(unity.await.unwrap()[0]["token"], "guess");

    // Nothing listens.
    let (listener, addr) = listen().await;
    drop(listener);
    let err = BridgeClient::connect(&addr, TOKEN, Duration::from_secs(5))
        .await
        .err()
        .unwrap();
    assert!(err.contains("Cannot connect"), "{err}");
}

#[tokio::test]
async fn closing_ends_the_connection_for_the_editor() {
    let (listener, addr) = listen().await;
    let unity = tokio::spawn(fake_unity(listener, 99));
    let (client, mut events) = BridgeClient::connect(&addr, TOKEN, Duration::from_secs(5))
        .await
        .unwrap();
    assert!(client.is_open());
    client.close();
    // The fake editor sees the end of the stream and stops; Forge then sees the close.
    let seen = tokio::time::timeout(Duration::from_secs(10), unity)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(seen.len(), 1);
    assert!(matches!(
        next(&mut events).await,
        BridgeEvent::Disconnected { .. }
    ));
}
