//! A tiny in-process FTP server on 127.0.0.1 for the tests: passive mode, `MLSD` (or only
//! `LIST`), `STOR`, `RETR`, `MKD`, `RMD`, `DELE`, `MFMT`, backed by a temporary folder.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

pub struct MockFtp {
    pub port: u16,
    pub root: PathBuf,
    pub stores: Arc<AtomicUsize>,
}

#[derive(Clone)]
pub struct FtpOptions {
    pub user: &'static str,
    pub password: &'static str,
    pub mlsd: bool,
    pub mfmt: bool,
}

impl Default for FtpOptions {
    fn default() -> Self {
        Self {
            user: "deploy",
            password: "secret",
            mlsd: true,
            mfmt: true,
        }
    }
}

pub fn start(root: &Path, options: FtpOptions) -> MockFtp {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let stores = Arc::new(AtomicUsize::new(0));
    let root_buf = root.to_path_buf();
    let counter = stores.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let root = root_buf.clone();
            let options = options.clone();
            let counter = counter.clone();
            std::thread::spawn(move || {
                let _ = session(stream, &root, &options, &counter);
            });
        }
    });
    MockFtp {
        port,
        root: root.to_path_buf(),
        stores,
    }
}

fn resolve(root: &Path, cwd: &str, arg: &str) -> (String, PathBuf) {
    let abs = if arg.starts_with('/') {
        arg.to_string()
    } else {
        format!("{}/{arg}", cwd.trim_end_matches('/'))
    };
    let parts: Vec<&str> = abs
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    assert!(!parts.contains(&".."), "the client must not send ..");
    let abs = format!("/{}", parts.join("/"));
    let mut path = root.to_path_buf();
    for p in parts {
        path.push(p);
    }
    (abs, path)
}

