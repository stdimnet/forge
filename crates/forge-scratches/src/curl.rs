//! cURL commands and `.http` requests: "Import from cURL" (a pasted `curl …` command
//! becomes a request) and "Copy as cURL" (a resolved request becomes a command line).

use crate::http::{Header, HttpRequest};

/// Split a shell command line: single and double quotes, backslash escapes, `$'…'`
/// strings and line continuations (`\` at the end of a line, `^` in cmd.exe).
fn split(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if matches!(chars.peek(), Some('\n') | Some('\r')) => {
                while matches!(chars.peek(), Some('\n') | Some('\r')) {
                    chars.next();
                }
            }
            '^' if matches!(chars.peek(), Some('\n') | Some('\r')) => {
                while matches!(chars.peek(), Some('\n') | Some('\r')) {
                    chars.next();
                }
            }
            '$' if chars.peek() == Some(&'\'') => {
                chars.next();
                in_word = true;
                while let Some(c) = chars.next() {
                    match c {
                        '\'' => break,
                        '\\' => match chars.next() {
                            Some('n') => cur.push('\n'),
                            Some('t') => cur.push('\t'),
                            Some('r') => cur.push('\r'),
                            Some(o) => cur.push(o),
                            None => {}
                        },
                        c => cur.push(c),
                    }
                }
            }
            '\'' => {
                in_word = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    cur.push(c);
                }
            }
            '"' => {
                in_word = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' if matches!(chars.peek(), Some('"' | '\\' | '$' | '`')) => {
                            cur.push(chars.next().expect("peeked"));
                        }
                        c => cur.push(c),
                    }
                }
            }
            '\\' => {
                if let Some(n) = chars.next() {
                    in_word = true;
                    cur.push(n);
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    out.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        out.push(cur);
    }
    out
}

fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Turn a `curl` command (as copied from browser dev tools or API docs) into a request.
/// Understood: `-X`, `-H`, `-d`/`--data*`, `--json`, `-F` (multipart is described as
/// form fields), `-u`, `-b`, `-A`, `-e`, `-G`, `-I`, `--url`; options that only change
/// how curl runs (`-s`, `-L`, `-k`, `--compressed`, …) are skipped.
pub fn parse_curl(command: &str) -> Result<HttpRequest, String> {
    let args = split(command.trim());
    let mut it = args.into_iter().peekable();
    match it.next() {
        Some(c) if c == "curl" || c.ends_with("/curl") || c.eq_ignore_ascii_case("curl.exe") => {}
        _ => return Err("Not a cURL command: it must start with curl".into()),
    }
    let mut method: Option<String> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<Header> = Vec::new();
    let mut data: Vec<String> = Vec::new();
    let mut form: Vec<String> = Vec::new();
    let mut get = false;
    let mut head = false;
    let mut json = false;
    let header = |headers: &mut Vec<Header>, name: &str, value: String| {
        headers.retain(|h: &Header| !h.name.eq_ignore_ascii_case(name));
        headers.push(Header {
            name: name.into(),
            value,
        });
    };
    // Options taking a value that do not matter for the request.
    const SKIP_VALUE: &[&str] = &[
        "-o",
        "--output",
        "-m",
        "--max-time",
        "--connect-timeout",
        "-w",
        "--write-out",
        "--retry",
        "-c",
        "--cookie-jar",
        "--cacert",
        "--cert",
        "--key",
        "-E",
        "-x",
        "--proxy",
        "--resolve",
        "--limit-rate",
        "-r",
        "--range",
    ];
    while let Some(arg) = it.next() {
        // `--data=x` / `-XPOST` forms.
        let (name, inline) = if let Some(rest) = arg.strip_prefix("--") {
            match rest.split_once('=') {
                Some((n, v)) => (format!("--{n}"), Some(v.to_string())),
                None => (arg.clone(), None),
            }
        } else if arg.starts_with('-') && arg.len() > 2 && "XHdubAeF".contains(&arg[1..2]) {
            (arg[..2].to_string(), Some(arg[2..].to_string()))
        } else {
            (arg.clone(), None)
        };
        let value = |it: &mut std::iter::Peekable<std::vec::IntoIter<String>>| {
            inline.clone().or_else(|| it.next()).unwrap_or_default()
        };
        match name.as_str() {
            "-X" | "--request" => method = Some(value(&mut it).to_ascii_uppercase()),
            "-H" | "--header" => {
                let v = value(&mut it);
                if let Some((n, val)) = v.split_once(':') {
                    headers.push(Header {
                        name: n.trim().to_string(),
                        value: val.trim().to_string(),
                    });
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii" => {
                data.push(value(&mut it))
            }
            "--data-urlencode" => data.push(value(&mut it)),
            "--json" => {
                json = true;
                data.push(value(&mut it));
            }
            "-F" | "--form" | "--form-string" => form.push(value(&mut it)),
            "-u" | "--user" => {
                let v = value(&mut it);
                header(
                    &mut headers,
                    "Authorization",
                    format!("Basic {}", base64(v.as_bytes())),
                );
            }
            "-b" | "--cookie" => header(&mut headers, "Cookie", value(&mut it)),
            "-A" | "--user-agent" => header(&mut headers, "User-Agent", value(&mut it)),
            "-e" | "--referer" => header(&mut headers, "Referer", value(&mut it)),
            "--url" => url = Some(value(&mut it)),
            "-G" | "--get" => get = true,
            "-I" | "--head" => head = true,
            n if SKIP_VALUE.contains(&n) => {
                value(&mut it);
            }
            n if n.starts_with('-') => {}
            _ => {
                if url.is_none() {
                    url = Some(arg);
                }
            }
        }
    }
    let mut url = url.ok_or("The cURL command has no URL")?;
    if !url.contains("://") {
        url = format!("http://{url}");
    }
    let mut body = None;
    if get && !data.is_empty() {
        let sep = if url.contains('?') { '&' } else { '?' };
        url = format!("{url}{sep}{}", data.join("&"));
    } else if !data.is_empty() {
        body = Some(data.join("&"));
        let has_type = headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case("content-type"));
        if json {
            if !has_type {
                header(&mut headers, "Content-Type", "application/json".into());
            }
            if !headers
                .iter()
                .any(|h| h.name.eq_ignore_ascii_case("accept"))
            {
                header(&mut headers, "Accept", "application/json".into());
            }
        } else if !has_type {
            header(
                &mut headers,
                "Content-Type",
                "application/x-www-form-urlencoded".into(),
            );
        }
    } else if !form.is_empty() {
        // multipart/form-data: one part per field, files as `< path`.
        let boundary = "WebAppBoundary";
        header(
            &mut headers,
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        );
        let mut parts = String::new();
        for f in &form {
            let (name, value) = f.split_once('=').unwrap_or((f, ""));
            parts.push_str(&format!("--{boundary}\n"));
            match value.strip_prefix('@') {
                Some(path) => {
                    let file = path.split(';').next().unwrap_or(path);
                    let file_name = file.rsplit(['/', '\\']).next().unwrap_or(file);
                    parts.push_str(&format!(
                        "Content-Disposition: form-data; name=\"{name}\"; filename=\"{file_name}\"\n\n< {file}\n"
                    ));
                }
                None => parts.push_str(&format!(
                    "Content-Disposition: form-data; name=\"{name}\"\n\n{value}\n"
                )),
            }
        }
        parts.push_str(&format!("--{boundary}--"));
        body = Some(parts);
    }
    let method = method.unwrap_or_else(|| {
        if head {
            "HEAD".into()
        } else if body.is_some() {
            "POST".into()
        } else {
            "GET".into()
        }
    });
    Ok(HttpRequest {
        index: 0,
        line: 0,
        end_line: 0,
        name: None,
        method,
        url,
        version: None,
        headers,
        body,
        handler: None,
        handler_file: None,
    })
}

