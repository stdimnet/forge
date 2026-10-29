//! URLs of the "PHP Web Page" run configuration and the Xdebug trigger that starts a debug
//! session for a browser request.

use crate::servers::ServerConfig;
use crate::IDE_KEY;

/// Query parameter that makes Xdebug start a session (and set the `XDEBUG_SESSION` cookie,
/// so the following requests of the page are debugged too).
pub const SESSION_START_PARAM: &str = "XDEBUG_SESSION_START";

/// The server's base URL: `http://shop.test`, `http://localhost:8080`, `https://…` for 443.
pub fn server_url(server: &ServerConfig) -> String {
    let host = match server.host.trim() {
        "" => "localhost",
        h => h,
    };
    if host.contains("://") {
        return host.trim_end_matches('/').to_string();
    }
    match server.port {
        80 | 0 => format!("http://{host}"),
        443 => format!("https://{host}"),
        p => format!("http://{host}:{p}"),
    }
}

/// The page to open: `start` (an absolute URL, or a path on `server`), with the Xdebug
/// trigger when debugging.
pub fn page_url(server: Option<&ServerConfig>, start: &str, debug: bool) -> String {
    let start = start.trim();
    let url = if start.contains("://") {
        start.to_string()
    } else {
        let base = server
            .map(server_url)
            .unwrap_or_else(|| "http://localhost".into());
        let path = start.trim_start_matches('/');
        format!("{base}/{path}")
    };
    if debug {
        with_session_start(&url)
    } else {
        url
    }
}

/// `url` with `XDEBUG_SESSION_START=FORGE` added to its query (before a `#fragment`).
pub fn with_session_start(url: &str) -> String {
    let (base, fragment) = match url.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (url, None),
    };
    if base.contains(&format!("{SESSION_START_PARAM}=")) {
        return url.to_string();
    }
    let sep = if !base.contains('?') {
        "?"
    } else if base.ends_with('?') || base.ends_with('&') {
        ""
    } else {
        "&"
    };
    let mut out = format!("{base}{sep}{SESSION_START_PARAM}={IDE_KEY}");
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(host: &str, port: u16) -> ServerConfig {
        ServerConfig {
            id: "s".into(),
            name: "s".into(),
            host: host.into(),
            port,
            debugger: "Xdebug".into(),
            use_path_mappings: false,
            mappings: vec![],
        }
    }

    #[test]
    fn builds_page_urls() {
        let shop = server("shop.test", 80);
        assert_eq!(
            page_url(Some(&shop), "/orders", false),
            "http://shop.test/orders"
        );
        assert_eq!(
            page_url(Some(&shop), "orders?page=2#top", true),
            "http://shop.test/orders?page=2&XDEBUG_SESSION_START=FORGE#top"
        );
        assert_eq!(
            page_url(Some(&server("localhost", 8080)), "", true),
            "http://localhost:8080/?XDEBUG_SESSION_START=FORGE"
        );
        assert_eq!(
            server_url(&server("secure.test", 443)),
            "https://secure.test"
        );
        assert_eq!(
            page_url(None, "https://x.test/a", false),
            "https://x.test/a"
        );
        assert_eq!(
            with_session_start("http://x/?XDEBUG_SESSION_START=FORGE"),
            "http://x/?XDEBUG_SESSION_START=FORGE"
        );
    }
}
