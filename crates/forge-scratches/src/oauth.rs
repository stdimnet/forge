//! OAuth 2.0 for the HTTP client, configured like PHPStorm's in `http-client.env.json`:
//!
//! ```json
//! { "dev": { "Security": { "Auth": { "shop": {
//!     "Type": "OAuth2",
//!     "Grant Type": "Authorization Code",
//!     "Auth URL": "https://id.example.com/authorize",
//!     "Token URL": "https://id.example.com/token",
//!     "Redirect URL": "http://localhost:51234/callback",
//!     "Client ID": "forge", "Client Secret": "{{secret}}",
//!     "Scope": "openid profile", "PKCE": true
//! } } } } }
//! ```
//!
//! Requests use the token with `{{$auth.token("shop")}}` (or `$auth.idToken`). Grants:
//! Client Credentials, Password, Authorization Code (with PKCE, in the browser) and
//! refresh tokens. This module is pure (configuration, URLs, form bodies, PKCE, token
//! responses); [`crate::oauth_client`] runs the flows.

use std::collections::BTreeMap;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Grant {
    ClientCredentials,
    Password,
    AuthorizationCode,
}

/// How the client authenticates at the token endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientAuth {
    /// `Authorization: Basic` (the default).
    Basic,
    /// `client_id` / `client_secret` in the form body.
    InBody,
}

/// One `Security.Auth` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthConfig {
    pub id: String,
    pub grant: Grant,
    pub token_url: String,
    #[serde(default)]
    pub auth_url: Option<String>,
    #[serde(default)]
    pub redirect_url: Option<String>,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub audience: Option<String>,
    pub pkce: bool,
    pub client_auth: ClientAuth,
}

