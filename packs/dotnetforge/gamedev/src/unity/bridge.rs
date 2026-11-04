//! The Forge editor bridge: the `dev.forge.unity` package (C# source in
//! `unity-package/`, installed by [`super::package`]) listens on a loopback TCP port inside
//! the Unity editor, and Forge connects to it. Rider's Unity plugin works the same way.
//!
//! Protocol (version [`PROTOCOL`], spec in `docs/guide/developer/unity-bridge-protocol.md`):
//!
//! - The package writes `Library/ForgeBridge.json` (`{ port, pid, token, protocol,
//!   unityVersion, projectPath }`) while it listens on `127.0.0.1:<port>`. The token is
//!   random per editor session; only processes of the user who can read the project's
//!   `Library` folder know it.
//! - Messages are JSON objects, one per line (UTF-8, `\n`), at most 1 MiB each.
//! - Forge sends `{"type":"hello","token","client":"Forge","protocol":1}` first; the
//!   editor answers `{"type":"welcome","protocol","unityVersion","projectPath","pid",
//!   "playMode","compiling"}` or `{"type":"error","message"}` and closes.
//! - Commands from Forge carry an `id`: `play`, `pause`, `resume`, `stop`, `step`,
//!   `refresh` (AssetDatabase.Refresh), `syncSolution` (regenerate the `.sln` / `.csproj`
//!   files); the editor answers `{"type":"ack","id","ok","message"?}`.
//! - Events from the editor: `{"type":"state","playMode":"edit"|"playing"|"paused",
//!   "compiling"}`, `{"type":"log","level":"log"|"warning"|"error"|"exception"|"assert",
//!   "message","stackTrace","time"?}` and `{"type":"openFile","path","line","column"}`
//!   (a script double-clicked in Unity while Forge is the external editor).
//!
//! [`BridgeClient`] is Forge's side; tests drive it against a fake Unity peer.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};

pub const PROTOCOL: u32 = 1;
/// The longest message either side accepts.
pub const MAX_MESSAGE: usize = 1 << 20;
const ACK_TIMEOUT: Duration = Duration::from_secs(15);

/// `Library/ForgeBridge.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeInstance {
    pub port: u16,
    #[serde(default)]
    pub pid: u32,
    pub token: String,
    #[serde(default)]
    pub protocol: u32,
    #[serde(default)]
    pub unity_version: String,
    #[serde(default)]
    pub project_path: String,
}

pub fn parse_instance(text: &str) -> Option<BridgeInstance> {
    serde_json::from_str::<BridgeInstance>(text.trim_start_matches('\u{feff}'))
        .ok()
        .filter(|i| i.port != 0 && !i.token.is_empty())
}

/// A command for the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    Play,
    Pause,
    Resume,
    Stop,
    Step,
    Refresh,
    SyncSolution,
}

impl Command {
    pub fn wire_name(self) -> &'static str {
        match self {
            Command::Play => "play",
            Command::Pause => "pause",
            Command::Resume => "resume",
            Command::Stop => "stop",
            Command::Step => "step",
            Command::Refresh => "refresh",
            Command::SyncSolution => "syncSolution",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayMode {
    #[default]
    Edit,
    Playing,
    Paused,
}

fn play_mode(v: &Value) -> PlayMode {
    match v.get("playMode").and_then(Value::as_str) {
        Some("playing") => PlayMode::Playing,
        Some("paused") => PlayMode::Paused,
        _ => PlayMode::Edit,
    }
}

/// The editor's answer to `hello`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Welcome {
    pub protocol: u32,
    pub unity_version: String,
    pub project_path: String,
    pub pid: u32,
    pub play_mode: PlayMode,
    pub compiling: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    #[default]
    Log,
    Warning,
    Error,
    Exception,
    Assert,
}

impl LogLevel {
    pub fn is_error(self) -> bool {
        matches!(
            self,
            LogLevel::Error | LogLevel::Exception | LogLevel::Assert
        )
    }
}

/// A source position, project-relative when it is inside the project (`Assets/…`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLocation {
    pub path: String,
    /// 1-based.
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

/// A message of Unity's console.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stack_trace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// Where the message points: a compiler message's file, else the first stack frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<SourceLocation>,
}

