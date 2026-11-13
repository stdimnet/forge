//! rustfmt for Reformat Code and "Reformat code" on save: the edition of the file's package
//! and the project's `rustfmt.toml` / `.rustfmt.toml` (the nearest one above the file, like
//! `cargo fmt`), formatting the editor's text on stdin.

/// Config files rustfmt would use for a file in `dir` (project relative), nearest first.
pub fn config_candidates(dir: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut d = dir.trim_matches('/').to_string();
    loop {
        for name in ["rustfmt.toml", ".rustfmt.toml"] {
            out.push(if d.is_empty() {
                name.to_string()
            } else {
                format!("{d}/{name}")
            });
        }
        if d.is_empty() {
            break;
        }
        d = d
            .rsplit_once('/')
            .map(|(p, _)| p.to_string())
            .unwrap_or_default();
    }
    out
}

/// rustfmt's arguments for stdin input.
pub fn rustfmt_args(edition: &str, config: Option<&str>, extra: &[String]) -> Vec<String> {
    let mut args = vec!["--emit".to_string(), "stdout".into()];
    let edition = if edition.trim().is_empty() {
        "2021"
    } else {
        edition.trim()
    };
    args.extend(["--edition".to_string(), edition.to_string()]);
    if let Some(c) = config {
        args.extend(["--config-path".to_string(), c.to_string()]);
    }
    args.extend(extra.iter().cloned());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_config_first() {
        assert_eq!(
            config_candidates("crates/shop-core/src"),
            [
                "crates/shop-core/src/rustfmt.toml",
                "crates/shop-core/src/.rustfmt.toml",
                "crates/shop-core/rustfmt.toml",
                "crates/shop-core/.rustfmt.toml",
                "crates/rustfmt.toml",
                "crates/.rustfmt.toml",
                "rustfmt.toml",
                ".rustfmt.toml",
            ]
        );
        assert_eq!(config_candidates(""), ["rustfmt.toml", ".rustfmt.toml"]);
        assert_eq!(
            rustfmt_args("2024", Some("/w/rustfmt.toml"), &[]),
            [
                "--emit",
                "stdout",
                "--edition",
                "2024",
                "--config-path",
                "/w/rustfmt.toml"
            ]
        );
        assert_eq!(rustfmt_args("", None, &[])[3], "2021");
    }
}