fn text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Read the `Security.Auth` entries of an environment (public and private files merged,
/// private values win). `subst` replaces `{{variables}}` in values.
pub fn configs(
    public: Option<&str>,
    private: Option<&str>,
    env: &str,
    subst: &dyn Fn(&str) -> String,
) -> Result<BTreeMap<String, OAuthConfig>, String> {
    let mut merged: BTreeMap<String, serde_json::Map<String, Value>> = BTreeMap::new();
    for file in [public, private].into_iter().flatten() {
        let Ok(Value::Object(root)) = serde_json::from_str::<Value>(file) else {
            continue;
        };
        let Some(auth) = root
            .get(env)
            .and_then(|e| e.get("Security"))
            .and_then(|s| s.get("Auth"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        for (id, cfg) in auth {
            if let Value::Object(cfg) = cfg {
                merged.entry(id.clone()).or_default().extend(cfg.clone());
            }
        }
    }
    let mut out = BTreeMap::new();
    for (id, m) in merged {
        let get = |k: &str| m.get(k).and_then(text).map(|v| subst(&v)).filter(|v| !v.is_empty());
        if let Some(t) = get("Type") {
            if !t.eq_ignore_ascii_case("OAuth2") {
                continue;
            }
        }
        let grant = match get("Grant Type").unwrap_or_default().to_ascii_lowercase().as_str() {
            "client credentials" | "client_credentials" => Grant::ClientCredentials,
            "password" => Grant::Password,
            "authorization code" | "authorization_code" | "" => Grant::AuthorizationCode,
            other => return Err(format!("Auth '{id}': grant type '{other}' is not supported")),
        };
        let token_url = get("Token URL").ok_or_else(|| format!("Auth '{id}': 'Token URL' is missing"))?;
        let client_id = get("Client ID").ok_or_else(|| format!("Auth '{id}': 'Client ID' is missing"))?;
        let pkce = match m.get("PKCE") {
            Some(Value::Bool(b)) => *b,
            Some(Value::Object(_)) => true,
            _ => false,
        };
        let client_auth = match get("Client Credentials").map(|v| v.to_ascii_lowercase()) {
            Some(v) if v.contains("body") => ClientAuth::InBody,
            _ => ClientAuth::Basic,
        };
        if grant == Grant::AuthorizationCode && get("Auth URL").is_none() {
            return Err(format!("Auth '{id}': 'Auth URL' is missing"));
        }
        out.insert(
            id.clone(),
            OAuthConfig {
                id: id.clone(),
                grant,
                token_url,
                auth_url: get("Auth URL"),
                redirect_url: get("Redirect URL"),
                client_id,
                client_secret: get("Client Secret"),
                scope: get("Scope"),
                username: get("Username"),
                password: get("Password"),
                audience: get("Audience"),
                pkce,
                client_auth,
            },
        );
    }
    Ok(out)
}

/// A token reference in a request: `$auth.token("shop")` / `$auth.idToken("shop")`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenRef {
    /// The text inside `{{ }}`.
    pub key: String,
    pub id: String,
    pub id_token: bool,
}

/// The token references in a text (URL, headers or body of a request).
pub fn token_refs(text: &str) -> Vec<TokenRef> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("{{") {
        let after = &rest[i + 2..];
        let Some(end) = after.find("}}") else { break };
        let key = after[..end].trim();
        for (prefix, id_token) in [("$auth.token(", false), ("$auth.idToken(", true)] {
            if let Some(arg) = key.strip_prefix(prefix).and_then(|a| a.strip_suffix(')')) {
                let id = arg.trim().trim_matches(['"', '\'']).to_string();
                let r = TokenRef {
                    key: key.to_string(),
                    id,
                    id_token,
                };
                if !out.contains(&r) {
                    out.push(r);
                }
            }
        }
        rest = &after[end + 2..];
    }
    out
}

/// A PKCE verifier (43–128 characters) from random bytes, and its S256 challenge.
pub fn pkce(random: &[u8; 32]) -> (String, String) {
    let verifier = URL_SAFE_NO_PAD.encode(random);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `application/x-www-form-urlencoded` text of fields.
pub fn form(fields: &[(String, String)]) -> String {
    fields
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Decode `a=1&b=x%20y` (query strings of the redirect).
pub fn parse_query(q: &str) -> BTreeMap<String, String> {
    let decode = |s: &str| {
        let s = s.replace('+', " ");
        let bytes = s.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(b);
                    i += 3;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    };
    q.trim_start_matches('?')
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (decode(k), decode(v))
        })
        .collect()
}

/// The browser URL of the Authorization Code grant.
pub fn authorization_url(cfg: &OAuthConfig, redirect: &str, state: &str, challenge: Option<&str>) -> String {
    let base = cfg.auth_url.clone().unwrap_or_default();
    let mut fields = vec![
        ("response_type".to_string(), "code".to_string()),
        ("client_id".to_string(), cfg.client_id.clone()),
        ("redirect_uri".to_string(), redirect.to_string()),
        ("state".to_string(), state.to_string()),
    ];
    if let Some(s) = &cfg.scope {
        fields.push(("scope".into(), s.clone()));
    }
    if let Some(a) = &cfg.audience {
        fields.push(("audience".into(), a.clone()));
    }
    if let Some(c) = challenge {
        fields.push(("code_challenge".into(), c.to_string()));
        fields.push(("code_challenge_method".into(), "S256".into()));
    }
    let sep = if base.contains('?') { '&' } else { '?' };
    format!("{base}{sep}{}", form(&fields))
}

/// A request to the token endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenRequest {
    pub url: String,
    pub fields: Vec<(String, String)>,
    /// `Authorization: Basic` credentials, when the client authenticates that way.
    pub basic: Option<(String, String)>,
}

/// What to exchange at the token endpoint.
pub enum Exchange<'a> {
    /// Client Credentials or Password grant.
    Direct,
    /// Authorization Code: the code, the redirect URI and the PKCE verifier.
    Code {
        code: &'a str,
        redirect: &'a str,
        verifier: Option<&'a str>,
    },
    Refresh(&'a str),
}