/// What the editor reports while connected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BridgeEvent {
    #[serde(rename_all = "camelCase")]
    State {
        play_mode: PlayMode,
        compiling: bool,
    },
    Log {
        entry: LogEntry,
    },
    OpenFile {
        path: String,
        line: u32,
        column: u32,
    },
    /// The connection ended (the editor quit or reloaded its scripts).
    Disconnected {
        reason: String,
    },
}

/// The editor's answer to a command.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ack {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// One message from the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    Welcome(Welcome),
    Event(BridgeEvent),
    Ack {
        id: u64,
        ack: Ack,
    },
    Error(String),
    /// A message of a newer protocol; ignored.
    Unknown,
}

fn str_of(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn u32_of(v: &Value, key: &str) -> u32 {
    v.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0)
}

/// `Assets/Scripts/Player.cs(12,5): error CS1002: ; expected`.
pub fn compiler_location(message: &str) -> Option<SourceLocation> {
    let first = message.lines().next()?;
    let open = first.find(".cs(")? + 3;
    let close = open + first[open..].find(')')?;
    let mut nums = first[open + 1..close].split(',');
    let line = nums.next()?.trim().parse().ok()?;
    let column = nums.next().and_then(|c| c.trim().parse().ok());
    first[close..].starts_with("):").then(|| SourceLocation {
        path: crate::util::slash(&first[..open]),
        line,
        column,
    })
}

/// The location of a console message.
pub fn log_location(message: &str, stack_trace: &str) -> Option<SourceLocation> {
    compiler_location(message).or_else(|| {
        forge_output::formats::nunit::frame_location(stack_trace).map(|(path, line)| {
            SourceLocation {
                path: crate::util::slash(&path),
                line,
                column: None,
            }
        })
    })
}

/// Parse one line from the editor.
pub fn parse_message(line: &str) -> Result<Incoming, String> {
    let v: Value = serde_json::from_str(line.trim()).map_err(|e| format!("bad message: {e}"))?;
    let kind = v.get("type").and_then(Value::as_str).unwrap_or_default();
    Ok(match kind {
        "welcome" => Incoming::Welcome(Welcome {
            protocol: u32_of(&v, "protocol"),
            unity_version: str_of(&v, "unityVersion"),
            project_path: str_of(&v, "projectPath"),
            pid: u32_of(&v, "pid"),
            play_mode: play_mode(&v),
            compiling: v.get("compiling").and_then(Value::as_bool).unwrap_or(false),
        }),
        "state" => Incoming::Event(BridgeEvent::State {
            play_mode: play_mode(&v),
            compiling: v.get("compiling").and_then(Value::as_bool).unwrap_or(false),
        }),
        "log" => {
            let message = str_of(&v, "message");
            let stack_trace = str_of(&v, "stackTrace");
            let level = match v.get("level").and_then(Value::as_str) {
                Some("warning") => LogLevel::Warning,
                Some("error") => LogLevel::Error,
                Some("exception") => LogLevel::Exception,
                Some("assert") => LogLevel::Assert,
                _ => LogLevel::Log,
            };
            Incoming::Event(BridgeEvent::Log {
                entry: LogEntry {
                    level,
                    location: log_location(&message, &stack_trace),
                    message,
                    stack_trace,
                    time: v.get("time").and_then(Value::as_str).map(str::to_string),
                },
            })
        }
        "openFile" => Incoming::Event(BridgeEvent::OpenFile {
            path: crate::util::slash(&str_of(&v, "path")),
            line: u32_of(&v, "line"),
            column: u32_of(&v, "column"),
        }),
        "ack" => Incoming::Ack {
            id: v.get("id").and_then(Value::as_u64).unwrap_or(0),
            ack: Ack {
                ok: v.get("ok").and_then(Value::as_bool).unwrap_or(false),
                message: v.get("message").and_then(Value::as_str).map(str::to_string),
            },
        },
        "error" => Incoming::Error(str_of(&v, "message")),
        _ => Incoming::Unknown,
    })
}

