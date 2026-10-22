//! What an interpreter is: version, php.ini, Xdebug and extensions, parsed from the output
//! of `php -v`, `php --ini` and `php -m` (the Reload button of the CLI Interpreters
//! dialog), and `php -i` for "Show phpinfo".

use serde::{Deserialize, Serialize};

/// The arguments of the three probe runs, in the order [`InterpreterInfo::from_outputs`]
/// takes their output.
pub const PROBES: [&[&str]; 3] = [&["-v"], &["--ini"], &["-m"]];

/// "Show phpinfo".
pub const PHPINFO: &[&str] = &["-i"];

/// The detected properties of an interpreter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterpreterInfo {
    /// `8.3.12`.
    pub version: String,
    /// "Configuration file": the loaded php.ini (empty when none is loaded).
    pub config_file: String,
    /// Additional .ini files, comma separated.
    pub additional_ini: String,
    /// Xdebug's version (`3.3.2`); `"unknown"` when the module is loaded but `php -v` does
    /// not name its version; empty when it is not loaded.
    pub xdebug: String,
    /// Loaded PHP and Zend modules.
    pub extensions: Vec<String>,
}

/// `PHP 8.3.12 (cli) …` → `8.3.12`; also accepts `8.4.0RC1` and `8.2.10-1ubuntu1`.
pub fn parse_version(php_v: &str) -> Option<String> {
    let line = php_v.lines().map(str::trim).find(|l| l.starts_with("PHP "))?;
    let token = line[4..].split_whitespace().next()?;
    let end = token
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(token.len());
    let version = token[..end].trim_end_matches('.');
    (version.split('.').count() >= 2).then(|| version.to_string())
}

/// `with Xdebug v3.3.2, Copyright …` in `php -v`.
pub fn parse_xdebug(php_v: &str) -> Option<String> {
    php_v.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("with Xdebug v")?;
        let v: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
            .collect();
        (!v.is_empty()).then_some(v)
    })
}

/// `major.minor` of a version string.
pub fn major_minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor_text: String = parts
        .next()?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    Some((major, minor_text.parse().ok()?))
}

/// Loaded modules of `php -m` (both sections), in order, without duplicates.
pub fn parse_modules(php_m: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in php_m.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('[') || line.starts_with("PHP Warning") {
            continue;
        }
        if !out.iter().any(|m| m.eq_ignore_ascii_case(line)) {
            out.push(line.to_string());
        }
    }
    out
}

/// `(loaded php.ini, additional .ini files)` from `php --ini`; `(none)` counts as empty.
pub fn parse_ini(php_ini: &str) -> (String, String) {
    let mut loaded = String::new();
    let mut additional: Vec<String> = Vec::new();
    let mut in_additional = false;
    for line in php_ini.lines() {
        let trimmed = line.trim();
        if let Some(v) = trimmed.strip_prefix("Loaded Configuration File:") {
            loaded = clean(v);
            in_additional = false;
        } else if let Some(v) = trimmed.strip_prefix("Additional .ini files parsed:") {
            in_additional = true;
            additional.extend(split_files(v));
        } else if trimmed.contains(':') && !trimmed.starts_with('/') && !is_drive(trimmed) {
            in_additional = false;
        } else if in_additional {
            additional.extend(split_files(trimmed));
        }
    }
    (loaded, additional.join(", "))
}

fn is_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

fn clean(v: &str) -> String {
    match v.trim() {
        "(none)" => String::new(),
        v => v.to_string(),
    }
}

fn split_files(v: &str) -> Vec<String> {
    v.split(',')
        .map(clean)
        .filter(|f| !f.is_empty())
        .collect()
}

