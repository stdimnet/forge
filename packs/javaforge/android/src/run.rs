//! The "Android App" run configuration, like Android Studio's: build the APK with Gradle
//! (`:app:assembleDebug`), install it on the chosen device (`adb -s <serial> install -r`)
//! and start the launcher activity (`adb shell am start -n <package>/<activity>`). The three
//! steps run as one command line through the platform's shell, so the Run tool window shows
//! one process with all output, and a failed step stops the rest.

use serde::{Deserialize, Serialize};

use crate::manifest::capitalize;

/// What an Android App run needs; the backend fills it from the configuration, the build,
/// the SDK, the devices and the manifest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppRun {
    /// Gradle (wrapper or executable).
    pub gradle: String,
    /// Extra Gradle arguments (`--offline`).
    pub gradle_args: Vec<String>,
    /// Gradle path of the module (`:app`).
    pub module_path: String,
    /// Module folder relative to the project (`app`).
    pub module_dir: String,
    /// Build variant (`debug`, `freeDebug`).
    pub variant: String,
    /// The APK to install (project-relative); empty: the variant's default output.
    pub apk: String,
    pub adb: String,
    pub serial: String,
    /// The package the app is installed as (`applicationId`).
    pub package: String,
    /// The activity to start (fully qualified); empty: the app's default launcher (`monkey`).
    pub activity: String,
    /// Skip the Gradle build (Before launch has no "Build").
    pub skip_build: bool,
}

/// `:app` + `debug` → `:app:assembleDebug`.
pub fn assemble_task(module_path: &str, variant: &str) -> String {
    let v = if variant.trim().is_empty() {
        "debug"
    } else {
        variant.trim()
    };
    let path = module_path.trim().trim_end_matches(':');
    format!("{path}:assemble{}", capitalize(v))
}

/// The default APK of a variant: `app/build/outputs/apk/debug/app-debug.apk`,
/// `app/build/outputs/apk/free/debug/app-free-debug.apk`.
pub fn default_apk(module_dir: &str, variant: &str) -> String {
    let v = if variant.trim().is_empty() {
        "debug"
    } else {
        variant.trim()
    };
    // `freeDebug` → flavor `free`, build type `debug`.
    let split = v
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_ascii_uppercase())
        .map(|(i, _)| i);
    let module = module_dir.trim_matches('/');
    let name = module
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or("app");
    let prefix = if module.is_empty() {
        String::new()
    } else {
        format!("{module}/")
    };
    match split {
        Some(i) => {
            let (flavor, build_type) = (&v[..i], v[i..].to_ascii_lowercase());
            format!(
                "{prefix}build/outputs/apk/{flavor}/{build_type}/{name}-{flavor}-{build_type}.apk"
            )
        }
        None => format!("{prefix}build/outputs/apk/{v}/{name}-{v}.apk"),
    }
}

/// The steps of a run: (program, arguments) each.
pub fn steps(run: &AppRun) -> Result<Vec<(String, Vec<String>)>, String> {
    if run.serial.trim().is_empty() {
        return Err("No device is connected: start an emulator in the Device Manager or connect a device with USB debugging".into());
    }
    if run.package.trim().is_empty() {
        return Err("The application id is unknown: set applicationId or namespace in the module's build file".into());
    }
    let adb = if run.adb.trim().is_empty() {
        "adb"
    } else {
        run.adb.trim()
    };
    let mut out = Vec::new();
    if !run.skip_build {
        if run.gradle.trim().is_empty() {
            return Err("Android apps build with Gradle: no Gradle build was found".into());
        }
        let mut a = vec!["--console=plain".to_string()];
        a.extend(run.gradle_args.iter().cloned());
        a.push(assemble_task(&run.module_path, &run.variant));
        out.push((run.gradle.clone(), a));
    }
    let apk = if run.apk.trim().is_empty() {
        default_apk(&run.module_dir, &run.variant)
    } else {
        run.apk.trim().to_string()
    };
    let device = |rest: &[&str]| -> (String, Vec<String>) {
        let mut a = vec!["-s".to_string(), run.serial.clone()];
        a.extend(rest.iter().map(|s| s.to_string()));
        (adb.to_string(), a)
    };
    out.push(device(&["install", "-r", "-t", &apk]));
    let start = if run.activity.trim().is_empty() {
        device(&[
            "shell",
            "monkey",
            "-p",
            &run.package,
            "-c",
            "android.intent.category.LAUNCHER",
            "1",
        ])
    } else {
        let component = format!("{}/{}", run.package, run.activity.trim());
        device(&[
            "shell",
            "am",
            "start",
            "-n",
            &component,
            "-a",
            "android.intent.action.MAIN",
            "-c",
            "android.intent.category.LAUNCHER",
        ])
    };
    out.push(start);
    Ok(out)
}

