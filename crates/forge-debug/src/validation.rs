//! "Web Server Debug Validation" (PHPStorm's "Validate Debugger Configuration on Web
//! Server"): what the PHP that runs a request or a CLI script reports about Xdebug, and
//! whether that lets it connect to Forge.
//!
//! A web server is asked through a small script placed in its document root
//! ([`VALIDATION_SCRIPT`], removed again afterwards); a CLI interpreter through `php -i`
//! ([`parse_phpinfo`]). [`validate`] turns either into ✓/✗ checks with hints.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The script that reports the web server's Xdebug configuration as JSON.
pub const VALIDATION_SCRIPT: &str = r#"<?php
// Created by Forge (Run | Web Server Debug Validation). Forge deletes it after the check.
header('Content-Type: application/json');
$loaded = extension_loaded('xdebug');
$keys = ['xdebug.mode', 'xdebug.client_host', 'xdebug.client_port', 'xdebug.start_with_request',
    'xdebug.discover_client_host', 'xdebug.idekey', 'xdebug.remote_enable', 'xdebug.remote_host',
    'xdebug.remote_port', 'xdebug.remote_autostart'];
$ini = [];
foreach ($keys as $k) {
    $v = ini_get($k);
    if ($v !== false) {
        $ini[$k] = (string) $v;
    }
}
$mode = null;
if ($loaded && function_exists('xdebug_info')) {
    $m = @xdebug_info('mode');
    $mode = is_array($m) ? implode(',', $m) : $m;
}
echo json_encode([
    'forgeValidation' => 1,
    'phpVersion' => PHP_VERSION,
    'sapi' => PHP_SAPI,
    'phpIni' => (string) php_ini_loaded_file(),
    'xdebug' => $loaded ? (string) phpversion('xdebug') : null,
    'ini' => $ini,
    'mode' => $mode,
    'remoteAddr' => $_SERVER['REMOTE_ADDR'] ?? null,
    'serverName' => $_SERVER['SERVER_NAME'] ?? null,
    'serverPort' => isset($_SERVER['SERVER_PORT']) ? (string) $_SERVER['SERVER_PORT'] : null,
    'documentRoot' => $_SERVER['DOCUMENT_ROOT'] ?? null,
]);
"#;

/// What PHP reported about itself and Xdebug.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct XdebugEnvironment {
    pub php_version: String,
    /// `cli`, `fpm-fcgi`, `apache2handler`, `cli-server`.
    pub sapi: String,
    /// The loaded php.ini (empty: none).
    pub php_ini: String,
    /// Xdebug's version; `None` when the extension is not loaded.
    pub xdebug: Option<String>,
    /// `xdebug.*` settings.
    pub ini: BTreeMap<String, String>,
    /// The effective mode (`xdebug_info('mode')`, includes `XDEBUG_MODE`).
    pub mode: Option<String>,
    pub remote_addr: Option<String>,
    pub server_name: Option<String>,
    pub server_port: Option<String>,
    pub document_root: Option<String>,
}

/// File name of the validation script (unique per run so a stale one is never reused).
pub fn script_name(token: u64) -> String {
    format!("forge_xdebug_validator_{token:x}.php")
}

/// URL of the script under the document root's URL (`http://shop.test/`).
pub fn script_url(base_url: &str, script: &str) -> String {
    let base = base_url.trim();
    let base = if base.contains("://") {
        base.to_string()
    } else {
        format!("http://{base}")
    };
    format!("{}/{script}", base.trim_end_matches('/'))
}

/// Parse what [`VALIDATION_SCRIPT`] printed (a notice or a framework banner before the
/// JSON is skipped).
pub fn parse_script_output(body: &str) -> Result<XdebugEnvironment, String> {
    let start = body.find("{\"forgeValidation\"").ok_or_else(|| {
        let snippet: String = body.trim().chars().take(200).collect();
        if snippet.contains("<?php") {
            "The web server returned the script's source: PHP does not run for this URL.".into()
        } else if snippet.is_empty() {
            "The web server returned an empty response.".into()
        } else {
            format!("The validation script did not answer. The server returned: {snippet}")
        }
    })?;
    let mut de = serde_json::Deserializer::from_str(&body[start..]);
    let env = XdebugEnvironment::deserialize(&mut de).map_err(|e| e.to_string())?;
    Ok(env)
}