fn mtime(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn timestamp(secs: i64) -> String {
    forge_deploy::ftp::ftp_timestamp(secs)
}

fn parse_timestamp(ts: &str) -> Option<i64> {
    // YYYYMMDDHHMMSS → seconds (days from civil, Howard Hinnant).
    if ts.len() < 14 {
        return None;
    }
    let n = |a: usize, b: usize| ts[a..b].parse::<i64>().ok();
    let (y, m, d) = (n(0, 4)?, n(4, 6)?, n(6, 8)?);
    let (hh, mm, ss) = (n(8, 10)?, n(10, 12)?, n(12, 14)?);
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}

fn session(
    stream: TcpStream,
    root: &Path,
    o: &FtpOptions,
    stores: &AtomicUsize,
) -> std::io::Result<()> {
    let mut out = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    let mut cwd = "/".to_string();
    let mut user = String::new();
    let mut logged_in = false;
    let mut passive: Option<TcpListener> = None;
    writeln!(out, "220 Forge test FTP\r")?;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let text = line.trim_end_matches(['\r', '\n']);
        let (cmd, arg) = match text.split_once(' ') {
            Some((c, a)) => (c.to_ascii_uppercase(), a.to_string()),
            None => (text.to_ascii_uppercase(), String::new()),
        };
        let mut reply = |s: &str| writeln!(out, "{s}\r");
        if !logged_in && !matches!(cmd.as_str(), "USER" | "PASS" | "QUIT" | "AUTH" | "FEAT") {
            reply("530 Please log in")?;
            continue;
        }
        match cmd.as_str() {
            "USER" => {
                user = arg;
                reply("331 Password required")?;
            }
            "PASS" => {
                if user == o.user && arg == o.password {
                    logged_in = true;
                    reply("230 Logged in")?;
                } else {
                    reply("530 Login incorrect")?;
                }
            }
            "AUTH" => reply("502 TLS is not available")?,
            "FEAT" => {
                let mut features = vec!["211-Features:".to_string(), " SIZE".to_string()];
                if o.mlsd {
                    features.push(" MLST type*;size*;modify*;".to_string());
                }
                if o.mfmt {
                    features.push(" MFMT".to_string());
                }
                features.push("211 End".to_string());
                for f in features {
                    reply(&f)?;
                }
            }
            "TYPE" | "NOOP" | "OPTS" => reply("200 OK")?,
            "PWD" => reply(&format!("257 \"{cwd}\" is the current directory"))?,
            "CWD" => {
                let (abs, path) = resolve(root, &cwd, &arg);
                if path.is_dir() {
                    cwd = abs;
                    reply("250 OK")?;
                } else {
                    reply("550 No such directory")?;
                }
            }
            "PASV" => {
                let l = TcpListener::bind("127.0.0.1:0")?;
                let p = l.local_addr()?.port();
                passive = Some(l);
                reply(&format!(
                    "227 Entering Passive Mode (127,0,0,1,{},{})",
                    p >> 8,
                    p & 0xff
                ))?;
            }
            "MLSD" | "LIST" | "RETR" | "STOR" => {
                if cmd == "MLSD" && !o.mlsd {
                    reply("500 Unknown command")?;
                    continue;
                }
                let target = if cmd == "LIST" && arg.starts_with('-') {
                    String::new()
                } else {
                    arg.clone()
                };
                let (_, path) = resolve(root, &cwd, &target);
                let Some(listener) = passive.take() else {
                    reply("425 Use PASV first")?;
                    continue;
                };
                let missing = match cmd.as_str() {
                    "MLSD" | "LIST" => !path.is_dir(),
                    "RETR" => !path.is_file(),
                    _ => !path.parent().is_some_and(Path::is_dir),
                };
                let (mut data, _) = listener.accept()?;
                if missing {
                    drop(data);
                    reply("550 No such file or directory")?;
                    continue;
                }
                reply("150 Opening data connection")?;
                match cmd.as_str() {
                    "MLSD" | "LIST" => {
                        let mut entries: Vec<_> = std::fs::read_dir(&path)?.flatten().collect();
                        entries.sort_by_key(|e| e.file_name());
                        let mut text = String::new();
                        if cmd == "MLSD" {
                            text.push_str(&format!(
                                "type=cdir;modify={}; .\r\n",
                                timestamp(mtime(&std::fs::metadata(&path)?))
                            ));
                        }
                        for e in entries {
                            let meta = e.metadata()?;
                            let name = e.file_name().to_string_lossy().into_owned();
                            if cmd == "MLSD" {
                                let kind = if meta.is_dir() { "dir" } else { "file" };
                                text.push_str(&format!(
                                    "type={kind};size={};modify={}; {name}\r\n",
                                    meta.len(),
                                    timestamp(mtime(&meta))
                                ));
                            } else {
                                let kind = if meta.is_dir() { 'd' } else { '-' };
                                text.push_str(&format!(
                                    "{kind}rw-r--r-- 1 deploy deploy {} Jan 01 2024 {name}\r\n",
                                    meta.len()
                                ));
                            }
                        }
                        data.write_all(text.as_bytes())?;
                    }
                    "RETR" => data.write_all(&std::fs::read(&path)?)?,
                    _ => {
                        let mut bytes = Vec::new();
                        data.read_to_end(&mut bytes)?;
                        std::fs::write(&path, bytes)?;
                        stores.fetch_add(1, Ordering::SeqCst);
                    }
                }
                drop(data);
                reply("226 Transfer complete")?;
            }
            "MKD" => {
                let (abs, path) = resolve(root, &cwd, &arg);
                match std::fs::create_dir(&path) {
                    Ok(()) => reply(&format!("257 \"{abs}\" created"))?,
                    Err(_) => reply("550 Cannot create")?,
                }
            }
            "RMD" => {
                let (_, path) = resolve(root, &cwd, &arg);
                match std::fs::remove_dir(&path) {
                    Ok(()) => reply("250 Removed")?,
                    Err(_) => reply("550 Cannot remove")?,
                }
            }
            "DELE" => {
                let (_, path) = resolve(root, &cwd, &arg);
                match std::fs::remove_file(&path) {
                    Ok(()) => reply("250 Deleted")?,
                    Err(_) => reply("550 Cannot delete")?,
                }
            }
            "MFMT" if o.mfmt => {
                let (ts, file) = arg.split_once(' ').unwrap_or((&arg, ""));
                let (_, path) = resolve(root, &cwd, file);
                match parse_timestamp(ts) {
                    Some(secs) if path.exists() => {
                        let t = filetime::FileTime::from_unix_time(secs, 0);
                        filetime::set_file_mtime(&path, t)?;
                        reply(&format!("213 Modify={ts}; {file}"))?;
                    }
                    _ => reply("550 Cannot set time")?,
                }
            }
            "QUIT" => {
                reply("221 Bye")?;
                return Ok(());
            }
            _ => reply("502 Command not implemented")?,
        }
    }
}
