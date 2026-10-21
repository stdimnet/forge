//! The Forge agent protocol: MessagePack frames over a byte stream (the agent's stdio).
//!
//! Every frame is a big-endian `u32` length followed by a MessagePack map
//! ([`rmp_serde::to_vec_named`], so fields are matched by name and old peers ignore new
//! ones). One connection multiplexes:
//!
//! - **calls**: [`Frame::Request`] with an id, answered by [`Frame::Response`] with the same
//!   id; answers may arrive in any order. The result is the MessagePack encoding of the
//!   call's reply type (see the comments on [`Call`]).
//! - **events** from the agent: [`Frame::Event`] (file changes, terminal and process output,
//!   index progress).
//! - **byte channels**: [`Frame::Data`] / [`Frame::Close`] carry a language server's stdio
//!   (and terminal / process input) under a channel id chosen by the client.
//!
//! The connection starts with [`Call::Hello`]; the agent refuses other protocol versions.

use std::path::PathBuf;

use forge_core::search::{FindRequest, TodoRequest};
use forge_core::{CollectRequest, FsChanges, Result};
use forge_process::{CommandLine, PtyEvent, RunEvent};
use forge_vcs::GitRequest;
use phpforge_lang_php::generate::{GenerateKind, GenerateOptions};
use phpforge_lang_php::index::SearchKind;
use phpforge_lang_php::lsp::PhpLspSettings;
use phpforge_lang_php::navigate::NavKind;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::frameworks::FrameworkCall;
use crate::langservers::{InstallRequest, LangServersRequest};
use crate::web::WebCall;
use crate::TerminalSpec;

/// Bumped on incompatible protocol changes; client and agent must agree.
pub const PROTOCOL_VERSION: u32 = 6;

/// Frames larger than this are refused (protects both sides from garbage input).
pub const MAX_FRAME: usize = 64 * 1024 * 1024;

/// One unit on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Frame {
    Request {
        id: u64,
        call: Call,
    },
    Response {
        id: u64,
        result: std::result::Result<serde_bytes::ByteBuf, String>,
    },
    Event(Event),
    Data {
        channel: u32,
        data: serde_bytes::ByteBuf,
    },
    Close {
        channel: u32,
    },
}

