//! Smoke test with a real Delve (`dlv dap`) and Go toolchain: debug a small program, stop at
//! a breakpoint, read the stack and the variables. Ignored by default; run with
//! `FORGE_DLV=/path/to/dlv cargo test -p goforge-delve -- --ignored` (or `dlv` on PATH).

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use forge_dap::DapManager;
use forge_debug::{BreakpointSpec, DebugEvent, DebugServer, EventSink, SessionStatus};
use forge_runtools::Configuration;
use goforge_delve::{request, DelveAdapter, DelveConfig};
use goforge_lang_go::{GoContext, ModuleRoot, GO_BUILD};
use tokio::sync::mpsc;

const MAIN: &str = "package main\n\nimport \"fmt\"\n\nfunc total(items []int) int {\n\tsum := 0\n\tfor _, i := range items {\n\t\tsum += i\n\t}\n\treturn sum\n}\n\nfunc main() {\n\tcart := []int{1250, 725}\n\tfmt.Println(total(cart))\n}\n";

#[tokio::test]
#[ignore]
async fn delve_stops_at_a_breakpoint() {
    let dlv = std::env::var("FORGE_DLV").unwrap_or_else(|_| "dlv".into());
    if std::process::Command::new(&dlv)
        .arg("version")
        .output()
        .is_err()
    {
        eprintln!("no dlv: skipped");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("go.mod"),
        "module example.com/cart\n\ngo 1.21\n",
    )
    .unwrap();
    std::fs::write(root.path().join("main.go"), MAIN).unwrap();

    let (tx, mut events) = mpsc::unbounded_channel();
    let slot: Arc<OnceLock<Weak<DapManager>>> = Arc::new(OnceLock::new());
    let hook = Arc::clone(&slot);
    let sink: EventSink = Arc::new(move |e: DebugEvent| {
        if matches!(e, DebugEvent::Breakpoints { .. }) {
            if let Some(m) = hook.get().and_then(Weak::upgrade) {
                m.breakpoints_changed();
            }
        }
        let _ = tx.send(e);
    });
    let server = Arc::new(DebugServer::new(Arc::clone(&sink)));
    server.open_project(Some(root.path()));
    let manager = Arc::new(DapManager::new(sink, server.clone()));
    manager.set_project_root(Some(root.path().to_path_buf()));
    slot.set(Arc::downgrade(&manager)).ok();
    manager.register_adapter(Arc::new(DelveAdapter::new(DelveConfig {
        dlv,
        env: Vec::new(),
    })));
    server
        .add_breakpoint(BreakpointSpec {
            file: Some("main.go".into()),
            line: Some(8),
            ..Default::default()
        })
        .unwrap();

    let go = GoContext {
        modules: vec![ModuleRoot {
            path: "example.com/cart".into(),
            dir: String::new(),
        }],
        ..GoContext::default()
    };
    let cfg = Configuration::new("cart", GO_BUILD).with_option("package", ".");
    let r = request(&cfg, &BTreeMap::new(), &go, &root.path().to_string_lossy()).unwrap();
    let info = match manager.start(r).await {
        Ok(i) => i,
        Err(e) => {
            while let Ok(ev) = events.try_recv() {
                if let DebugEvent::Output { text, .. } = ev {
                    eprintln!("{text}");
                }
            }
            panic!("{e}");
        }
    };
    let id = info.id.clone();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    let paused = loop {
        let e = tokio::time::timeout_at(deadline, events.recv())
            .await
            .expect("timed out waiting for the breakpoint")
            .expect("events");
        if let DebugEvent::Session { session } = e {
            if session.id == id && session.status == SessionStatus::Paused {
                break session;
            }
        }
    };
    assert_eq!(paused.location.as_ref().map(|l| l.line), Some(8));
    let session = manager.session(&id).unwrap();
    let frames = session.stack(None).await.unwrap();
    assert_eq!(frames[0].r#where, "main.total");
    let vars = session.variables(0, 0).await.unwrap();
    assert!(vars.iter().any(|v| v.name == "sum"), "{vars:?}");
    session.stop().await;
}