/// Parse `php -i` (text output) of a CLI interpreter.
pub fn parse_phpinfo(text: &str) -> XdebugEnvironment {
    let mut env = XdebugEnvironment::default();
    let mut in_xdebug = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line == "xdebug" {
            in_xdebug = true;
            continue;
        }
        let parts: Vec<&str> = line.split(" => ").map(str::trim).collect();
        if parts.len() < 2 {
            // Module sections are a single word; anything else ends the Xdebug block.
            if !line.is_empty() && !line.contains(' ') {
                in_xdebug = false;
            }
            if let Some(v) = line
                .strip_prefix("with Xdebug v")
                .or_else(|| line.split("with Xdebug v").nth(1))
            {
                let v: String = v
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
                    .collect();
                if !v.is_empty() && env.xdebug.is_none() {
                    env.xdebug = Some(v);
                }
            }
            continue;
        }
        let (key, value) = (parts[0], parts[1]);
        match key {
            "PHP Version" if env.php_version.is_empty() => env.php_version = value.into(),
            "Server API" => {
                env.sapi = match value {
                    "Command Line Interface" => "cli".into(),
                    other => other.to_lowercase(),
                }
            }
            "Loaded Configuration File" => {
                env.php_ini = if value == "(none)" {
                    String::new()
                } else {
                    value.into()
                }
            }
            "Version" if in_xdebug => env.xdebug = Some(value.into()),
            k if k.starts_with("xdebug.") => {
                env.xdebug.get_or_insert_with(|| "unknown".into());
                let v = if value == "no value" { "" } else { value };
                env.ini.insert(k.into(), v.into());
            }
            _ => {}
        }
    }
    env
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckStatus {
    Ok,
    Warning,
    Error,
    /// Nothing to check (e.g. Forge sets it for debug runs).
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub title: String,
    pub value: String,
    pub status: CheckStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub checks: Vec<Check>,
    /// No check failed.
    pub ok: bool,
    pub environment: XdebugEnvironment,
}

/// What the checks compare against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expectation {
    /// The port Forge listens on (`php.debug.port`).
    pub ide_port: u16,
    /// A CLI interpreter that Forge starts itself: debug runs set `XDEBUG_MODE`,
    /// `XDEBUG_CONFIG` and `XDEBUG_SESSION`, so only the extension matters.
    pub cli: bool,
}

fn check(title: &str, value: impl Into<String>, status: CheckStatus, hint: Option<&str>) -> Check {
    Check {
        title: title.into(),
        value: value.into(),
        status,
        hint: hint.map(str::to_string),
    }
}

fn is_loopback(host: &str) -> bool {
    let h = host.trim().trim_start_matches('[').trim_end_matches(']');
    h.eq_ignore_ascii_case("localhost") || h.starts_with("127.") || h == "::1"
}

fn on(v: Option<&String>) -> bool {
    matches!(
        v.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "on" | "yes" | "true")
    )
}

