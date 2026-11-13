//! Where the open project lives for the Services tool window: its compose files and
//! Dockerfiles are read there, and `docker compose` / `docker build` run there. A local
//! project is a folder here ([`LocalProject`]); the app implements [`ProjectHost`] for a
//! remote project through its backend (files and processes on the host, the Docker socket
//! of the host through the project's SSH connection).

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use crate::http::BoxIo;
use crate::logs::LogStream;
use crate::{err, Result};

/// Output of a command run with [`ProjectHost::run`].
pub type OutputSink<'a> = Box<dyn FnMut(LogStream, String) + Send + 'a>;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The machine and folder of the open project.
pub trait ProjectHost: Send + Sync {
    /// Absolute project root on the machine that has the files.
    fn root(&self) -> &Path;

    /// `user@host` when the project is on a remote host.
    fn remote(&self) -> Option<&str> {
        None
    }

    /// A project file's text (`rel` is relative to the root, `/`-separated); `None` when it
    /// does not exist. Blocks (call off the async runtime).
    fn read(&self, rel: &str) -> Option<String>;

    /// Names of the folders directly in the project root. Blocks.
    fn dirs(&self) -> Vec<String>;

    /// Run `program args…` in `cwd` (absolute, on the project's machine), sending output
    /// to `sink`. Returns the exit code; dropping the future stops the process.
    fn run<'a>(
        &'a self,
        program: &'a str,
        args: &'a [String],
        env: &'a [(String, String)],
        cwd: &'a Path,
        sink: OutputSink<'a>,
    ) -> BoxFuture<'a, Result<Option<i32>>>;

    /// A stream to a Unix socket on the project's machine (the Docker daemon of a remote
    /// project's host, through its SSH connection).
    fn open_socket(&self, path: &str) -> BoxFuture<'static, Result<BoxIo>> {
        let path = path.to_string();
        Box::pin(async move { err(format!("{path} is not on a remote host")) })
    }

    /// A TCP connection to `127.0.0.1:port` as the project's host sees it, through its SSH
    /// connection (`kubectl port-forward` running on the host).
    fn open_tcp(&self, port: u16) -> BoxFuture<'static, Result<BoxIo>> {
        Box::pin(async move { err(format!("127.0.0.1:{port} is not on a remote host")) })
    }
}

/// A project folder on this machine.
#[derive(Debug, Clone)]
pub struct LocalProject {
    root: PathBuf,
}

impl LocalProject {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl ProjectHost for LocalProject {
    fn root(&self) -> &Path {
        &self.root
    }

    fn read(&self, rel: &str) -> Option<String> {
        let path = self.root.join(rel);
        path.is_file()
            .then(|| std::fs::read_to_string(path).ok())
            .flatten()
    }

    fn dirs(&self) -> Vec<String> {
        std::fs::read_dir(&self.root)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn run<'a>(
        &'a self,
        program: &'a str,
        args: &'a [String],
        env: &'a [(String, String)],
        cwd: &'a Path,
        sink: OutputSink<'a>,
    ) -> BoxFuture<'a, Result<Option<i32>>> {
        Box::pin(crate::cli::run_streaming(program, args, env, cwd, sink))
    }
}
