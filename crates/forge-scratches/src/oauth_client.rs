//! Runs the OAuth 2.0 flows of [`crate::oauth`] (feature `client`): token requests with
//! reqwest, the Authorization Code grant through the system browser with a one-shot
//! loopback listener for the redirect, and refresh tokens. Tokens are cached in memory
//! for the IDE session ([`TokenCache`]); nothing is written to disk.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::oauth::{self, Exchange, Grant, OAuthConfig, Token};

/// How long the browser login may take.
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// Tokens by `(environment, auth id)`, for the session.
#[derive(Clone, Default)]
pub struct TokenCache {
    tokens: Arc<Mutex<BTreeMap<(String, String), Token>>>,
}

impl TokenCache {
    pub fn get(&self, env: &str, id: &str) -> Option<Token> {
        self.tokens
            .lock()
            .expect("token cache")
            .get(&(env.to_string(), id.to_string()))
            .cloned()
    }

    pub fn put(&self, env: &str, id: &str, token: Token) {
        self.tokens
            .lock()
            .expect("token cache")
            .insert((env.to_string(), id.to_string()), token);
    }

    pub fn clear(&self) {
        self.tokens.lock().expect("token cache").clear();
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn random_bytes() -> [u8; 32] {
    use std::hash::{BuildHasher, Hasher};
    let mut out = [0u8; 32];
    for chunk in out.chunks_mut(8) {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        );
        chunk.copy_from_slice(&h.finish().to_le_bytes());
    }
    out
}

/// Send a token request and read the token.
pub async fn request_token(req: &oauth::TokenRequest) -> Result<Token, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let mut b = client
        .post(&req.url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(oauth::form(&req.fields));
    if let Some((id, secret)) = &req.basic {
        b = b.basic_auth(id, Some(secret));
    }
    let resp = b
        .send()
        .await
        .map_err(|e| format!("Token request to {} failed: {e}", req.url))?;
    let status = resp.status().as_u16();
    let body = resp.text().await.map_err(|e| e.to_string())?;
    oauth::parse_token_response(status, &body, now())
}

/// The Authorization Code grant: listen on the redirect URL's loopback port (a free port
/// when the config has none), open the authorization page with `open_browser` and
/// exchange the code that comes back.
pub async fn authorization_code(
    cfg: &OAuthConfig,
    open_browser: &(dyn Fn(&str) + Send + Sync),
) -> Result<Token, String> {
    let configured = cfg.redirect_url.clone();
    let (host, port, path) = match &configured {
        Some(u) => {
            let url =
                reqwest::Url::parse(u).map_err(|e| format!("Invalid Redirect URL '{u}': {e}"))?;
            let host = url.host_str().unwrap_or("127.0.0.1").to_string();
            if !matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]") {
                return Err(format!(
                    "Redirect URL '{u}' must point at this machine (localhost) so Forge can receive the code"
                ));
            }
            (
                host,
                url.port_or_known_default().unwrap_or(80),
                url.path().to_string(),
            )
        }
        None => ("127.0.0.1".to_string(), 0, "/callback".to_string()),
    };
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| format!("Cannot listen for the OAuth redirect on port {port}: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = configured.unwrap_or_else(|| format!("http://{host}:{port}{path}"));
    let (verifier, challenge) = oauth::pkce(&random_bytes());
    let state = oauth::pkce(&random_bytes()).0[..16].to_string();
    let url = oauth::authorization_url(
        cfg,
        &redirect,
        &state,
        cfg.pkce.then_some(challenge.as_str()),
    );
    open_browser(&url);
    let code = tokio::time::timeout(LOGIN_TIMEOUT, receive_code(&listener, &path, &state))
        .await
        .map_err(|_| "No answer from the browser login within 5 minutes".to_string())??;
    let req = oauth::token_request(
        cfg,
        Exchange::Code {
            code: &code,
            redirect: &redirect,
            verifier: cfg.pkce.then_some(verifier.as_str()),
        },
    );
    request_token(&req).await
}