/// Quote an argument for `sh`.
fn sh_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Quote an argument for `cmd.exe`.
fn cmd_quote(s: &str) -> String {
    if !s.is_empty()
        && !s
            .chars()
            .any(|c| c.is_whitespace() || "\"&|<>^()".contains(c))
    {
        s.to_string()
    } else {
        format!("\"{}\"", s.replace('"', "\"\""))
    }
}

/// The steps as one shell command line: each step is echoed (`> adb install …`) and the
/// next runs only when it succeeded.
pub fn chain(steps: &[(String, Vec<String>)], windows: bool) -> (String, Vec<String>) {
    let quote: fn(&str) -> String = if windows { cmd_quote } else { sh_quote };
    let short = |p: &str| {
        p.rsplit(['/', '\\'])
            .next()
            .unwrap_or(p)
            .trim_end_matches(".exe")
            .trim_end_matches(".bat")
            .to_string()
    };
    let parts: Vec<String> = steps
        .iter()
        .map(|(program, args)| {
            let shown = std::iter::once(short(program))
                .chain(args.iter().cloned())
                .collect::<Vec<_>>()
                .join(" ");
            let line = std::iter::once(quote(program))
                .chain(args.iter().map(|a| quote(a)))
                .collect::<Vec<_>>()
                .join(" ");
            if windows {
                format!("echo ^> {} && {line}", shown.replace('&', "^&"))
            } else {
                format!("echo {} && {line}", sh_quote(&format!("> {shown}")))
            }
        })
        .collect();
    let script = parts.join(" && ");
    if windows {
        (
            "cmd.exe".into(),
            vec!["/d".into(), "/s".into(), "/c".into(), script],
        )
    } else {
        ("/bin/sh".into(), vec!["-c".into(), script])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run() -> AppRun {
        AppRun {
            gradle: "/p/gradlew".into(),
            module_path: ":app".into(),
            module_dir: "app".into(),
            variant: "debug".into(),
            adb: "/sdk/platform-tools/adb".into(),
            serial: "emulator-5554".into(),
            package: "com.acme.notes".into(),
            activity: "com.acme.notes.MainActivity".into(),
            ..Default::default()
        }
    }

    #[test]
    fn tasks_and_apks() {
        assert_eq!(assemble_task(":app", "debug"), ":app:assembleDebug");
        assert_eq!(assemble_task(":", ""), ":assembleDebug");
        assert_eq!(assemble_task(":app", "freeDebug"), ":app:assembleFreeDebug");
        assert_eq!(
            default_apk("app", "debug"),
            "app/build/outputs/apk/debug/app-debug.apk"
        );
        assert_eq!(
            default_apk("app", "freeDebug"),
            "app/build/outputs/apk/free/debug/app-free-debug.apk"
        );
        assert_eq!(
            default_apk("", "release"),
            "build/outputs/apk/release/app-release.apk"
        );
    }

    #[test]
    fn steps_and_shell_chain() {
        let s = steps(&run()).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(
            s[0],
            (
                "/p/gradlew".to_string(),
                vec!["--console=plain".to_string(), ":app:assembleDebug".into()]
            )
        );
        assert_eq!(
            s[1].1,
            [
                "-s",
                "emulator-5554",
                "install",
                "-r",
                "-t",
                "app/build/outputs/apk/debug/app-debug.apk"
            ]
        );
        assert_eq!(
            s[2].1[..6],
            ["-s", "emulator-5554", "shell", "am", "start", "-n"]
        );
        assert_eq!(s[2].1[6], "com.acme.notes/com.acme.notes.MainActivity");
        let (p, a) = chain(&s, false);
        assert_eq!(p, "/bin/sh");
        assert_eq!(a[0], "-c");
        assert!(a[1].starts_with("echo '> gradlew --console=plain :app:assembleDebug' && /p/gradlew --console=plain :app:assembleDebug && echo '> adb -s emulator-5554 install"));
        assert!(a[1].ends_with("android.intent.category.LAUNCHER"));
        let (p, a) = chain(&s, true);
        assert_eq!(p, "cmd.exe");
        assert!(
            a[3].starts_with("echo ^> gradlew --console=plain :app:assembleDebug && /p/gradlew")
        );

        let mut r = run();
        r.activity.clear();
        r.skip_build = true;
        let s = steps(&r).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].1[3], "monkey");
        r.serial.clear();
        assert!(steps(&r).unwrap_err().contains("No device"));
        assert_eq!(sh_quote("a b'c"), "'a b'\\''c'");
        assert_eq!(
            cmd_quote("C:\\Program Files\\x"),
            "\"C:\\Program Files\\x\""
        );
    }
}
