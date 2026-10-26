//! Command lines of Java programs: the "Application" run configuration (`java` with VM
//! options, class or module path, main class and program arguments) and the JDWP agent of
//! debug runs and "Remote JVM Debug".

use forge_runtools::args::split_args;
use serde::{Deserialize, Serialize};

/// The environment variable a debug run sets to ask for a JDWP agent on that port (the
/// backend turns it into the right arguments for `java`, Gradle or Maven).
pub const DEBUG_PORT_ENV: &str = "FORGE_JAVA_DEBUG_PORT";

/// Gradle's `--debug-jvm` always listens on this port.
pub const GRADLE_DEBUG_PORT: u16 = 5005;

/// An "Application" configuration resolved for one run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppLaunch {
    /// The `java` executable of the configuration's JDK.
    pub java: String,
    /// VM options as typed (`-Xmx512m -Dprofile=dev`).
    pub vm_options: String,
    /// Main class (binary name).
    pub main_class: String,
    /// The Java module that holds the main class (runs with `-m module/class`).
    pub module: Option<String>,
    pub classpath: Vec<String>,
    pub modulepath: Vec<String>,
    /// Program arguments as typed.
    pub arguments: String,
    pub windows: bool,
    /// Debug: a JDWP agent that listens on this port and waits for the debugger.
    pub debug_port: Option<u16>,
}

/// The JDWP agent option that listens on `port` and suspends until a debugger attaches.
pub fn jdwp_listen(port: u16, suspend: bool) -> String {
    format!(
        "-agentlib:jdwp=transport=dt_socket,server=y,suspend={},address=127.0.0.1:{port}",
        if suspend { "y" } else { "n" }
    )
}

/// The VM option a remote JVM needs so "Remote JVM Debug" can attach (IntelliJ shows it in
/// the configuration as "Command line arguments for remote JVM").
pub fn remote_jvm_arguments(port: u16, java9_or_later: bool) -> String {
    let address = if java9_or_later {
        format!("*:{port}")
    } else {
        port.to_string()
    };
    format!("-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address={address}")
}

/// `(program, args)` of an application run.
pub fn application_command(l: &AppLaunch) -> Result<(String, Vec<String>), String> {
    let main = l.main_class.trim();
    if main.is_empty() {
        return Err("Main class is not specified".into());
    }
    let mut args = Vec::new();
    if let Some(port) = l.debug_port {
        args.push(jdwp_listen(port, true));
    }
    args.extend(split_args(&l.vm_options));
    let sep = if l.windows { ";" } else { ":" };
    if !l.modulepath.is_empty() {
        args.push("-p".into());
        args.push(l.modulepath.join(sep));
    }
    if !l.classpath.is_empty() {
        args.push("-cp".into());
        args.push(l.classpath.join(sep));
    }
    match l.module.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(module) => {
            args.push("-m".into());
            args.push(format!("{module}/{main}"));
        }
        None => args.push(main.to_string()),
    }
    args.extend(split_args(&l.arguments));
    let java = if l.java.trim().is_empty() {
        "java".to_string()
    } else {
        l.java.trim().to_string()
    };
    Ok((java, args))
}

/// Default class path of a module built by its build tool, when the language server
/// cannot resolve one: the compiled classes and resources.
pub fn build_output_classpath(tool: &str, module_dir: &str) -> Vec<String> {
    let prefix = match module_dir.trim_end_matches('/') {
        "" => String::new(),
        d => format!("{d}/"),
    };
    match tool {
        "maven" => vec![format!("{prefix}target/classes")],
        _ => vec![
            format!("{prefix}build/classes/java/main"),
            format!("{prefix}build/resources/main"),
        ],
    }
}

/// The arguments a debug run adds for the build tool: Gradle's `--debug-jvm` (port 5005)
/// after the tasks, Surefire's `-Dmaven.surefire.debug` with the port.
pub fn build_tool_debug_args(tool: &str, port: u16) -> Vec<String> {
    match tool {
        "maven" => vec![format!(
            "-Dmaven.surefire.debug={}",
            jdwp_listen(port, true)
        )],
        _ => vec!["--debug-jvm".into()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_command_lines() {
        let mut l = AppLaunch {
            java: "/jdk/bin/java".into(),
            vm_options: "-Xmx512m -Dname='a b'".into(),
            main_class: "com.acme.App".into(),
            classpath: vec!["/p/bin/main".into(), "/m2/lib.jar".into()],
            arguments: "--port 8080".into(),
            ..Default::default()
        };
        let (java, args) = application_command(&l).unwrap();
        assert_eq!(java, "/jdk/bin/java");
        assert_eq!(
            args,
            [
                "-Xmx512m",
                "-Dname=a b",
                "-cp",
                "/p/bin/main:/m2/lib.jar",
                "com.acme.App",
                "--port",
                "8080"
            ]
        );
        l.windows = true;
        l.module = Some("com.acme.app".into());
        l.modulepath = vec!["C:\\a".into(), "C:\\b".into()];
        l.debug_port = Some(41234);
        let (_, args) = application_command(&l).unwrap();
        assert_eq!(
            args[0],
            "-agentlib:jdwp=transport=dt_socket,server=y,suspend=y,address=127.0.0.1:41234"
        );
        assert!(args.windows(2).any(|w| w == ["-p", "C:\\a;C:\\b"]));
        assert!(args
            .windows(2)
            .any(|w| w == ["-m", "com.acme.app/com.acme.App"]));
        l.main_class = " ".into();
        assert!(application_command(&l).is_err());
    }

    #[test]
    fn debug_arguments() {
        assert_eq!(
            remote_jvm_arguments(5005, true),
            "-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005"
        );
        assert!(remote_jvm_arguments(5005, false).ends_with("address=5005"));
        assert_eq!(build_tool_debug_args("gradle", 5005), ["--debug-jvm"]);
        assert!(build_tool_debug_args("maven", 6000)[0].ends_with("127.0.0.1:6000"));
        assert_eq!(
            build_output_classpath("maven", "app"),
            ["app/target/classes"]
        );
        assert_eq!(
            build_output_classpath("gradle", "")[0],
            "build/classes/java/main"
        );
    }
}