pub fn token_request(cfg: &OAuthConfig, exchange: Exchange) -> TokenRequest {
    let mut fields: Vec<(String, String)> = Vec::new();
    let mut push = |k: &str, v: &str| fields.push((k.to_string(), v.to_string()));
    match exchange {
        Exchange::Direct => match cfg.grant {
            Grant::Password => {
                push("grant_type", "password");
                push("username", cfg.username.as_deref().unwrap_or(""));
                push("password", cfg.password.as_deref().unwrap_or(""));
            }
            _ => push("grant_type", "client_credentials"),
        },
        Exchange::Code {
            code,
            redirect,
            verifier,
        } => {
            push("grant_type", "authorization_code");
            push("code", code);
            push("redirect_uri", redirect);
            if let Some(v) = verifier {
                push("code_verifier", v);
            }
        }
        Exchange::Refresh(token) => {
            push("grant_type", "refresh_token");
            push("refresh_token", token);
        }
    }
    let direct_or_refresh = !fields.iter().any(|(k, v)| k == "grant_type" && v == "authorization_code");
    if direct_or_refresh {
        if let Some(s) = &cfg.scope {
            fields.push(("scope".into(), s.clone()));
        }
        if let Some(a) = &cfg.audience {
            fields.push(("audience".into(), a.clone()));
        }
    }
    let secret = cfg.client_secret.clone();
    let basic = match (cfg.client_auth, &secret) {
        (ClientAuth::Basic, Some(s)) => Some((cfg.client_id.clone(), s.clone())),
        _ => {
            fields.push(("client_id".into(), cfg.client_id.clone()));
            if let Some(s) = secret {
                fields.push(("client_secret".into(), s));
            }
            None
        }
    };
    TokenRequest {
        url: cfg.token_url.clone(),
        fields,
        basic,
    }
}

/// Tokens returned by the token endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    pub access_token: String,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Unix seconds when it expires, if the server said.
    #[serde(default)]
    pub expires_at: Option<u64>,
}

impl Token {
    /// Expired (or about to, within 30 s) at Unix time `now`.
    pub fn expired(&self, now: u64) -> bool {
        self.expires_at.is_some_and(|t| now + 30 >= t)
    }
}