/// Requests from the client to the agent. The reply type of each call is noted; `()` for
/// calls that only report success.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Call {
    /// -> [`Hello`]
    Hello {
        version: String,
        protocol: u32,
    },
    /// -> `()`; measures latency.
    Ping,
    /// List an absolute directory (`~` = home) for the remote folder chooser -> [`BrowseResult`]
    Browse {
        path: String,
    },
    /// Open the project folder; later calls work on it -> [`Opened`]
    Open {
        root: String,
    },
    /// -> `Vec<FileEntry>`
    ListDir {
        path: String,
    },
    /// -> `String`
    ReadFile {
        path: String,
    },
    /// -> `Option<String>` (`None` when the file does not exist)
    ReadOptional {
        path: String,
    },
    /// -> `()`
    WriteFile {
        path: String,
        contents: String,
    },
    /// -> `ByteBuf`: the raw bytes (files in other encodings than UTF-8). Protocol 4.
    ReadBytes {
        path: String,
    },
    /// -> `()`. Protocol 4.
    WriteBytes {
        path: String,
        bytes: serde_bytes::ByteBuf,
    },
    /// -> `()`
    CreateFile {
        path: String,
    },
    /// -> `()`
    CreateDir {
        path: String,
    },
    /// -> `()`
    Rename {
        from: String,
        to: String,
    },
    /// -> `()`
    Delete {
        path: String,
    },
    /// -> `Vec<FileMatch>`
    FindFiles {
        query: String,
        limit: usize,
        #[serde(default)]
        exact: bool,
    },
    /// Find in Files -> `FindResult`
    FindInFiles(FindRequest),
    /// Find in Files with the result kept on the agent for paging -> `FindPaged` (first rows)
    FindStart {
        req: FindRequest,
        page_size: usize,
    },
    /// Rows `[offset, offset + count)` of the result `FindStart` returned `id` for
    /// -> `Vec<FileMatches>`
    FindPage {
        id: u64,
        offset: usize,
        count: usize,
    },
    /// TODO tool window -> `TodoResult` (protocol 4)
    FindTodos(TodoRequest),
    /// The text of the project's files with some extensions (language indexes) ->
    /// `Vec<SourceFile>`
    CollectFiles(CollectRequest),
    /// Run git on the agent's machine -> `GitOutput`
    Git(GitRequest),
    /// -> `ProjectInfo`
    Detect,
    /// Start sending [`Event::FsChanged`] for the project -> `()`
    Watch,
    /// Start a shell; output arrives as [`Event::Pty`], input as [`Frame::Data`] on
    /// `channel` -> [`TerminalOpened`]
    TerminalOpen {
        channel: u32,
        spec: TerminalSpec,
    },
    /// -> `()`
    TerminalResize {
        channel: u32,
        cols: u16,
        rows: u16,
    },
    /// Start a process; output arrives as [`Event::Run`], stdin as [`Frame::Data`] -> `()`
    ProcessStart {
        channel: u32,
        cmd: CommandLine,
    },
    /// -> `()`
    ProcessKill {
        channel: u32,
        grace_ms: u64,
    },
    /// End of the process' input -> `()`
    ProcessCloseStdin {
        channel: u32,
    },
    /// Look for an executable on the host -> `Option<PathBuf>`
    FindExecutable {
        names: Vec<String>,
        custom: Option<String>,
    },
    /// Laravel / Symfony / Doctrine analysis on the host -> the call's reply type
    Framework(FrameworkCall),
    /// Plan the catalog language servers on the host -> `LangServersAnswer`
    LangServers(LangServersRequest),
    /// Start a language server in the project root; its stdio is `channel` -> `()`
    LspStart {
        channel: u32,
        command: String,
        args: Vec<String>,
    },
    Php(PhpCall),
    /// Classes or symbols matching `query` -> `Vec<SymbolHit>`
    FindSymbols {
        query: String,
        types_only: bool,
        limit: usize,
        skip_extensions: Vec<String>,
        #[serde(default)]
        exact: bool,
    },
    /// State of the project's search index -> `SearchStatus`
    SearchStatus,
    /// npm tool window and React / Vue / Angular analysis on the host -> the call's reply
    /// type (see [`WebCall`])
    Web(WebCall),
    /// Install a language server on the host (Node.js and npm packages into the host's
    /// Forge directories). Steps and progress arrive as [`Event::Install`] on `channel`;
    /// closing the channel cancels it -> `InstallOutcome`
    LangServerInstall {
        channel: u32,
        request: InstallRequest,
    },
    /// Newer versions of the npm servers Forge installed on the host ->
    /// `Option<Vec<PackageUpdate>>`
    LangServerUpdates {
        registry: Option<String>,
        node_mirror: Option<String>,
        force: bool,
    },
}

/// PHP index queries (see [`crate::PhpBackend`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PhpCall {
    /// Progress arrives as [`Event::PhpIndexProgress`] -> `IndexStats`
    Index,
    /// -> `Vec<SymbolHit>`
    Search {
        query: String,
        kind: SearchKind,
        limit: usize,
        #[serde(default)]
        exact: bool,
    },
    /// -> `FileStructure`
    Structure {
        path: String,
        content: Option<String>,
        inherited: bool,
    },
    /// -> `Option<Hover>`
    Hover {
        path: String,
        offset: usize,
        content: Option<String>,
    },
    /// -> `GenerateTargets`
    GenerateTargets {
        path: String,
        offset: usize,
        content: Option<String>,
    },
    /// -> `Vec<TextEdit>`
    Generate {
        path: String,
        offset: usize,
        kind: GenerateKind,
        options: GenerateOptions,
        content: Option<String>,
    },
    /// -> `Option<PhpUsages>`
    Usages {
        path: String,
        offset: usize,
        content: Option<String>,
    },
    /// -> `Vec<NavTarget>`
    Navigate {
        path: String,
        offset: usize,
        kind: NavKind,
        content: Option<String>,
    },
    /// -> `PhpDefinition`
    Definition {
        path: String,
        offset: usize,
        content: Option<String>,
    },
    /// -> `Vec<PhpLineMarker>`
    LineMarkers {
        path: String,
        content: Option<String>,
    },
    /// -> `Vec<Intention>`
    Intentions {
        path: String,
        offset: usize,
        content: Option<String>,
    },
    /// -> `Option<LspServerConfig>`
    LspConfig { settings: PhpLspSettings },
    /// -> `DetectedServers`
    DetectServers { settings: PhpLspSettings },
}