impl InterpreterInfo {
    /// From the output of the [`PROBES`] (`php -v`, `php --ini`, `php -m`). Fails when
    /// `php -v` does not look like PHP.
    pub fn from_outputs(php_v: &str, php_ini: &str, php_m: &str) -> Result<Self, String> {
        let version = parse_version(php_v).ok_or_else(|| {
            let first = php_v.lines().map(str::trim).find(|l| !l.is_empty());
            match first {
                Some(l) => format!("Not a PHP interpreter: {l}"),
                None => "The interpreter printed no version".to_string(),
            }
        })?;
        let extensions = parse_modules(php_m);
        let xdebug = parse_xdebug(php_v).unwrap_or_else(|| {
            if extensions.iter().any(|e| e.eq_ignore_ascii_case("xdebug")) {
                "unknown".into()
            } else {
                String::new()
            }
        });
        let (config_file, additional_ini) = parse_ini(php_ini);
        Ok(Self {
            version,
            config_file,
            additional_ini,
            xdebug,
            extensions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHP_V: &str = "PHP 8.3.12 (cli) (built: Sep 24 2024 12:00:00) (NTS)\n\
Copyright (c) The PHP Group\n\
Zend Engine v4.3.12, Copyright (c) Zend Technologies\n    \
with Zend OPcache v8.3.12, Copyright (c), by Zend Technologies\n    \
with Xdebug v3.3.2, Copyright (c) 2002-2024, by Derick Rethans\n";

    const PHP_INI: &str = "Configuration File (php.ini) Path: /etc/php/8.3/cli\n\
Loaded Configuration File:         /etc/php/8.3/cli/php.ini\n\
Scan for additional .ini files in: /etc/php/8.3/cli/conf.d\n\
Additional .ini files parsed:      /etc/php/8.3/cli/conf.d/10-opcache.ini,\n\
/etc/php/8.3/cli/conf.d/20-xdebug.ini\n";

    const PHP_M: &str = "[PHP Modules]\nCore\nctype\nmbstring\nxdebug\n\n[Zend Modules]\nXdebug\nZend OPcache\n";

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version(PHP_V).as_deref(), Some("8.3.12"));
        assert_eq!(
            parse_version("PHP 8.2.10-1ubuntu1 (cli)").as_deref(),
            Some("8.2.10")
        );
        assert_eq!(parse_version("PHP 8.4.0RC1 (cli)").as_deref(), Some("8.4.0"));
        assert_eq!(parse_version("Python 3.12"), None);
        assert_eq!(parse_xdebug(PHP_V).as_deref(), Some("3.3.2"));
        assert_eq!(major_minor("8.3.12"), Some((8, 3)));
        assert_eq!(major_minor("8.4.0RC1"), Some((8, 4)));
        assert_eq!(major_minor("8"), None);
    }

    #[test]
    fn parses_ini_and_modules() {
        let (loaded, additional) = parse_ini(PHP_INI);
        assert_eq!(loaded, "/etc/php/8.3/cli/php.ini");
        assert_eq!(
            additional,
            "/etc/php/8.3/cli/conf.d/10-opcache.ini, /etc/php/8.3/cli/conf.d/20-xdebug.ini"
        );
        let none = "Loaded Configuration File:         (none)\nAdditional .ini files parsed:      (none)\n";
        assert_eq!(parse_ini(none), (String::new(), String::new()));
        assert_eq!(
            parse_modules(PHP_M),
            ["Core", "ctype", "mbstring", "xdebug", "Zend OPcache"]
        );
    }

    #[test]
    fn combines_the_probes() {
        let info = InterpreterInfo::from_outputs(PHP_V, PHP_INI, PHP_M).unwrap();
        assert_eq!(info.version, "8.3.12");
        assert_eq!(info.xdebug, "3.3.2");
        assert_eq!(info.config_file, "/etc/php/8.3/cli/php.ini");
        let plain = InterpreterInfo::from_outputs("PHP 8.1.2 (cli)", "", "[PHP Modules]\nxdebug\n")
            .unwrap();
        assert_eq!(plain.xdebug, "unknown");
        let err = InterpreterInfo::from_outputs("bash: php: command not found", "", "").unwrap_err();
        assert!(err.contains("command not found"), "{err}");
    }
}
