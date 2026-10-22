//! Just enough URI handling for `$id` and `$ref`: reference resolution (RFC 3986 section 5)
//! and fragments. Project files are `project:/<path>`, files on this machine `file:///…`.

/// The URI without its fragment, and the fragment (without `#`).
pub fn split_fragment(uri: &str) -> (&str, Option<&str>) {
    match uri.find('#') {
        Some(i) => (&uri[..i], Some(&uri[i + 1..])),
        None => (uri, None),
    }
}

/// The URI without fragment (an empty fragment `…/schema#` is the same document).
pub fn document(uri: &str) -> &str {
    split_fragment(uri).0
}

fn scheme_len(s: &str) -> Option<usize> {
    let i = s.find(':')?;
    let scheme = &s[..i];
    (!scheme.is_empty()
        && scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))
    .then_some(i)
}

pub fn is_absolute(s: &str) -> bool {
    // A Windows path (`C:\…`) is not a URI.
    scheme_len(s).is_some_and(|i| i > 1)
}

pub fn is_remote(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// `(scheme incl. ':', authority incl. '//' or "", path, query incl. '?')`.
fn parts(uri: &str) -> (&str, &str, &str, &str) {
    let uri = document(uri);
    let s = scheme_len(uri).map_or(0, |i| i + 1);
    let (scheme, rest) = uri.split_at(s);
    let (authority, rest) = if let Some(r) = rest.strip_prefix("//") {
        let end = r.find('/').map_or(r.len(), |i| i);
        rest.split_at(end + 2)
    } else {
        ("", rest)
    };
    let (path, query) = match rest.find('?') {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    (scheme, authority, path, query)
}

fn remove_dots(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    let segs: Vec<&str> = path.split('/').collect();
    for (i, seg) in segs.iter().enumerate() {
        let last = i == segs.len() - 1;
        match *seg {
            "." => {
                if last {
                    out.push("");
                }
            }
            ".." => {
                if out.len() > usize::from(absolute) {
                    out.pop();
                }
                if last {
                    out.push("");
                }
            }
            s => out.push(s),
        }
    }
    let joined = out.join("/");
    if absolute && !joined.starts_with('/') {
        format!("/{joined}")
    } else {
        joined
    }
}

/// Resolve `reference` against `base`.
pub fn resolve(base: &str, reference: &str) -> String {
    if reference.is_empty() {
        return document(base).to_string();
    }
    if is_absolute(reference) {
        let (s, a, p, q) = parts(reference);
        let frag = split_fragment(reference).1;
        return with_fragment(format!("{s}{a}{}{q}", remove_dots(p)), frag);
    }
    let frag = split_fragment(reference).1;
    let r = document(reference);
    let (bs, ba, bp, bq) = parts(base);
    let joined = if r.is_empty() {
        format!("{bs}{ba}{bp}{bq}")
    } else if let Some(rest) = r.strip_prefix("//") {
        format!("{bs}//{rest}")
    } else if r.starts_with('/') {
        format!("{bs}{ba}{}", remove_dots(r))
    } else if r.starts_with('?') {
        format!("{bs}{ba}{bp}{r}")
    } else {
        let dir = match bp.rfind('/') {
            Some(i) => &bp[..=i],
            None if !ba.is_empty() => "/",
            None => "",
        };
        let (rp, rq) = match r.find('?') {
            Some(i) => r.split_at(i),
            None => (r, ""),
        };
        format!("{bs}{ba}{}{rq}", remove_dots(&format!("{dir}{rp}")))
    };
    with_fragment(joined, frag)
}

fn with_fragment(uri: String, frag: Option<&str>) -> String {
    match frag {
        Some(f) if !f.is_empty() => format!("{uri}#{f}"),
        _ => uri,
    }
}

/// Decode `%xx` escapes (JSON pointers in URI fragments).
pub fn percent_decode(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() + 1 {
            if let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The `project:/` URI of a project-relative path.
pub fn project(path: &str) -> String {
    format!(
        "project:/{}",
        path.trim_start_matches("./").trim_start_matches('/')
    )
}

/// The project path of a `project:/` URI.
pub fn project_path(uri: &str) -> Option<&str> {
    document(uri).strip_prefix("project:/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_references() {
        let b = "https://json.schemastore.org/package.json";
        assert_eq!(
            resolve(b, "eslintrc.json"),
            "https://json.schemastore.org/eslintrc.json"
        );
        assert_eq!(
            resolve(b, "#/definitions/x"),
            "https://json.schemastore.org/package.json#/definitions/x"
        );
        assert_eq!(
            resolve(b, "../a/b.json#c"),
            "https://json.schemastore.org/a/b.json#c"
        );
        assert_eq!(resolve(b, "/x.json"), "https://json.schemastore.org/x.json");
        assert_eq!(resolve(b, "https://other.org/s"), "https://other.org/s");
        assert_eq!(
            resolve("project:/config/a.json", "./defs/b.json"),
            "project:/config/defs/b.json"
        );
        assert_eq!(
            resolve("project:/config/a.json", "../b.json#/x"),
            "project:/b.json#/x"
        );
        assert_eq!(resolve("project:/a.json", "#"), "project:/a.json");
        assert_eq!(
            resolve("http://json-schema.org/draft-07/schema#", ""),
            "http://json-schema.org/draft-07/schema"
        );
        assert_eq!(resolve("https://h.org", "s.json"), "https://h.org/s.json");
        assert!(!is_absolute("C:\\x.json"));
        assert_eq!(percent_decode("a%25b%7E"), "a%b~");
        assert_eq!(project("./x/y.json"), "project:/x/y.json");
        assert_eq!(project_path("project:/x/y.json#/a"), Some("x/y.json"));
    }
}
