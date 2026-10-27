//! The kernel protocol against an in-process fake kernel on 127.0.0.1: ROUTER sockets for
//! shell and control and a PUB socket for iopub, checking every signature like ipykernel.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use pyforge_jupyter::kernel::free_ports;
use pyforge_jupyter::protocol::{decode, encode, ConnectionInfo, Message, Signer};
use pyforge_jupyter::session::{EventSink, Session, SessionEvent};
use pyforge_jupyter::{Output, Rendered};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use zeromq::{PubSocket, RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

const KEY: &str = "a0436f6c-1916-498b-8eb9-e81ab9368e84";

fn zmq(frames: Vec<Vec<u8>>) -> ZmqMessage {
    ZmqMessage::try_from(frames.into_iter().map(Bytes::from).collect::<Vec<_>>()).unwrap()
}

fn reply_to(req: &Message, msg_type: &str, content: Value) -> Message {
    let mut m = Message::new(
        &req.header.session,
        &format!("{}-{msg_type}", req.header.msg_id),
        msg_type,
        content,
    );
    m.parent_header = serde_json::to_value(&req.header).unwrap();
    m.identities = req.identities.clone();
    m
}

fn publish(req: &Message, msg_type: &str, content: Value) -> Message {
    let mut m = reply_to(req, msg_type, content);
    m.identities = vec![msg_type.as_bytes().to_vec()];
    m
}

/// A kernel that answers kernel_info, execute (with outputs), helper expressions,
/// interrupt and shutdown. Returns the ports.
async fn fake_kernel() -> ConnectionInfo {
    let ports = free_ports().unwrap();
    let info = ConnectionInfo::local(ports, KEY);
    let signer = Signer::new(KEY);
    let mut shell = RouterSocket::new();
    shell.bind(&info.endpoint(info.shell_port)).await.unwrap();
    let mut control = RouterSocket::new();
    control
        .bind(&info.endpoint(info.control_port))
        .await
        .unwrap();
    let mut iopub = PubSocket::new();
    iopub.bind(&info.endpoint(info.iopub_port)).await.unwrap();
    let (pub_tx, mut pub_rx) = mpsc::unbounded_channel::<Message>();
    let s = signer.clone();
    tokio::spawn(async move {
        while let Some(m) = pub_rx.recv().await {
            let _ = iopub.send(zmq(encode(&m, &s))).await;
        }
    });
    let s = signer.clone();
    let p = pub_tx.clone();
    tokio::spawn(async move {
        let mut count = 0u64;
        loop {
            let Ok(raw) = shell.recv().await else { return };
            let frames: Vec<Vec<u8>> = raw.into_vec().into_iter().map(|b| b.to_vec()).collect();
            // Like ipykernel: messages with a bad signature are dropped.
            let Ok(req) = decode(&frames, &s) else {
                continue;
            };
            let _ = p.send(publish(&req, "status", json!({"execution_state": "busy"})));
            let reply = match req.header.msg_type.as_str() {
                "kernel_info_request" => reply_to(
                    &req,
                    "kernel_info_reply",
                    json!({"status": "ok", "protocol_version": "5.3", "language_info": {"name": "python"}}),
                ),
                "execute_request" => {
                    let code = req.content["code"].as_str().unwrap_or("").to_string();
                    if req.content["silent"] == json!(true) {
                        let expr = req.content["user_expressions"]["forge"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();
                        let value = if expr.contains("variables()") {
                            json!([{"name": "sales", "type": "pandas.core.frame.DataFrame", "value": "DataFrame 2×2", "size": null, "expr": "sales", "children": false, "frame": true}])
                        } else if expr.contains("frame(\"sales\"") {
                            json!({"columns": ["", "region", "orders"], "indexColumns": 1, "rows": [["0", "North", "12"], ["1", "South", "7"]], "totalRows": 2})
                        } else {
                            json!(null)
                        };
                        reply_to(
                            &req,
                            "execute_reply",
                            json!({"status": "ok", "execution_count": count, "user_expressions": {"forge": {"status": "ok", "data": {"text/plain": value.to_string()}, "metadata": {}}}}),
                        )
                    } else {
                        count += 1;
                        let _ = p.send(publish(
                            &req,
                            "execute_input",
                            json!({"code": code, "execution_count": count}),
                        ));
                        if code == "1/0" {
                            let _ = p.send(publish(&req, "error", json!({"ename": "ZeroDivisionError", "evalue": "division by zero", "traceback": ["\u{1b}[0;31mZeroDivisionError\u{1b}[0m: division by zero"]})));
                            reply_to(
                                &req,
                                "execute_reply",
                                json!({"status": "error", "execution_count": count, "ename": "ZeroDivisionError", "evalue": "division by zero", "traceback": []}),
                            )
                        } else {
                            let _ = p.send(publish(
                                &req,
                                "stream",
                                json!({"name": "stdout", "text": "hello\n"}),
                            ));
                            let _ = p.send(publish(&req, "execute_result", json!({"execution_count": count, "data": {"text/plain": "2"}, "metadata": {}})));
                            reply_to(
                                &req,
                                "execute_reply",
                                json!({"status": "ok", "execution_count": count, "user_expressions": {}}),
                            )
                        }
                    }
                }
                other => reply_to(
                    &req,
                    &other.replace("_request", "_reply"),
                    json!({"status": "ok"}),
                ),
            };
            let _ = shell.send(zmq(encode(&reply, &s))).await;
            let _ = p.send(publish(&req, "status", json!({"execution_state": "idle"})));
        }
    });
    let s = signer;
    tokio::spawn(async move {
        loop {
            let Ok(raw) = control.recv().await else {
                return;
            };
            let frames: Vec<Vec<u8>> = raw.into_vec().into_iter().map(|b| b.to_vec()).collect();
            let Ok(req) = decode(&frames, &s) else {
                continue;
            };
            let t = req.header.msg_type.replace("_request", "_reply");
            let reply = reply_to(&req, &t, json!({"status": "ok"}));
            let _ = control.send(zmq(encode(&reply, &s))).await;
        }
    });
    info
}

fn recorder() -> (EventSink, Arc<Mutex<Vec<SessionEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let e = events.clone();
    (Arc::new(move |ev| e.lock().unwrap().push(ev)), events)
}

async fn until(events: &Arc<Mutex<Vec<SessionEvent>>>, pred: impl Fn(&[SessionEvent]) -> bool) {
    for _ in 0..200 {
        if pred(&events.lock().unwrap()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out; events: {:#?}", events.lock().unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runs_cells_against_a_fake_kernel() {
    let info = fake_kernel().await;
    let (sink, events) = recorder();
    let session = Session::connect_zmq(&info, None, sink, Duration::from_secs(10))
        .await
        .unwrap();

    session.execute("cell-a", "1+1").await.unwrap();
    until(&events, |e| {
        e.iter().any(|x| matches!(x, SessionEvent::Done { .. }))
    })
    .await;
    until(&events, |e| {
        e.iter()
            .filter(|x| matches!(x, SessionEvent::Output { .. }))
            .count()
            == 2
    })
    .await;
    let got = events.lock().unwrap().clone();
    assert!(got.contains(&SessionEvent::ExecuteInput {
        cell_id: "cell-a".into(),
        execution_count: Some(1)
    }));
    assert!(got.iter().any(|e| matches!(e, SessionEvent::Output { cell_id, output: Output::Stream { text, .. }, .. } if cell_id == "cell-a" && text == "hello\n")));
    assert!(got.iter().any(|e| matches!(e, SessionEvent::Output { rendered: Rendered::Text { lines, .. }, output: Output::ExecuteResult { .. }, .. } if lines[0].text() == "2")));
    assert!(got.contains(&SessionEvent::Done {
        cell_id: "cell-a".into(),
        status: "ok".into(),
        execution_count: Some(1)
    }));
    assert!(got
        .iter()
        .any(|e| matches!(e, SessionEvent::Status { state } if state == "busy")));

    events.lock().unwrap().clear();
    session.execute("cell-b", "1/0").await.unwrap();
    until(&events, |e| {
        e.iter().any(|x| matches!(x, SessionEvent::Done { .. }))
    })
    .await;
    until(&events, |e| {
        e.iter().any(|x| matches!(x, SessionEvent::Output { .. }))
    })
    .await;
    let got = events.lock().unwrap().clone();
    assert!(got.contains(&SessionEvent::Done {
        cell_id: "cell-b".into(),
        status: "error".into(),
        execution_count: Some(2)
    }));
    assert!(got.iter().any(|e| matches!(
        e,
        SessionEvent::Output {
            rendered: Rendered::Text { error: true, .. },
            ..
        }
    )));

    // Helper requests: no cell events, values from user_expressions.
    events.lock().unwrap().clear();
    let vars = session.variables().await.unwrap();
    assert_eq!(vars[0].name, "sales");
    assert!(vars[0].frame);
    let frame = session.frame("sales", 100).await.unwrap();
    assert_eq!(frame.rows[1], ["1", "South", "7"]);
    assert_eq!(frame.index_columns, 1);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(events.lock().unwrap().iter().all(|e| !matches!(
        e,
        SessionEvent::Output { .. } | SessionEvent::Done { .. } | SessionEvent::Status { .. }
    )));

    session.interrupt().await.unwrap();
    assert!(!session.restart().await.unwrap());
    session.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wrong_key_gets_no_answers() {
    let mut info = fake_kernel().await;
    info.key = "not-the-key".into();
    let (sink, _) = recorder();
    let err = match Session::connect_zmq(&info, None, sink, Duration::from_millis(1500)).await {
        Ok(_) => panic!("connected with a wrong key"),
        Err(e) => e,
    };
    assert!(err.0.contains("did not answer"), "{err}");
}