/// Pushed by the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    FsChanged(FsChanges),
    Pty {
        channel: u32,
        event: PtyEvent,
    },
    Run {
        channel: u32,
        event: RunEvent,
    },
    PhpIndexProgress {
        done: usize,
        total: usize,
    },
    /// A step, download progress or output line of [`Call::LangServerInstall`].
    Install {
        channel: u32,
        event: forge_langservers::managed::Event,
    },
}

/// Reply to [`Call::Hello`]: who and where the agent is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub version: String,
    pub protocol: u32,
    /// `linux`, `macos`, ...
    pub os: String,
    /// `x86_64`, `aarch64`, ...
    pub arch: String,
    pub home: String,
    pub hostname: String,
}

/// Reply to [`Call::Open`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    /// The canonical root.
    pub root: PathBuf,
    pub name: String,
}

/// One entry of [`BrowseResult`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseEntry {
    pub name: String,
    /// Absolute path.
    pub path: String,
    pub dir: bool,
    /// Holds a project marker (`composer.json`, `package.json`, `.git`, ...).
    #[serde(default)]
    pub project: bool,
}

/// Reply to [`Call::Browse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseResult {
    /// The absolute directory that was listed (`~` expanded, `..` removed).
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<BrowseEntry>,
}

/// Reply to [`Call::TerminalOpen`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOpened {
    pub shell: String,
    pub cwd: PathBuf,
}

fn codec(e: impl std::fmt::Display) -> forge_core::ForgeError {
    forge_core::ForgeError::Other(format!("agent protocol: {e}"))
}

/// Encode a value as a call result payload.
pub fn encode_value<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(value).map_err(codec)
}

/// Decode a call result payload.
pub fn decode_value<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    rmp_serde::from_slice(bytes).map_err(codec)
}

/// A frame with its length prefix, ready to write.
pub fn encode_frame(frame: &Frame) -> Result<Vec<u8>> {
    let body = rmp_serde::to_vec_named(frame).map_err(codec)?;
    if body.len() > MAX_FRAME {
        return Err(codec(format!("frame of {} bytes is too large", body.len())));
    }
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Splits a byte stream into frames; feed it whatever arrives.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// The next complete frame, `Ok(None)` when more bytes are needed.
    pub fn next_frame(&mut self) -> Result<Option<Frame>> {
        if self.buf.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        if len > MAX_FRAME {
            return Err(codec(format!("frame of {len} bytes is too large")));
        }
        if self.buf.len() < 4 + len {
            return Ok(None);
        }
        let frame = rmp_serde::from_slice(&self.buf[4..4 + len]).map_err(codec);
        self.buf.drain(..4 + len);
        frame.map(Some)
    }
}

/// Async framing over tokio streams.
pub mod io {
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

    use super::*;

