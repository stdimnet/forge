//! Free local ports for servers that Forge starts and then connects to: debug adapters
//! (`${port}`), the JVM's JDWP agent, debugpy, Jupyter kernels.
//!
//! Two rules keep Forge from talking to a stranger on such a port:
//!
//! - The ports lie below the ephemeral port ranges (Linux 32768-60999, macOS and Windows
//!   49152-65535). The system hands ephemeral ports to every program that binds port 0 or
//!   connects out, so on a busy machine a port Forge took from there can go to another
//!   program before the server binds it. The server then fails with "address in use" and
//!   Forge connects to that program instead.
//! - A port is checked by binding it, never by listening on it. A process that is spawned
//!   at the same moment (by Forge or by another test) inherits a copy of the probe socket
//!   until it executes its program. A copied listening socket keeps accepting connections
//!   after Forge closed the probe: the server cannot bind the port, Forge connects to the
//!   copy, and that connection is reset once the copy closes. A copy that is only bound
//!   refuses connections, and the server can still bind the port (address reuse).
//!
//! A free port is only free when it is checked: callers that start a server on it should
//! also notice when the server exits early, so they never keep talking to whoever answers.

use std::collections::HashSet;
use std::hash::{BuildHasher, RandomState};
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::ops::{Range, RangeInclusive};

use socket2::{Domain, Protocol, Socket, Type};

/// The ports handed out, below every system's default ephemeral range.
pub const PORTS: Range<u16> = 20000..32768;

/// Random tries per port before falling back to a port the system picks.
const ATTEMPTS: u32 = 64;

/// A free port on 127.0.0.1, outside the system's ephemeral range.
pub fn free_port() -> io::Result<u16> {
    free_ports(1).map(|p| p[0])
}

/// `n` distinct free ports on 127.0.0.1, outside the system's ephemeral range (for example
/// the five ports of a Jupyter kernel).
pub fn free_ports(n: usize) -> io::Result<Vec<u16>> {
    let ephemeral = ephemeral_ports();
    let random = RandomState::new();
    let span = u64::from(PORTS.end - PORTS.start);
    let mut ports = Vec::with_capacity(n);
    let mut seen = HashSet::new();
    let mut attempt = 0u64;
    while ports.len() < n {
        let budget = attempt + u64::from(ATTEMPTS);
        let mut found = None;
        while attempt < budget {
            let port = PORTS.start + (random.hash_one(attempt) % span) as u16;
            attempt += 1;
            if seen.contains(&port) || ephemeral.as_ref().is_some_and(|r| r.contains(&port)) {
                continue;
            }
            if let Ok(port) = probe(port) {
                found = Some(port);
                break;
            }
        }
        // Every try was taken (or the ephemeral range covers ours): let the system pick.
        let port = match found {
            Some(port) => port,
            None => loop {
                let port = probe(0)?;
                if !seen.contains(&port) {
                    break port;
                }
            },
        };
        seen.insert(port);
        ports.push(port);
    }
    Ok(ports)
}

/// Check that `port` is free by binding it on 127.0.0.1, without listening (port 0: a port
/// the system picks).
pub fn probe(port: u16) -> io::Result<u16> {
    let port = bind_probe(Ipv4Addr::LOCALHOST, port)?;
    // With SO_REUSEADDR, macOS and the BSDs let 127.0.0.1:P be bound while a server listens
    // on 0.0.0.0:P: that port is in use all the same.
    bind_probe(Ipv4Addr::UNSPECIFIED, port)?;
    Ok(port)
}

fn bind_probe(ip: Ipv4Addr, port: u16) -> io::Result<u16> {
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
    // Not on Windows: there the option lets a socket take over a port that is in use.
    #[cfg(unix)]
    socket.set_reuse_address(true)?;
    socket.bind(&SocketAddr::from((ip, port)).into())?;
    socket
        .local_addr()?
        .as_socket()
        .map(|a| a.port())
        .ok_or_else(|| io::Error::other("the probe socket has no IP address"))
}

/// The system's ephemeral port range, where it can be read (Linux).
pub fn ephemeral_ports() -> Option<RangeInclusive<u16>> {
    let text = std::fs::read_to_string("/proc/sys/net/ipv4/ip_local_port_range").ok()?;
    let mut ports = text.split_whitespace().map(|p| p.parse::<u16>().ok());
    Some(ports.next()??..=ports.next()??)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};

    /// Unless the system's ephemeral range was moved down over these ports.
    fn ours_are_usable() -> bool {
        !matches!(ephemeral_ports(), Some(r) if r.contains(&PORTS.start))
    }

    #[test]
    fn ports_are_distinct_and_avoid_the_ephemeral_range() {
        let ports = free_ports(5).unwrap();
        assert_eq!(ports.len(), 5);
        assert_eq!(ports.iter().collect::<HashSet<_>>().len(), 5, "{ports:?}");
        if ours_are_usable() {
            for p in &ports {
                assert!(PORTS.contains(p), "{p}");
            }
        }
        let port = free_port().unwrap();
        assert!(port > 0);
        if ours_are_usable() {
            assert!(PORTS.contains(&port), "{port}");
        }
        assert!(free_ports(0).unwrap().is_empty());
    }

    #[test]
    fn ports_in_use_are_not_free() {
        let server = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = server.local_addr().unwrap().port();
        assert!(probe(port).is_err());
        // A server on every address uses the port on 127.0.0.1 too.
        let server = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let port = server.local_addr().unwrap().port();
        assert!(probe(port).is_err());
    }

    /// A process spawned while Forge probes a port keeps a copy of the probe socket until
    /// it executes its program: the copy must neither accept connections nor keep the
    /// server from listening.
    #[cfg(unix)]
    #[test]
    fn a_copy_of_the_probe_neither_accepts_nor_blocks_the_server() {
        let port = free_port().unwrap();
        let copy = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
        copy.set_reuse_address(true).unwrap();
        copy.bind(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)).into())
            .unwrap();
        assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err());
        let server = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).unwrap();
        let client = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        let (accepted, peer) = server.accept().unwrap();
        assert_eq!(peer, client.local_addr().unwrap());
        drop((accepted, copy));
    }
}
