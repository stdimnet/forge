//! The Spring Boot run configuration (`spring-boot`): main class, active profiles, override
//! properties and the actuator endpoints the run dashboard reads. It runs as JavaForge's
//! Application configuration with these turned into VM options and program arguments.
//! [`Observed`] follows the console output of a run: port, context path, profiles, PID,
//! startup time and failures.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The run configuration type id.
pub const TYPE: &str = "spring-boot";

/// What the dashboard asks the actuator to expose unless the configuration says otherwise.
pub const EXPOSED: &str = "health,info,beans,mappings";

/// Active profiles of a configuration (`activeProfiles`: `dev, local`).
pub fn profiles(options: &BTreeMap<String, String>) -> Vec<String> {
    options
        .get("activeProfiles")
        .map(|p| p.split([',', ' ']).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect())
        .unwrap_or_default()
}

/// Override properties (`overrideProperties`: one `key=value` per line).
pub fn overrides(options: &BTreeMap<String, String>) -> Vec<(String, String)> {
    options
        .get("overrideProperties")
        .map(|o| {
            o.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(|l| match l.split_once('=') {
                    Some((k, v)) => (k.trim().to_string(), v.trim().to_string()),
                    None => (l.to_string(), String::new()),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn quote(arg: &str) -> String {
    if arg.chars().any(|c| c.is_whitespace() || c == '"' || c == '\'') {
        format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

/// VM options and program arguments of a Spring Boot run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Launch {
    pub vm_options: String,
    pub arguments: String,
}

/// Turn a Spring Boot configuration into an Application run: profiles as
/// `-Dspring.profiles.active`, colored output, override properties as `--key=value`
/// arguments (they win over every configuration file) and, unless turned off
/// (`exposeActuator = false`), the actuator endpoints of the run dashboard.
pub fn launch(options: &BTreeMap<String, String>, vm_options: &str, arguments: &str) -> Launch {
    let mut vm: Vec<String> = Vec::new();
    if !vm_options.trim().is_empty() {
        vm.push(vm_options.trim().to_string());
    }
    let profiles = profiles(options);
    if !profiles.is_empty() && !vm_options.contains("-Dspring.profiles.active") {
        vm.push(format!("-Dspring.profiles.active={}", profiles.join(",")));
    }
    if !vm_options.contains("spring.output.ansi.enabled") {
        vm.push("-Dspring.output.ansi.enabled=always".into());
    }
    let mut args: Vec<String> = Vec::new();
    if !arguments.trim().is_empty() {
        args.push(arguments.trim().to_string());
    }
    let overrides = overrides(options);
    let has = |key: &str| overrides.iter().any(|(k, _)| crate::config::canonical(k) == key) || arguments.contains(key) || vm_options.contains(key);
    for (k, v) in &overrides {
        args.push(quote(&format!("--{k}={v}")));
    }
    if options.get("exposeActuator").map(String::as_str) != Some("false") {
        if !has("management.endpoints.web.exposure.include") {
            args.push(format!("--management.endpoints.web.exposure.include={EXPOSED}"));
        }
        if !has("management.endpoint.health.show-details") {
            args.push("--management.endpoint.health.show-details=always".into());
        }
    }
    Launch {
        vm_options: vm.join(" "),
        arguments: args.join(" "),
    }
}

/// Remove ANSI escape sequences.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// What the console of a run told about the application.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Observed {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_path: Option<String>,
    /// `tomcat`, `netty`, `jetty`, `undertow`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub started: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_seconds: Option<f64>,
    pub failed: bool,
    /// The Spring Boot version from the banner (`:: Spring Boot ::  (v3.3.2)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_version: Option<String>,
}

fn number_after(s: &str, marker: &str) -> Option<u64> {
    let rest = &s[s.find(marker)? + marker.len()..];
    let rest = rest.trim_start_matches(['(', 's', ')', ':', ' ']);
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn quoted_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|p| p.trim().trim_matches('"').trim())
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

impl Observed {
    /// Read one console line; returns whether something changed.
    pub fn observe(&mut self, raw: &str) -> bool {
        let line = strip_ansi(raw);
        let before = self.clone();
        if let Some(i) = line.find(" started on port") {
            let server = line[..i].rsplit([' ', '.']).next().unwrap_or_default().to_ascii_lowercase();
            if self.port.is_none() {
                self.port = number_after(&line, "started on port").and_then(|p| u16::try_from(p).ok());
                if !server.is_empty() {
                    self.server = Some(server);
                }
                if let Some(c) = line.find("context path '") {
                    let rest = &line[c + "context path '".len()..];
                    let path = rest.split('\'').next().unwrap_or_default();
                    self.context_path = Some(if path == "/" { String::new() } else { path.to_string() });
                }
            }
        } else if let Some(i) = line.find(" profiles are active: ").or_else(|| line.find(" profile is active: ")) {
            let rest = &line[i..];
            let list = rest.split_once(": ").map(|x| x.1).unwrap_or_default();
            self.profiles = quoted_list(list);
        } else if line.contains("No active profile set") {
            self.profiles = vec!["default".into()];
        } else if line.contains(" with PID ") && self.pid.is_none() {
            self.pid = number_after(&line, " with PID ").and_then(|p| u32::try_from(p).ok());
        } else if let Some(i) = line.find(":: Spring Boot ::") {
            let v = line[i..].split("(v").nth(1).and_then(|r| r.split(')').next());
            self.boot_version = v.map(str::to_string);
        } else if line.contains("APPLICATION FAILED TO START") || line.contains("Application run failed") {
            self.failed = true;
        } else if let Some(i) = line.find("Started ") {
            if let Some(secs) = line[i..].split(" in ").nth(1).and_then(|r| r.split(' ').next()).and_then(|n| n.parse::<f64>().ok()) {
                self.started = true;
                self.startup_seconds = Some(secs);
            }
        }
        *self != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(list: &[(&str, &str)]) -> BTreeMap<String, String> {
        list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn launch_options() {
        let o = opts(&[
            ("mainClass", "com.acme.ShopApplication"),
            ("activeProfiles", "dev, local"),
            ("overrideProperties", "server.port=9090\n# comment\nshop.banner=Hello world\n"),
        ]);
        let l = launch(&o, "-Xmx512m", "--trace");
        assert_eq!(l.vm_options, "-Xmx512m -Dspring.profiles.active=dev,local -Dspring.output.ansi.enabled=always");
        assert_eq!(
            l.arguments,
            "--trace --server.port=9090 \"--shop.banner=Hello world\" --management.endpoints.web.exposure.include=health,info,beans,mappings --management.endpoint.health.show-details=always"
        );
        let own = opts(&[("overrideProperties", "management.endpoints.web.exposure.include=*"), ("exposeActuator", "true")]);
        let l = launch(&own, "", "");
        assert_eq!(l.arguments, "--management.endpoints.web.exposure.include=* --management.endpoint.health.show-details=always");
        let off = opts(&[("exposeActuator", "false")]);
        assert_eq!(launch(&off, "", "").arguments, "");
        assert_eq!(profiles(&opts(&[])), Vec::<String>::new());
    }

    #[test]
    fn observes_spring_boot_3_and_2_output() {
        let mut o = Observed::default();
        let lines = [
            " :: Spring Boot ::                (v3.3.2)",
            "2024-07-31T10:00:00.000+02:00  INFO 4711 --- [shop] [           main] com.acme.ShopApplication                 : Starting ShopApplication using Java 21.0.4 with PID 4711 (/srv/shop/build/classes/java/main started by dev in /srv/shop)",
            "\u{1b}[32m INFO\u{1b}[0m 4711 --- [main] com.acme.ShopApplication : The following 2 profiles are active: \"dev\", \"local\"",
            "INFO 4711 --- [main] o.s.b.w.embedded.tomcat.TomcatWebServer  : Tomcat initialized with port 8080 (http)",
            "INFO 4711 --- [main] o.s.b.w.embedded.tomcat.TomcatWebServer  : Tomcat started on port 8080 (http) with context path '/shop'",
            "INFO 4711 --- [main] com.acme.ShopApplication                 : Started ShopApplication in 2.345 seconds (process running for 2.9)",
        ];
        for l in lines {
            o.observe(l);
        }
        assert_eq!(o.boot_version.as_deref(), Some("3.3.2"));
        assert_eq!(o.pid, Some(4711));
        assert_eq!(o.profiles, ["dev", "local"]);
        assert_eq!((o.port, o.context_path.as_deref(), o.server.as_deref()), (Some(8080), Some("/shop"), Some("tomcat")));
        assert!(o.started);
        assert_eq!(o.startup_seconds, Some(2.345));
        assert!(!o.observe("INFO  some other line"));

        let mut old = Observed::default();
        old.observe("INFO 1 --- [main] c.a.App : The following profiles are active: dev,local");
        old.observe("INFO 1 --- [main] o.s.b.w.e.netty.NettyWebServer : Netty started on port(s): 8081");
        old.observe("INFO 1 --- [main] o.s.b.w.e.tomcat.TomcatWebServer : Tomcat started on port(s): 9999 (http) with context path ''");
        assert_eq!(old.profiles, ["dev", "local"]);
        assert_eq!((old.port, old.server.as_deref()), (Some(8081), Some("netty")));

        let mut jetty = Observed::default();
        jetty.observe("o.s.b.w.e.jetty.JettyWebServer : Jetty started on port 8080 (http/1.1) with context path '/'");
        assert_eq!((jetty.port, jetty.context_path.as_deref()), (Some(8080), Some("")));
        let mut fail = Observed::default();
        fail.observe("***************************\nAPPLICATION FAILED TO START\n***************************");
        assert!(fail.failed);
        let mut none = Observed::default();
        none.observe("INFO c.a.App : No active profile set, falling back to 1 default profile: \"default\"");
        assert_eq!(none.profiles, ["default"]);
    }
}