    /// Read one frame; `Ok(None)` at a clean end of stream.
    pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Option<Frame>> {
        let mut len = [0u8; 4];
        match reader.read_exact(&mut len).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(codec(e)),
        }
        let len = u32::from_be_bytes(len) as usize;
        if len > MAX_FRAME {
            return Err(codec(format!("frame of {len} bytes is too large")));
        }
        let mut body = vec![0u8; len];
        reader.read_exact(&mut body).await.map_err(codec)?;
        rmp_serde::from_slice(&body).map(Some).map_err(codec)
    }

    pub async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, frame: &Frame) -> Result<()> {
        let bytes = encode_frame(frame)?;
        writer.write_all(&bytes).await.map_err(codec)?;
        writer.flush().await.map_err(codec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_core::{FileEntry, FileKind};
    use serde_bytes::ByteBuf;

    fn roundtrip(frame: Frame) {
        let bytes = encode_frame(&frame).unwrap();
        let mut dec = FrameDecoder::new();
        // Feed byte by byte: frames may be split anywhere.
        let mut out = Vec::new();
        for b in &bytes {
            dec.push(std::slice::from_ref(b));
            if let Some(f) = dec.next_frame().unwrap() {
                out.push(f);
            }
        }
        assert_eq!(out, vec![frame]);
    }

    #[test]
    fn frames_round_trip() {
        roundtrip(Frame::Request {
            id: 7,
            call: Call::Hello {
                version: "0.1.0".into(),
                protocol: PROTOCOL_VERSION,
            },
        });
        roundtrip(Frame::Request {
            id: u64::MAX,
            call: Call::Git(GitRequest {
                cwd: "/var/www/shop".into(),
                args: vec!["status".into(), "--porcelain=v2".into()],
                stdin: Some("message ü".into()),
                non_interactive: true,
            }),
        });
        roundtrip(Frame::Request {
            id: 1,
            call: Call::Php(PhpCall::Generate {
                path: "app/User.php".into(),
                offset: 42,
                kind: GenerateKind::Constructor,
                options: GenerateOptions::default(),
                content: None,
            }),
        });
        roundtrip(Frame::Request {
            id: 2,
            call: Call::TerminalOpen {
                channel: 3,
                spec: TerminalSpec {
                    shell: None,
                    args: None,
                    cwd: "/srv".into(),
                    env: vec![("A".into(), "b".into())],
                    cols: 80,
                    rows: 24,
                    shell_integration: true,
                },
            },
        });
        roundtrip(Frame::Request {
            id: 3,
            call: Call::Framework(FrameworkCall::Symfony {
                config: phpforge_fw_symfony::Config::default(),
                query: crate::frameworks::SymfonyQuery::Complete {
                    path: "templates/a.html.twig".into(),
                    content: "{{ path('".into(),
                    offset: 9,
                },
            }),
        });
        roundtrip(Frame::Request {
            id: 4,
            call: Call::FindExecutable {
                names: vec!["claude".into()],
                custom: None,
            },
        });
        roundtrip(Frame::Request {
            id: 4,
            call: Call::CollectFiles(CollectRequest {
                extensions: vec!["lua".into()],
                max_files: 10,
                max_file_size: 1024,
                max_total: 4096,
            }),
        });
        roundtrip(Frame::Request {
            id: 5,
            call: Call::Web(WebCall::Gutter {
                framework: "vue".into(),
                path: "src/App.vue".into(),
                content: "<template><Card/></template>".into(),
            }),
        });
        roundtrip(Frame::Request {
            id: 6,
            call: Call::LangServerInstall {
                channel: 12,
                request: InstallRequest {
                    target: crate::langservers::InstallTarget::Server {
                        id: "intelephense".into(),
                        index: 0,
                    },
                    registry: None,
                    node_mirror: Some("https://mirror.example/node".into()),
                },
            },
        });
        roundtrip(Frame::Event(Event::Install {
            channel: 12,
            event: forge_langservers::managed::Event::progress("node.tar.xz", 10, Some(100)),
        }));
        roundtrip(Frame::Event(Event::Install {
            channel: 12,
            event: forge_langservers::managed::Event::Output {
                stream: forge_langservers::install::Stream::Stderr,
                line: "npm warn".into(),
            },
        }));
        roundtrip(Frame::Response {
            id: 7,
            result: Err("no such file".into()),
        });
        roundtrip(Frame::Event(Event::FsChanged(FsChanges {
            changed: vec!["a.php".into()],
            removed: vec![],
            git: true,
        })));
        roundtrip(Frame::Event(Event::Run {
            channel: 9,
            event: RunEvent::Exited {
                code: Some(0),
                signal: None,
            },
        }));
        roundtrip(Frame::Data {
            channel: 4,
            data: ByteBuf::from(vec![0, 159, 146, 150, 255]),
        });
        roundtrip(Frame::Close { channel: 4 });
    }

    #[test]
    fn several_frames_in_one_chunk() {
        let mut bytes = encode_frame(&Frame::Close { channel: 1 }).unwrap();
        bytes.extend(encode_frame(&Frame::Close { channel: 2 }).unwrap());
        let mut dec = FrameDecoder::new();
        dec.push(&bytes);
        assert_eq!(dec.next_frame().unwrap(), Some(Frame::Close { channel: 1 }));
        assert_eq!(dec.next_frame().unwrap(), Some(Frame::Close { channel: 2 }));
        assert_eq!(dec.next_frame().unwrap(), None);
    }

    #[test]
    fn reply_payloads_round_trip() {
        let entries = vec![FileEntry {
            name: "vendor".into(),
            path: "vendor".into(),
            kind: FileKind::Dir,
            ignored: true,
        }];
        let back: Vec<FileEntry> = decode_value(&encode_value(&entries).unwrap()).unwrap();
        assert_eq!(back[0].path, "vendor");
        assert!(back[0].ignored);
        let unit: () = decode_value(&encode_value(&()).unwrap()).unwrap();
        assert_eq!(unit, ());
    }

    #[test]
    fn oversized_frames_are_refused() {
        let mut dec = FrameDecoder::new();
        dec.push(&(u32::MAX).to_be_bytes());
        assert!(dec.next_frame().is_err());
    }
}