/// Read a token endpoint response (`now` = Unix seconds, for `expires_in`).
pub fn parse_token_response(status: u16, body: &str, now: u64) -> Result<Token, String> {
    let v: Value = serde_json::from_str(body).map_err(|_| {
        format!("The token endpoint answered {status} without JSON: {}", body.chars().take(200).collect::<String>())
    })?;
    if let Some(err) = v.get("error").and_then(Value::as_str) {
        let desc = v.get("error_description").and_then(Value::as_str).unwrap_or("");
        return Err(format!("OAuth error: {err} {desc}").trim().to_string());
    }
    if !(200..300).contains(&status) {
        return Err(format!("The token endpoint answered {status}"));
    }
    let access_token = v
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or("The token response has no access_token")?
        .to_string();
    let expires_at = v
        .get("expires_in")
        .and_then(|e| e.as_u64().or_else(|| e.as_str()?.parse().ok()))
        .map(|s| now + s);
    Ok(Token {
        access_token,
        id_token: v.get("id_token").and_then(Value::as_str).map(str::to_string),
        refresh_token: v.get("refresh_token").and_then(Value::as_str).map(str::to_string),
        expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENV: &str = r#"{
  "dev": {
    "host": "localhost",
    "Security": { "Auth": {
      "shop": {
        "Type": "OAuth2", "Grant Type": "Authorization Code",
        "Auth URL": "https://{{host}}/authorize", "Token URL": "https://{{host}}/token",
        "Client ID": "forge", "Scope": "openid profile", "PKCE": true
      },
      "machine": { "Type": "OAuth2", "Grant Type": "Client Credentials", "Token URL": "https://id/token", "Client ID": "svc", "Client Credentials": "in body" }
    } }
  }
}"#;
    const PRIVATE: &str = r#"{ "dev": { "Security": { "Auth": { "machine": { "Client Secret": "s3cret" } } } } }"#;

    fn subst(s: &str) -> String {
        s.replace("{{host}}", "id.example.com")
    }

    #[test]
    fn reads_configs_from_environment_files() {
        let c = configs(Some(ENV), Some(PRIVATE), "dev", &subst).unwrap();
        let shop = &c["shop"];
        assert_eq!(shop.grant, Grant::AuthorizationCode);
        assert_eq!(shop.auth_url.as_deref(), Some("https://id.example.com/authorize"));
        assert!(shop.pkce);
        let m = &c["machine"];
        assert_eq!((m.grant, m.client_auth), (Grant::ClientCredentials, ClientAuth::InBody));
        assert_eq!(m.client_secret.as_deref(), Some("s3cret"));
        assert!(configs(Some(ENV), None, "prod", &subst).unwrap().is_empty());
        let bad = r#"{"dev": {"Security": {"Auth": {"x": {"Type": "OAuth2", "Grant Type": "Implicit", "Token URL": "t", "Client ID": "c"}}}}}"#;
        assert!(configs(Some(bad), None, "dev", &subst).is_err());
    }

    #[test]
    fn finds_token_references() {
        let r = token_refs("Bearer {{$auth.token(\"shop\")}} {{ $auth.idToken('shop') }} {{host}}");
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].id.as_str(), r[0].id_token), ("shop", false));
        assert_eq!(r[1].key, "$auth.idToken('shop')");
    }

    #[test]
    fn pkce_challenge_matches_rfc_7636() {
        // RFC 7636 appendix B.
        let bytes: [u8; 32] = [
            116, 24, 223, 180, 151, 153, 224, 37, 79, 250, 96, 125, 216, 173, 187, 186, 22, 212, 37, 77,
            105, 214, 191, 240, 91, 88, 5, 88, 83, 132, 141, 121,
        ];
        let (verifier, challenge) = pkce(&bytes);
        assert_eq!(verifier, "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn builds_urls_and_token_requests() {
        let c = configs(Some(ENV), Some(PRIVATE), "dev", &subst).unwrap();
        let url = authorization_url(&c["shop"], "http://127.0.0.1:5000/callback", "st", Some("ch"));
        assert_eq!(
            url,
            "https://id.example.com/authorize?response_type=code&client_id=forge&redirect_uri=http%3A%2F%2F127.0.0.1%3A5000%2Fcallback&state=st&scope=openid%20profile&code_challenge=ch&code_challenge_method=S256"
        );
        let q = parse_query("?code=abc%2F1&state=st&x=a+b");
        assert_eq!((q["code"].as_str(), q["x"].as_str()), ("abc/1", "a b"));
        let req = token_request(&c["machine"], Exchange::Direct);
        assert_eq!(form(&req.fields), "grant_type=client_credentials&client_id=svc&client_secret=s3cret");
        assert_eq!(req.basic, None);
        let code = token_request(
            &c["shop"],
            Exchange::Code { code: "abc", redirect: "http://x/cb", verifier: Some("v") },
        );
        assert_eq!(form(&code.fields), "grant_type=authorization_code&code=abc&redirect_uri=http%3A%2F%2Fx%2Fcb&code_verifier=v&client_id=forge");
        let refresh = token_request(&c["shop"], Exchange::Refresh("r1"));
        assert!(form(&refresh.fields).starts_with("grant_type=refresh_token&refresh_token=r1&scope=openid%20profile"));
    }

    #[test]
    fn reads_token_responses() {
        let t = parse_token_response(200, r#"{"access_token":"a","expires_in":3600,"refresh_token":"r","id_token":"i"}"#, 1000).unwrap();
        assert_eq!(t.expires_at, Some(4600));
        assert!(!t.expired(1000));
        assert!(t.expired(4580));
        assert_eq!(
            parse_token_response(400, r#"{"error":"invalid_client","error_description":"bad secret"}"#, 0).unwrap_err(),
            "OAuth error: invalid_client bad secret"
        );
        assert!(parse_token_response(500, "oops", 0).is_err());
    }
}
