//! `/`-separated relative paths (MSBuild and solutions write `\`).

/// Backslashes to `/`, without `./` segments and with `..` resolved where possible.
pub fn normalize(path: &str) -> String {
    let p = path.trim().replace('\\', "/");
    let absolute = p.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if matches!(out.last(), Some(s) if *s != "..") {
                    out.pop();
                } else if !absolute {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    let joined = out.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

/// The folder of a path (`""` for a file in the root).
pub fn parent(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((d, _)) => d.to_string(),
        None => String::new(),
    }
}

/// `dir/rel`, normalized; absolute `rel` stays as it is.
pub fn join(dir: &str, rel: &str) -> String {
    let rel = rel.replace('\\', "/");
    if rel.starts_with('/') || dir.is_empty() || rel.as_bytes().get(1) == Some(&b':') {
        return normalize(&rel);
    }
    normalize(&format!("{dir}/{rel}"))
}

/// The file name of a path.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// `root`-relative form of an absolute path (`None` outside the root).
pub fn relative_to(root: &str, path: &str) -> Option<String> {
    let root = normalize(root);
    let path = normalize(path);
    if root.is_empty() {
        return Some(path);
    }
    let lower = |s: &str| {
        if cfg!(windows) || s.as_bytes().get(1) == Some(&b':') {
            s.to_ascii_lowercase()
        } else {
            s.to_string()
        }
    };
    let (r, p) = (lower(&root), lower(&path));
    if p == r {
        return Some(String::new());
    }
    p.strip_prefix(&format!("{}/", r.trim_end_matches('/')))
        .map(|_| path[root.trim_end_matches('/').len() + 1..].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(normalize(".\\src\\..\\lib\\A.csproj"), "lib/A.csproj");
        assert_eq!(normalize("../x/./y"), "../x/y");
        assert_eq!(normalize("/a/b/../c"), "/a/c");
        assert_eq!(parent("a/b/c.cs"), "a/b");
        assert_eq!(parent("c.cs"), "");
        assert_eq!(
            join("src/App", "..\\Core\\Core.csproj"),
            "src/Core/Core.csproj"
        );
        assert_eq!(join("", "x.cs"), "x.cs");
        assert_eq!(join("a", "/abs/x"), "/abs/x");
        assert_eq!(file_name("a\\b/c.sln"), "c.sln");
        assert_eq!(
            relative_to("/p/shop", "/p/shop/src/A.cs").as_deref(),
            Some("src/A.cs")
        );
        assert_eq!(relative_to("/p/shop", "/p/other/A.cs"), None);
        assert_eq!(relative_to("C:/p", "c:\\p\\x.cs").as_deref(), Some("x.cs"));
    }
}
