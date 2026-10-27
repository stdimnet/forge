//! A real ipykernel (ignored: needs Python with ipykernel, and pandas for the DataFrame
//! part). Run with `FORGE_JUPYTER_PYTHON=/path/to/python cargo test -p pyforge-jupyter
//! --features runtime --test ipykernel_smoke -- --ignored`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pyforge_jupyter::kernel::{free_ports, write_connection_file};
use pyforge_jupyter::protocol::ConnectionInfo;
use pyforge_jupyter::session::{Session, SessionEvent};
use pyforge_jupyter::{Output, Rendered};

async fn wait_for(events: &Arc<Mutex<Vec<SessionEvent>>>, pred: impl Fn(&SessionEvent) -> bool) {
    for _ in 0..400 {
        if events.lock().unwrap().iter().any(&pred) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out: {:#?}", events.lock().unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Python with ipykernel"]
async fn talks_to_ipykernel() {
    let python = std::env::var("FORGE_JUPYTER_PYTHON").unwrap_or_else(|_| "python3".into());
    let dir = tempfile::tempdir().unwrap();
    let info = ConnectionInfo::local(free_ports().unwrap(), "6c9b2f6e-smoke-key");
    let file = dir.path().join("kernel.json");
    write_connection_file(&file, &info).unwrap();
    let mut child = std::process::Command::new(&python)
        .args(["-m", "ipykernel_launcher", "-f"])
        .arg(&file)
        .spawn()
        .expect("python starts");
    let events = Arc::new(Mutex::new(Vec::new()));
    let e = events.clone();
    let session = Session::connect_zmq(
        &info,
        None,
        Arc::new(move |ev| e.lock().unwrap().push(ev)),
        Duration::from_secs(60),
    )
    .await
    .expect("ipykernel answers");

    session.execute("a", "import pandas as pd\nsales = pd.DataFrame({'region': ['North', 'South'], 'orders': [12, 7]})\nprint('hi')\nsales").await.unwrap();
    wait_for(
        &events,
        |e| matches!(e, SessionEvent::Done { cell_id, .. } if cell_id == "a"),
    )
    .await;
    wait_for(&events, |e| {
        matches!(
            e,
            SessionEvent::Output {
                rendered: Rendered::DataFrame { .. },
                ..
            }
        )
    })
    .await;
    wait_for(&events, |e| matches!(e, SessionEvent::Output { output: Output::Stream { text, .. }, .. } if text == "hi\n")).await;

    let vars = session.variables().await.unwrap();
    let sales = vars
        .iter()
        .find(|v| v.name == "sales")
        .expect("sales in the variables");
    assert!(sales.frame, "{sales:?}");
    let frame = session.frame("sales", 100).await.unwrap();
    assert_eq!(frame.rows, [["0", "North", "12"], ["1", "South", "7"]]);

    session
        .execute("b", "import time\ntime.sleep(30)")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    session.interrupt().await.unwrap();
    wait_for(&events, |e| matches!(e, SessionEvent::Done { cell_id, status, .. } if cell_id == "b" && status == "error")).await;
    wait_for(&events, |e| matches!(e, SessionEvent::Output { output: Output::Error { ename, .. }, .. } if ename == "KeyboardInterrupt")).await;

    session.shutdown().await.unwrap();
    let _ = child.wait();
}