/// The `hello` line Forge sends first.
pub fn hello(token: &str) -> String {
    json!({"type": "hello", "token": token, "client": "Forge", "protocol": PROTOCOL}).to_string()
}

/// A command line with its id.
pub fn command_message(id: u64, command: Command) -> String {
    json!({"type": command.wire_name(), "id": id}).to_string()
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Ack>>>>;

/// Read one `\n`-terminated message of at most [`MAX_MESSAGE`] bytes; `None` at the end.
async fn read_line<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<String>, String> {
    let mut buf = Vec::new();
    let n = reader
        .take(MAX_MESSAGE as u64 + 1)
        .read_until(b'\n', &mut buf)
        .await
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Ok(None);
    }
    if buf.len() > MAX_MESSAGE {
        return Err("message too long".into());
    }
    Ok(Some(String::from_utf8_lossy(&buf).trim_end().to_string()))
}

/// Forge's side of a connection to the editor.
pub struct BridgeClient {
    out: mpsc::UnboundedSender<Option<String>>,
    pending: Pending,
    closed: Arc<AtomicBool>,
    next_id: AtomicU64,
    pub welcome: Welcome,
}

impl BridgeClient {
    /// Connect to the bridge at `address` (`127.0.0.1:<port>`) with the instance's token.
    /// Events arrive on the returned receiver until [`BridgeEvent::Disconnected`].
    pub async fn connect(
        address: &str,
        token: &str,
        timeout: Duration,
    ) -> Result<(BridgeClient, mpsc::UnboundedReceiver<BridgeEvent>), String> {
        let stream = tokio::time::timeout(timeout, TcpStream::connect(address))
            .await
            .map_err(|_| format!("Unity did not answer on {address}"))?
            .map_err(|e| format!("Cannot connect to Unity on {address}: {e}"))?;
        let _ = stream.set_nodelay(true);
        let (read, mut write) = stream.into_split();
        let mut reader = BufReader::new(read);
        write
            .write_all(format!("{}\n", hello(token)).as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        let welcome = tokio::time::timeout(timeout, async {
            loop {
                let Some(line) = read_line(&mut reader).await? else {
                    return Err("Unity closed the connection".to_string());
                };
                if line.is_empty() {
                    continue;
                }
                match parse_message(&line)? {
                    Incoming::Welcome(w) => return Ok(w),
                    Incoming::Error(m) => return Err(format!("Unity refused the connection: {m}")),
                    _ => continue,
                }
            }
        })
        .await
        .map_err(|_| "Unity did not answer the handshake".to_string())??;
        // A newer bridge stays compatible: messages Forge does not know are ignored.

        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Option<String>>();
        tokio::spawn(async move {
            while let Some(Some(line)) = out_rx.recv().await {
                if write
                    .write_all(format!("{line}\n").as_bytes())
                    .await
                    .is_err()
                {
                    break;
                }
            }
            let _ = write.shutdown().await;
        });

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let (ev_tx, ev_rx) = mpsc::unbounded_channel();
        let p = pending.clone();
        let done = closed.clone();
        let stop_writer = out_tx.clone();
        tokio::spawn(async move {
            let reason = loop {
                match read_line(&mut reader).await {
                    Ok(None) => break "Unity closed the connection".to_string(),
                    Err(e) => break e,
                    Ok(Some(line)) if line.is_empty() => continue,
                    Ok(Some(line)) => match parse_message(&line) {
                        Ok(Incoming::Event(e)) => {
                            let _ = ev_tx.send(e);
                        }
                        Ok(Incoming::Ack { id, ack }) => {
                            if let Some(tx) = p.lock().expect("bridge acks").remove(&id) {
                                let _ = tx.send(ack);
                            }
                        }
                        Ok(Incoming::Error(m)) => break m,
                        Ok(_) | Err(_) => continue,
                    },
                }
            };
            // Unanswered commands fail when their senders drop.
            done.store(true, Ordering::SeqCst);
            p.lock().expect("bridge acks").clear();
            let _ = stop_writer.send(None);
            let _ = ev_tx.send(BridgeEvent::Disconnected { reason });
        });

        Ok((
            BridgeClient {
                out: out_tx,
                pending,
                closed,
                next_id: AtomicU64::new(1),
                welcome,
            },
            ev_rx,
        ))
    }

    /// Send a command and wait for the editor's answer.
    pub async fn send(&self, command: Command) -> Result<Ack, String> {
        if !self.is_open() {
            return Err("Not connected to Unity".into());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().expect("bridge acks").insert(id, tx);
        // The reader may have ended in between (it sets `closed`, then drops waiting acks).
        if self.closed.load(Ordering::SeqCst)
            || self.out.send(Some(command_message(id, command))).is_err()
        {
            self.pending.lock().expect("bridge acks").remove(&id);
            return Err("Not connected to Unity".into());
        }
        match tokio::time::timeout(ACK_TIMEOUT, rx).await {
            Ok(Ok(ack)) => Ok(ack),
            Ok(Err(_)) => Err("The connection to Unity was closed".into()),
            Err(_) => {
                self.pending.lock().expect("bridge acks").remove(&id);
                Err(format!(
                    "Unity did not answer '{}' (is it compiling or in a modal dialog?)",
                    command.wire_name()
                ))
            }
        }
    }

    /// Close the connection (the editor keeps running).
    pub fn close(&self) {
        let _ = self.out.send(None);
    }

    pub fn is_open(&self) -> bool {
        !self.closed.load(Ordering::SeqCst) && !self.out.is_closed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_messages_and_locations() {
        let w = parse_message(r#"{"type":"welcome","protocol":1,"unityVersion":"2022.3.10f1","projectPath":"/p","pid":7,"playMode":"paused","compiling":true}"#).unwrap();
        assert_eq!(
            w,
            Incoming::Welcome(Welcome {
                protocol: 1,
                unity_version: "2022.3.10f1".into(),
                project_path: "/p".into(),
                pid: 7,
                play_mode: PlayMode::Paused,
                compiling: true
            })
        );
        let Incoming::Event(BridgeEvent::Log { entry }) = parse_message(
            r#"{"type":"log","level":"error","message":"Assets/Scripts/Player.cs(12,5): error CS1002: ; expected","stackTrace":""}"#,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(
            entry.location,
            Some(SourceLocation {
                path: "Assets/Scripts/Player.cs".into(),
                line: 12,
                column: Some(5)
            })
        );
        let Incoming::Event(BridgeEvent::Log { entry }) = parse_message(
            r#"{"type":"log","level":"exception","message":"NullReferenceException","stackTrace":"Player.Update () (at Assets/Scripts/Player.cs:30)\n"}"#,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(entry.location.unwrap().line, 30);
        assert!(LogLevel::Exception.is_error());
        assert_eq!(
            parse_message(r#"{"type":"ack","id":3,"ok":false,"message":"busy"}"#).unwrap(),
            Incoming::Ack {
                id: 3,
                ack: Ack {
                    ok: false,
                    message: Some("busy".into())
                }
            }
        );
        assert_eq!(
            parse_message(r#"{"type":"future"}"#).unwrap(),
            Incoming::Unknown
        );
        assert!(parse_message("nope").is_err());
        assert_eq!(
            command_message(4, Command::SyncSolution),
            r#"{"id":4,"type":"syncSolution"}"#
        );
        assert!(compiler_location("Player.cs(1,2) something").is_none());
        let e = serde_json::to_value(BridgeEvent::State {
            play_mode: PlayMode::Playing,
            compiling: false,
        })
        .unwrap();
        assert_eq!(
            e,
            json!({"type": "state", "playMode": "playing", "compiling": false})
        );
    }

    #[test]
    fn instance_file() {
        let i = parse_instance(r#"{"port": 51234, "pid": 99, "token": "abc", "protocol": 1, "unityVersion": "2022.3.10f1", "projectPath": "/p"}"#).unwrap();
        assert_eq!((i.port, i.token.as_str()), (51234, "abc"));
        assert!(parse_instance(r#"{"port": 0, "token": "abc"}"#).is_none());
        assert!(parse_instance(r#"{"port": 1, "token": ""}"#).is_none());
    }
}