/// Wait for the browser's redirect to `path` and answer it with a short page.
async fn receive_code(listener: &TcpListener, path: &str, state: &str) -> Result<String, String> {
    loop {
        let (mut sock, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 8192];
        let n = sock.read(&mut buf).await.map_err(|e| e.to_string())?;
        let head = String::from_utf8_lossy(&buf[..n]).to_string();
        let target = head.split_whitespace().nth(1).unwrap_or("").to_string();
        let (p, query) = target.split_once('?').unwrap_or((&target, ""));
        if p != path {
            let _ = sock
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
            continue;
        }
        let q = oauth::parse_query(query);
        let (ok, result) = match (q.get("code"), q.get("error")) {
            (_, Some(err)) => (
                false,
                Err(format!(
                    "OAuth error: {err} {}",
                    q.get("error_description").map(String::as_str).unwrap_or("")
                )
                .trim()
                .to_string()),
            ),
            (Some(code), None) if q.get("state").map(String::as_str) == Some(state) => {
                (true, Ok(code.clone()))
            }
            (Some(_), None) => (
                false,
                Err(
                    "The OAuth state does not match; the login was not started by Forge"
                        .to_string(),
                ),
            ),
            _ => (
                false,
                Err("The redirect has no authorization code".to_string()),
            ),
        };
        let page = if ok {
            "<html><body style=\"font-family:sans-serif\"><h3>Forge received the authorization.</h3><p>You can close this window.</p></body></html>"
        } else {
            "<html><body style=\"font-family:sans-serif\"><h3>Authorization failed.</h3><p>See the Run console in Forge.</p></body></html>"
        };
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
            page.len()
        );
        let _ = sock.write_all(resp.as_bytes()).await;
        let _ = sock.shutdown().await;
        return result;
    }
}