/// The checks of a validation, in PHPStorm's order.
pub fn validate(env: &XdebugEnvironment, expect: Expectation) -> ValidationReport {
    use CheckStatus::*;
    let mut checks = Vec::new();
    if !env.php_version.is_empty() {
        let sapi = if env.sapi.is_empty() {
            String::new()
        } else {
            format!(" ({})", env.sapi)
        };
        checks.push(check(
            "PHP version",
            format!("{}{sapi}", env.php_version),
            Info,
            None,
        ));
    }
    checks.push(check(
        "Loaded php.ini",
        if env.php_ini.is_empty() {
            "(none)"
        } else {
            env.php_ini.as_str()
        },
        Info,
        None,
    ));
    let Some(version) = env.xdebug.as_deref() else {
        checks.push(check(
            "Debug extension",
            "Xdebug is not installed",
            Error,
            Some(if expect.cli {
                "Install Xdebug 3 for this interpreter, or set its \"Debugger extension\" in the CLI Interpreters dialog."
            } else {
                "Install Xdebug 3 (e.g. pecl install xdebug) and load it with zend_extension=xdebug in the php.ini of the web server, then restart it."
            }),
        ));
        return finish(checks, env);
    };
    if version.starts_with("2.") {
        checks.push(check(
            "Debug extension",
            format!("Xdebug {version}"),
            Error,
            Some("Xdebug 2 is not supported. Upgrade to Xdebug 3."),
        ));
        return finish(checks, env);
    }
    checks.push(check(
        "Debug extension",
        format!("Xdebug {version}"),
        Ok,
        None,
    ));

    let ini = |k: &str| {
        env.ini
            .get(k)
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    let mode = env.mode.clone().unwrap_or_else(|| ini("xdebug.mode"));
    let has_debug = mode.split(',').any(|m| m.trim() == "debug");
    checks.push(if expect.cli {
        check(
            "xdebug.mode",
            if mode.is_empty() {
                "develop".into()
            } else {
                mode.clone()
            },
            if has_debug { Ok } else { Info },
            (!has_debug).then_some("Debug runs start PHP with XDEBUG_MODE=debug."),
        )
    } else if has_debug {
        check("xdebug.mode", mode, Ok, None)
    } else {
        check(
            "xdebug.mode",
            if mode.is_empty() {
                "develop".into()
            } else {
                mode
            },
            Error,
            Some("Step debugging is off. Set xdebug.mode=debug (or debug,develop) in php.ini."),
        )
    });

    let discover = on(env.ini.get("xdebug.discover_client_host"));
    let host = ini("xdebug.client_host");
    let host_shown = if host.is_empty() {
        "localhost".to_string()
    } else {
        host.clone()
    };
    let remote_client = env.remote_addr.as_deref().is_some_and(|a| !is_loopback(a));
    checks.push(if expect.cli {
        check(
            "xdebug.client_host",
            host_shown,
            Info,
            Some("Debug runs point Xdebug at Forge (host.docker.internal in containers, an SSH tunnel over SSH)."),
        )
    } else if discover {
        check(
            "xdebug.client_host",
            format!("{host_shown} (xdebug.discover_client_host is on)"),
            Ok,
            Some("Xdebug connects back to the address the request came from."),
        )
    } else if remote_client && is_loopback(&host_shown) {
        check(
            "xdebug.client_host",
            host_shown,
            Warning,
            Some("The web server runs on another machine or in a container, so localhost is not Forge. Set xdebug.client_host to this machine's address (host.docker.internal for Docker) or enable xdebug.discover_client_host."),
        )
    } else {
        check("xdebug.client_host", host_shown, Ok, None)
    });

    let port: Option<u16> = ini("xdebug.client_port").parse().ok();
    let port_shown = port.map_or("9003".to_string(), |p| p.to_string());
    let port_ok = port.unwrap_or(9003) == expect.ide_port;
    let port_hint = format!(
        "Forge listens on port {} (Settings | PHP | Debug). Set xdebug.client_port={} or change the debug port.",
        expect.ide_port, expect.ide_port
    );
    checks.push(if expect.cli {
        check(
            "xdebug.client_port",
            port_shown,
            Info,
            Some("Debug runs pass Forge's port in XDEBUG_CONFIG."),
        )
    } else if port_ok {
        check("xdebug.client_port", port_shown, Ok, None)
    } else {
        check("xdebug.client_port", port_shown, Error, Some(&port_hint))
    });

    let start = ini("xdebug.start_with_request").to_ascii_lowercase();
    let start_shown = if start.is_empty() {
        "default".to_string()
    } else {
        start.clone()
    };
    checks.push(match start.as_str() {
        _ if expect.cli => check(
            "xdebug.start_with_request",
            start_shown,
            Info,
            Some("Debug runs set XDEBUG_SESSION, which starts the session."),
        ),
        "yes" => check("xdebug.start_with_request", start_shown, Ok, Some("Every request starts a debug session.")),
        "no" => check(
            "xdebug.start_with_request",
            start_shown,
            Error,
            Some("Xdebug never starts a session. Set xdebug.start_with_request=trigger (or yes)."),
        ),
        _ => check(
            "xdebug.start_with_request",
            start_shown,
            Warning,
            Some("Only requests with the XDEBUG_SESSION cookie or ?XDEBUG_SESSION_START=FORGE are debugged. PHP Web Page and PHP HTTP Request debug configurations and Debug in .http files add it; set yes to debug every request."),
        ),
    });
    if !expect.cli {
        if let Some(name) = env.server_name.as_deref().filter(|n| !n.is_empty()) {
            let shown = match env.server_port.as_deref() {
                Some(p) if p != "80" && p != "443" && !p.is_empty() => format!("{name}:{p}"),
                _ => name.to_string(),
            };
            checks.push(check(
                "Server name",
                shown,
                Info,
                Some("Matched against Settings | PHP | Servers to map the server's paths to the project."),
            ));
        }
    }
    finish(checks, env)
}

fn finish(checks: Vec<Check>, env: &XdebugEnvironment) -> ValidationReport {
    ValidationReport {
        ok: !checks.iter().any(|c| c.status == CheckStatus::Error),
        checks,
        environment: env.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEB: Expectation = Expectation {
        ide_port: 9003,
        cli: false,
    };

    fn status(r: &ValidationReport, title: &str) -> CheckStatus {
        r.checks.iter().find(|c| c.title == title).unwrap().status
    }

    #[test]
    fn parses_the_script_output_after_noise() {
        let body = r#"Notice: something
{"forgeValidation":1,"phpVersion":"8.3.4","sapi":"fpm-fcgi","phpIni":"/etc/php/8.3/fpm/php.ini","xdebug":"3.3.2","ini":{"xdebug.mode":"debug","xdebug.client_host":"localhost","xdebug.client_port":"9003","xdebug.start_with_request":"trigger"},"mode":"debug","remoteAddr":"127.0.0.1","serverName":"shop.test","serverPort":"80","documentRoot":"/var/www/public"}"#;
        let env = parse_script_output(body).unwrap();
        assert_eq!(env.xdebug.as_deref(), Some("3.3.2"));
        assert_eq!(env.ini["xdebug.start_with_request"], "trigger");
        let r = validate(&env, WEB);
        assert!(r.ok);
        assert_eq!(status(&r, "xdebug.mode"), CheckStatus::Ok);
        assert_eq!(status(&r, "xdebug.client_port"), CheckStatus::Ok);
        assert_eq!(
            status(&r, "xdebug.start_with_request"),
            CheckStatus::Warning
        );
        assert_eq!(status(&r, "Server name"), CheckStatus::Info);

        assert!(parse_script_output("<?php echo 1;")
            .unwrap_err()
            .contains("PHP does not run"));
        assert!(parse_script_output("").unwrap_err().contains("empty"));
    }

    #[test]
    fn reports_missing_xdebug_wrong_port_and_mode() {
        let none = XdebugEnvironment {
            php_version: "8.2.1".into(),
            ..Default::default()
        };
        let r = validate(&none, WEB);
        assert!(!r.ok);
        assert_eq!(status(&r, "Debug extension"), CheckStatus::Error);
        assert!(r.checks.iter().all(|c| !c.title.starts_with("xdebug.")));

        let mut env = XdebugEnvironment {
            xdebug: Some("3.2.0".into()),
            remote_addr: Some("172.18.0.1".into()),
            ..Default::default()
        };
        env.ini.insert("xdebug.mode".into(), "develop".into());
        env.ini.insert("xdebug.client_port".into(), "9000".into());
        env.ini
            .insert("xdebug.start_with_request".into(), "yes".into());
        let r = validate(&env, WEB);
        assert!(!r.ok);
        assert_eq!(status(&r, "xdebug.mode"), CheckStatus::Error);
        assert_eq!(status(&r, "xdebug.client_port"), CheckStatus::Error);
        assert_eq!(status(&r, "xdebug.client_host"), CheckStatus::Warning);
        assert_eq!(status(&r, "xdebug.start_with_request"), CheckStatus::Ok);
        // The effective mode (XDEBUG_MODE) wins over the ini value.
        env.mode = Some("debug,develop".into());
        env.ini
            .insert("xdebug.discover_client_host".into(), "1".into());
        let r = validate(&env, WEB);
        assert_eq!(status(&r, "xdebug.mode"), CheckStatus::Ok);
        assert_eq!(status(&r, "xdebug.client_host"), CheckStatus::Ok);

        env.xdebug = Some("2.9.8".into());
        assert!(!validate(&env, WEB).ok);
    }

    #[test]
    fn parses_cli_phpinfo() {
        let text = "phpinfo()
PHP Version => 8.3.12

System => Linux
Server API => Command Line Interface
Loaded Configuration File => /etc/php/8.3/cli/php.ini

xdebug

Version => 3.3.2
xdebug.client_host => localhost => localhost
xdebug.client_port => 9003 => 9003
xdebug.mode => develop => develop
xdebug.start_with_request => default => default
xdebug.idekey => no value => no value

zlib
";
        let env = parse_phpinfo(text);
        assert_eq!(env.php_version, "8.3.12");
        assert_eq!(env.sapi, "cli");
        assert_eq!(env.php_ini, "/etc/php/8.3/cli/php.ini");
        assert_eq!(env.xdebug.as_deref(), Some("3.3.2"));
        assert_eq!(env.ini["xdebug.mode"], "develop");
        assert_eq!(env.ini["xdebug.idekey"], "");
        let r = validate(
            &env,
            Expectation {
                ide_port: 9010,
                cli: true,
            },
        );
        // Forge sets mode, host, port and session for debug runs.
        assert!(r.ok);
        assert_eq!(status(&r, "xdebug.mode"), CheckStatus::Info);
        assert_eq!(status(&r, "xdebug.client_port"), CheckStatus::Info);

        let without = parse_phpinfo("PHP Version => 8.1.2\nServer API => Command Line Interface\n");
        assert_eq!(without.xdebug, None);
        assert!(
            !validate(
                &without,
                Expectation {
                    ide_port: 9003,
                    cli: true
                }
            )
            .ok
        );
    }

    #[test]
    fn script_names_and_urls() {
        assert_eq!(script_name(255), "forge_xdebug_validator_ff.php");
        assert_eq!(script_url("shop.test/", "v.php"), "http://shop.test/v.php");
        assert_eq!(
            script_url("https://shop.test:8443", "v.php"),
            "https://shop.test:8443/v.php"
        );
    }
}