/// The `.http` text of a request (as "Import from cURL" inserts it).
pub fn to_http(req: &HttpRequest) -> String {
    let mut out = format!("{} {}\n", req.method, req.url);
    for h in &req.headers {
        out.push_str(&format!("{}: {}\n", h.name, h.value));
    }
    if let Some(body) = &req.body {
        out.push('\n');
        let pretty = req
            .header("Content-Type")
            .is_some_and(|t| t.contains("json"))
            .then(|| crate::http::pretty_json(body))
            .flatten();
        out.push_str(pretty.as_deref().unwrap_or(body));
        out.push('\n');
    }
    out
}

fn quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,?&".contains(c))
    {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

/// "Copy as cURL": a POSIX shell command line for a resolved request, one option per
/// line like PHPStorm's.
pub fn to_curl(req: &HttpRequest) -> String {
    let method = match req.method.as_str() {
        "GRAPHQL" => "POST",
        m => m,
    };
    let mut parts = vec![format!("curl -X {} {}", method, quote(&req.url))];
    for h in &req.headers {
        parts.push(format!("-H {}", quote(&format!("{}: {}", h.name, h.value))));
    }
    if let Some(body) = &req.body {
        parts.push(format!("--data-raw {}", quote(body)));
    }
    parts.join(" \\\n    ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_commands_from_dev_tools() {
        let cmd = r#"curl 'https://api.example.com/users?page=2' \
  -H 'accept: application/json' \
  -H 'authorization: Bearer abc' \
  --data-raw '{"name":"Ada"}' \
  --compressed"#;
        let r = parse_curl(cmd).unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "https://api.example.com/users?page=2");
        assert_eq!(r.header("authorization"), Some("Bearer abc"));
        assert_eq!(r.body.as_deref(), Some(r#"{"name":"Ada"}"#));
        assert_eq!(
            r.header("Content-Type"),
            Some("application/x-www-form-urlencoded")
        );
        let http = to_http(&r);
        assert!(
            http.starts_with(
                "POST https://api.example.com/users?page=2\naccept: application/json\n"
            ),
            "{http}"
        );

        let r = parse_curl("curl -XPUT localhost:8000/a -u ada:secret --json '{\"a\":1}'").unwrap();
        assert_eq!(
            (r.method.as_str(), r.url.as_str()),
            ("PUT", "http://localhost:8000/a")
        );
        assert_eq!(r.header("Authorization"), Some("Basic YWRhOnNlY3JldA=="));
        assert_eq!(r.header("Content-Type"), Some("application/json"));
        assert!(to_http(&r).contains("{\n  \"a\": 1\n}"));

        let r = parse_curl("curl -G https://x.dev/search -d q=forge -d n=5").unwrap();
        assert_eq!(
            (r.method.as_str(), r.url.as_str()),
            ("GET", "https://x.dev/search?q=forge&n=5")
        );
        assert_eq!(r.body, None);

        let r = parse_curl("curl -F name=Ada -F avatar=@./me.png https://x.dev/upload").unwrap();
        assert!(r
            .header("Content-Type")
            .unwrap()
            .starts_with("multipart/form-data"));
        assert!(r
            .body
            .as_deref()
            .unwrap()
            .contains("filename=\"me.png\"\n\n< ./me.png"));

        assert!(parse_curl("wget https://x.dev").is_err());
        assert!(parse_curl("curl -s").is_err());
        let r = parse_curl("curl -I --url https://x.dev -o out.txt").unwrap();
        assert_eq!(
            (r.method.as_str(), r.url.as_str()),
            ("HEAD", "https://x.dev")
        );
    }

    #[test]
    fn copies_requests_as_curl_and_back() {
        let req = parse_curl(
            "curl -X POST https://api.example.com/users -H 'Content-Type: application/json' --data-raw '{\"name\":\"O'\\''Brien\"}'",
        )
        .unwrap();
        assert_eq!(req.body.as_deref(), Some("{\"name\":\"O'Brien\"}"));
        let cmd = to_curl(&req);
        assert_eq!(
            cmd,
            "curl -X POST https://api.example.com/users \\\n    -H 'Content-Type: application/json' \\\n    --data-raw '{\"name\":\"O'\\''Brien\"}'"
        );
        let back = parse_curl(&cmd).unwrap();
        assert_eq!(back.body, req.body);
        assert_eq!(back.headers, req.headers);
    }
}