/// A valid token for `cfg`: the cached one, a refreshed one, or a new one from its grant.
/// Returns the token and a line for the console saying where it came from.
pub async fn token(
    cfg: &OAuthConfig,
    env: &str,
    cache: &TokenCache,
    open_browser: &(dyn Fn(&str) + Send + Sync),
) -> Result<(Token, String), String> {
    if let Some(t) = cache.get(env, &cfg.id) {
        if !t.expired(now()) {
            return Ok((t, format!("Using the cached OAuth token '{}'", cfg.id)));
        }
        if let Some(refresh) = &t.refresh_token {
            let req = oauth::token_request(cfg, Exchange::Refresh(refresh));
            if let Ok(mut new) = request_token(&req).await {
                if new.refresh_token.is_none() {
                    new.refresh_token = Some(refresh.clone());
                }
                cache.put(env, &cfg.id, new.clone());
                return Ok((new, format!("Refreshed the OAuth token '{}'", cfg.id)));
            }
        }
    }
    let (t, how) = match cfg.grant {
        Grant::ClientCredentials | Grant::Password => (
            request_token(&oauth::token_request(cfg, Exchange::Direct)).await?,
            "Acquired",
        ),
        Grant::AuthorizationCode => (
            authorization_code(cfg, open_browser).await?,
            "Logged in for",
        ),
    };
    cache.put(env, &cfg.id, t.clone());
    Ok((t, format!("{how} the OAuth token '{}'", cfg.id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::{configs, pkce};
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use sha2::{Digest, Sha256};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A local authorization server: `/token` checks grants, client auth and PKCE;
    /// `/authorize` redirects to the redirect URI with a code.
    async fn auth_server() -> (String, Arc<AtomicUsize>, Arc<Mutex<Option<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let calls = Arc::new(AtomicUsize::new(0));
        let challenge: Arc<Mutex<Option<String>>> = Arc::default();
        let (c2, ch2) = (calls.clone(), challenge.clone());
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let (calls, challenge) = (c2.clone(), ch2.clone());
                tokio::spawn(async move {
                    let mut got = Vec::new();
                    let mut buf = vec![0u8; 4096];
                    loop {
                        let n = sock.read(&mut buf).await.unwrap_or(0);
                        got.extend_from_slice(&buf[..n]);
                        let text = String::from_utf8_lossy(&got).to_string();
                        if let Some(h) = text.find("\r\n\r\n") {
                            let len = text
                                .lines()
                                .find_map(|l| {
                                    l.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .map(|v| v.trim().parse::<usize>().unwrap())
                                })
                                .unwrap_or(0);
                            if got.len() >= h + 4 + len {
                                break;
                            }
                        }
                        if n == 0 {
                            break;
                        }
                    }
                    let text = String::from_utf8_lossy(&got).to_string();
                    let target = text.split_whitespace().nth(1).unwrap_or("").to_string();
                    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
                    let resp = if target.starts_with("/authorize") {
                        let q = oauth::parse_query(target.split_once('?').unwrap().1);
                        *challenge.lock().unwrap() = q.get("code_challenge").cloned();
                        let loc = format!("{}?code=c0de&state={}", q["redirect_uri"], q["state"]);
                        format!("HTTP/1.1 302 Found\r\nLocation: {loc}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    } else {
                        calls.fetch_add(1, Ordering::SeqCst);
                        let f = oauth::parse_query(&body);
                        let basic_ok = text.to_ascii_lowercase().contains(
                            &"authorization: basic c3ZjOnMzY3JldA==".to_ascii_lowercase(),
                        );
                        let json = match f.get("grant_type").map(String::as_str) {
                            Some("client_credentials") if basic_ok => {
                                r#"{"access_token":"cc-token","expires_in":1,"refresh_token":"r1"}"#
                                    .to_string()
                            }
                            Some("password")
                                if f.get("username").map(String::as_str) == Some("ada")
                                    && f.get("password").map(String::as_str) == Some("pw") =>
                            {
                                r#"{"access_token":"pw-token"}"#.to_string()
                            }
                            Some("refresh_token")
                                if f.get("refresh_token").map(String::as_str) == Some("r1") =>
                            {
                                r#"{"access_token":"refreshed","expires_in":3600}"#.to_string()
                            }
                            Some("authorization_code") => {
                                let verifier = f.get("code_verifier").cloned().unwrap_or_default();
                                let expected =
                                    challenge.lock().unwrap().clone().unwrap_or_default();
                                let actual =
                                    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
                                if f.get("code").map(String::as_str) == Some("c0de")
                                    && actual == expected
                                {
                                    r#"{"access_token":"code-token","id_token":"idt"}"#.to_string()
                                } else {
                                    r#"{"error":"invalid_grant"}"#.to_string()
                                }
                            }
                            _ => r#"{"error":"invalid_client"}"#.to_string(),
                        };
                        let status = if json.contains("error") {
                            "400 Bad Request"
                        } else {
                            "200 OK"
                        };
                        format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}", json.len())
                    };
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        (base, calls, challenge)
    }

    fn env(base: &str) -> String {
        format!(
            r#"{{"dev": {{"Security": {{"Auth": {{
  "machine": {{"Type": "OAuth2", "Grant Type": "Client Credentials", "Token URL": "{base}/token", "Client ID": "svc", "Client Secret": "s3cret"}},
  "user": {{"Type": "OAuth2", "Grant Type": "Password", "Token URL": "{base}/token", "Client ID": "app", "Username": "ada", "Password": "pw", "Client Credentials": "in body"}},
  "browser": {{"Type": "OAuth2", "Grant Type": "Authorization Code", "Auth URL": "{base}/authorize", "Token URL": "{base}/token", "Client ID": "web", "PKCE": true}}
}}}}}}}}"#
        )
    }

    fn no_browser(_: &str) {
        panic!("no browser expected");
    }

    #[tokio::test]
    async fn client_credentials_password_and_refresh() {
        let (base, calls, _) = auth_server().await;
        let cfgs = configs(Some(&env(&base)), None, "dev", &|s| s.to_string()).unwrap();
        let cache = TokenCache::default();
        let (t, how) = token(&cfgs["machine"], "dev", &cache, &no_browser)
            .await
            .unwrap();
        assert_eq!(
            (t.access_token.as_str(), how.as_str()),
            ("cc-token", "Acquired the OAuth token 'machine'")
        );
        // The token expires within a second (inside the 30 s margin): it is refreshed.
        let (t, how) = token(&cfgs["machine"], "dev", &cache, &no_browser)
            .await
            .unwrap();
        assert_eq!(
            (t.access_token.as_str(), how.as_str()),
            ("refreshed", "Refreshed the OAuth token 'machine'")
        );
        assert_eq!(t.refresh_token.as_deref(), Some("r1"));
        // Now cached.
        let (_, how) = token(&cfgs["machine"], "dev", &cache, &no_browser)
            .await
            .unwrap();
        assert!(how.starts_with("Using the cached"));
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        let (t, _) = token(&cfgs["user"], "dev", &cache, &no_browser)
            .await
            .unwrap();
        assert_eq!(t.access_token, "pw-token");

        let mut bad = cfgs["machine"].clone();
        bad.client_secret = Some("wrong".into());
        let err = token(&bad, "other", &cache, &no_browser).await.unwrap_err();
        assert_eq!(err, "OAuth error: invalid_client");
    }

    #[tokio::test]
    async fn authorization_code_with_pkce_through_the_browser() {
        let (base, _, challenge) = auth_server().await;
        let cfgs = configs(Some(&env(&base)), None, "dev", &|s| s.to_string()).unwrap();
        // The "browser" follows the authorization page's redirect to Forge's listener.
        let browser = |url: &str| {
            let url = url.to_string();
            tokio::spawn(async move {
                let client = reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::limited(3))
                    .build()
                    .unwrap();
                let page = client.get(&url).send().await.unwrap().text().await.unwrap();
                assert!(page.contains("Forge received the authorization"), "{page}");
            });
        };
        let cache = TokenCache::default();
        let (t, how) = token(&cfgs["browser"], "dev", &cache, &browser)
            .await
            .unwrap();
        assert_eq!(
            (t.access_token.as_str(), t.id_token.as_deref()),
            ("code-token", Some("idt"))
        );
        assert_eq!(how, "Logged in for the OAuth token 'browser'");
        assert!(challenge
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|c| c.len() == 43));
        let _ = pkce;
    }
}
