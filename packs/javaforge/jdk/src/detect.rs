//! Finding the JDKs on a machine, like IntelliJ's "Detected SDKs": `JAVA_HOME`, `java` on
//! `PATH`, the system folders of Linux, macOS and Windows (plus the Windows registry), and
//! the folders of SDKMAN!, asdf, jabba, Gradle toolchains, IntelliJ (`~/.jdks`) and Forge's
//! own downloads.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::release::{major_of, parse_release, vendor_name};

/// A JDK on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Jdk {
    /// The JDK's home (contains `bin/java` and `release`).
    pub home: String,
    /// `21.0.4`; empty without a `release` file.
    pub version: String,
    /// Feature release (`21`); 0 when unknown.
    pub major: u32,
    /// `Eclipse Temurin`, `Azul Zulu`, …
    pub vendor: String,
    /// Where it was found: `JAVA_HOME`, `PATH`, `System`, `SDKMAN!`, `asdf`, `jabba`,
    /// `Gradle`, `IntelliJ IDEA`, `Forge`, `Windows Registry`, `Added`.
    pub source: String,
    /// IntelliJ-style name: `temurin-21` (vendor id + major).
    pub name: String,
}

/// Where to look; [`DetectEnv::system`] fills it from this machine.
#[derive(Debug, Clone, Default)]
pub struct DetectEnv {
    pub java_home: Option<PathBuf>,
    pub path: Option<OsString>,
    pub home: Option<PathBuf>,
    /// `linux`, `macos`, `windows`.
    pub os: String,
    pub sdkman_dir: Option<PathBuf>,
    pub asdf_dir: Option<PathBuf>,
    pub jabba_home: Option<PathBuf>,
    pub gradle_user_home: Option<PathBuf>,
    /// Forge's downloads (`<data dir>/forge/jdks`).
    pub forge_dir: Option<PathBuf>,
    /// System folders whose children are JDKs (`/usr/lib/jvm`, `C:\Program Files\Java`).
    pub system_roots: Vec<PathBuf>,
    /// JDK homes from the Windows registry.
    pub registry: Vec<PathBuf>,
}

/// The default system folders of an OS.
pub fn system_roots(os: &str, home: Option<&Path>) -> Vec<PathBuf> {
    match os {
        "macos" => {
            let mut roots = vec![PathBuf::from("/Library/Java/JavaVirtualMachines")];
            if let Some(h) = home {
                roots.push(h.join("Library/Java/JavaVirtualMachines"));
            }
            roots.push(PathBuf::from("/opt/homebrew/opt"));
            roots.push(PathBuf::from("/usr/local/opt"));
            roots
        }
        "windows" => {
            let mut roots = Vec::new();
            for var in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
                if let Some(pf) = std::env::var_os(var) {
                    let pf = PathBuf::from(pf);
                    for vendor in [
                        "Java",
                        "Eclipse Adoptium",
                        "Eclipse Foundation",
                        "AdoptOpenJDK",
                        "Zulu",
                        "Microsoft",
                        "Amazon Corretto",
                        "BellSoft",
                        "Semeru",
                        "OpenJDK",
                    ] {
                        let r = pf.join(vendor);
                        if !roots.contains(&r) {
                            roots.push(r);
                        }
                    }
                }
            }
            roots
        }
        _ => [
            "/usr/lib/jvm",
            "/usr/lib64/jvm",
            "/usr/java",
            "/opt/java",
            "/opt/jdk",
            "/usr/local/java",
        ]
        .iter()
        .map(PathBuf::from)
        .collect(),
    }
}

impl DetectEnv {
    /// This machine's environment and folders.
    pub fn system() -> Self {
        let os = std::env::consts::OS.to_string();
        let home = dirs::home_dir();
        // In Flatpak: the host's environment, PATH and system folders (as the sandbox sees
        // them).
        let var = |k: &str| {
            forge_hostspawn::env_var(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let registry = if os == "windows" {
            windows_registry()
        } else {
            Vec::new()
        };
        DetectEnv {
            java_home: var("JAVA_HOME"),
            path: forge_hostspawn::sandbox_path_var(),
            system_roots: system_roots(&os, home.as_deref())
                .iter()
                .map(|r| forge_hostspawn::sandbox_view(r))
                .collect(),
            sdkman_dir: var("SDKMAN_DIR"),
            asdf_dir: var("ASDF_DATA_DIR"),
            jabba_home: var("JABBA_HOME"),
            gradle_user_home: var("GRADLE_USER_HOME"),
            forge_dir: crate::default_jdks_dir(),
            home,
            os,
            registry,
        }
    }

    fn exe(&self) -> &'static str {
        if self.os == "windows" {
            "java.exe"
        } else {
            "java"
        }
    }
}

/// JDK homes listed by `reg query <key> /s` output (`JavaHome` and Adoptium's `Path`
/// values).
pub fn parse_registry(output: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        let mut parts = line.splitn(3, "    ");
        let (Some(name), Some(kind), Some(value)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if kind.trim() == "REG_SZ" && matches!(name.trim(), "JavaHome" | "Path") {
            let p = PathBuf::from(value.trim().trim_end_matches('\\'));
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// JDK homes from the Windows registry (Oracle, Adoptium, Azul, Microsoft keys).
fn windows_registry() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for key in [
        r"HKLM\SOFTWARE\JavaSoft\JDK",
        r"HKLM\SOFTWARE\JavaSoft\Java Development Kit",
        r"HKLM\SOFTWARE\Eclipse Adoptium\JDK",
        r"HKLM\SOFTWARE\Eclipse Foundation\JDK",
        r"HKLM\SOFTWARE\Azul Systems\Zulu",
        r"HKLM\SOFTWARE\Microsoft\JDK",
    ] {
        let mut reg = std::process::Command::new("reg");
        reg.args(["query", key, "/s"]);
        let output = forge_hostspawn::local_command(reg).output();
        if let Ok(o) = output {
            for p in parse_registry(&String::from_utf8_lossy(&o.stdout)) {
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// The JDK home inside a folder: the folder itself, macOS bundles' `Contents/Home`, or the
/// single JDK folder inside it (Gradle toolchains, Homebrew's `libexec/openjdk.jdk`).
fn home_in(dir: &Path, exe: &str) -> Option<PathBuf> {
    let is_home = |d: &Path| d.join("bin").join(exe).is_file();
    for candidate in [
        dir.to_path_buf(),
        dir.join("Contents/Home"),
        dir.join("libexec/openjdk.jdk/Contents/Home"),
    ] {
        if is_home(&candidate) {
            return Some(candidate);
        }
    }
    let subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    if let [only] = subdirs.as_slice() {
        for candidate in [only.clone(), only.join("Contents/Home")] {
            if is_home(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

/// The JDK at `home` (a JDK home, a macOS bundle or a folder with one JDK in it). `None`
/// when there is no `bin/java`, or no `javac` (a JRE).
pub fn inspect(home: &Path, source: &str) -> Option<Jdk> {
    let windows = home.join("bin/java.exe").is_file();
    let exe = if windows { "java.exe" } else { "java" };
    let home = home_in(home, exe)?;
    let javac = if windows { "javac.exe" } else { "javac" };
    if !home.join("bin").join(javac).is_file() {
        return None;
    }
    let info = parse_release(&std::fs::read_to_string(home.join("release")).unwrap_or_default());
    let major = major_of(&info.java_version);
    let vendor = vendor_name(&info);
    let vendor_id = vendor
        .rsplit(' ')
        .next()
        .unwrap_or("openjdk")
        .to_ascii_lowercase();
    Some(Jdk {
        home: home.to_string_lossy().into_owned(),
        version: info.java_version,
        major,
        name: if major > 0 {
            format!("{vendor_id}-{major}")
        } else {
            vendor_id
        },
        vendor,
        source: source.to_string(),
    })
}

fn children(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .filter(|p| {
                    // SDKMAN!'s `current` and Debian's `default-java` point at other entries.
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    name != "current" && !name.starts_with('.')
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Every JDK found, newest feature release first; the same JDK reached in several ways
/// (symlinks, `JAVA_HOME` and `PATH`) is listed once, with the first source.
pub fn detect(env: &DetectEnv) -> Vec<Jdk> {
    let mut candidates: Vec<(PathBuf, &str)> = Vec::new();
    if let Some(h) = &env.java_home {
        candidates.push((h.clone(), "JAVA_HOME"));
    }
    if let Some(path) = &env.path {
        for dir in std::env::split_paths(path) {
            let java = dir.join(env.exe());
            if java.is_file() {
                // /usr/bin/java → /etc/alternatives/java → /usr/lib/jvm/…/bin/java
                let real = std::fs::canonicalize(&java).unwrap_or(java);
                if let Some(home) = real.parent().and_then(Path::parent) {
                    candidates.push((home.to_path_buf(), "PATH"));
                }
            }
        }
    }
    for root in &env.system_roots {
        for c in children(root) {
            candidates.push((c, "System"));
        }
    }
    for p in &env.registry {
        candidates.push((p.clone(), "Windows Registry"));
    }
    let home = env.home.clone();
    let under_home = |rel: &str| home.as_ref().map(|h| h.join(rel));
    let tool_dirs: [(Option<PathBuf>, &str); 6] = [
        (
            env.sdkman_dir
                .clone()
                .or_else(|| under_home(".sdkman"))
                .map(|d| d.join("candidates/java")),
            "SDKMAN!",
        ),
        (
            env.asdf_dir
                .clone()
                .or_else(|| under_home(".asdf"))
                .map(|d| d.join("installs/java")),
            "asdf",
        ),
        (
            env.jabba_home
                .clone()
                .or_else(|| under_home(".jabba"))
                .map(|d| d.join("jdk")),
            "jabba",
        ),
        (
            env.gradle_user_home
                .clone()
                .or_else(|| under_home(".gradle"))
                .map(|d| d.join("jdks")),
            "Gradle",
        ),
        (under_home(".jdks"), "IntelliJ IDEA"),
        (env.forge_dir.clone(), "Forge"),
    ];
    for (dir, source) in tool_dirs {
        if let Some(dir) = dir {
            for c in children(&dir) {
                candidates.push((c, source));
            }
        }
    }
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out = Vec::new();
    for (dir, source) in candidates {
        let Some(jdk) = inspect(&dir, source) else {
            continue;
        };
        let key = std::fs::canonicalize(&jdk.home).unwrap_or_else(|_| PathBuf::from(&jdk.home));
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(jdk);
    }
    out.sort_by(|a, b| b.major.cmp(&a.major).then_with(|| a.home.cmp(&b.home)));
    out
}

/// The JDK that runs JDT LS and is the default project JDK: `preferred` (a home) when it
/// is a JDK of at least `min_major`, else the newest one that is.
pub fn pick<'a>(jdks: &'a [Jdk], preferred: Option<&str>, min_major: u32) -> Option<&'a Jdk> {
    let ok = |j: &&Jdk| j.major >= min_major;
    preferred
        .and_then(|p| jdks.iter().filter(ok).find(|j| j.home == p || j.name == p))
        .or_else(|| jdks.iter().find(ok))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A fake JDK folder: `bin/java`, `bin/javac` and a `release` file.
    pub fn fake_jdk(home: &Path, version: &str, implementor: &str) {
        std::fs::create_dir_all(home.join("bin")).unwrap();
        for exe in ["java", "javac"] {
            std::fs::write(home.join("bin").join(exe), "#!/bin/sh\n").unwrap();
        }
        std::fs::write(
            home.join("release"),
            format!("IMPLEMENTOR=\"{implementor}\"\nJAVA_VERSION=\"{version}\"\n"),
        )
        .unwrap();
    }

    #[test]
    fn detects_jdks_in_every_known_place() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        fake_jdk(
            &t.join("usr/lib/jvm/java-17-openjdk-amd64"),
            "17.0.12",
            "Ubuntu",
        );
        fake_jdk(
            &t.join("home/.sdkman/candidates/java/21.0.4-tem"),
            "21.0.4",
            "Eclipse Adoptium",
        );
        fake_jdk(
            &t.join("home/.asdf/installs/java/zulu-11.72.19"),
            "11.0.23",
            "Azul Systems, Inc.",
        );
        fake_jdk(
            &t.join("home/.jabba/jdk/openjdk@1.8/Contents/Home"),
            "1.8.0_412",
            "Oracle Corporation",
        );
        fake_jdk(
            &t.join("home/.gradle/jdks/eclipse_adoptium-22-amd64-linux/jdk-22.0.2+9"),
            "22.0.2",
            "Eclipse Adoptium",
        );
        fake_jdk(
            &t.join("home/.jdks/corretto-23.0.1"),
            "23.0.1",
            "Amazon.com Inc.",
        );
        fake_jdk(
            &t.join("data/forge/jdks/jdk-25+36"),
            "25",
            "Eclipse Adoptium",
        );
        fake_jdk(&t.join("opt/custom-jdk"), "24", "Oracle Corporation");
        // A JRE (no javac) is not a JDK.
        let jre = t.join("usr/lib/jvm/jre-8");
        std::fs::create_dir_all(jre.join("bin")).unwrap();
        std::fs::write(jre.join("bin/java"), "").unwrap();
        // `java` on PATH inside the JAVA_HOME JDK: listed once.
        let env = DetectEnv {
            java_home: Some(t.join("opt/custom-jdk")),
            path: Some(std::env::join_paths([t.join("opt/custom-jdk/bin")]).unwrap()),
            home: Some(t.join("home")),
            os: "linux".into(),
            forge_dir: Some(t.join("data/forge/jdks")),
            system_roots: vec![t.join("usr/lib/jvm")],
            ..Default::default()
        };
        let jdks = detect(&env);
        let summary: Vec<(u32, &str, &str)> = jdks
            .iter()
            .map(|j| (j.major, j.source.as_str(), j.vendor.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                (25, "Forge", "Eclipse Temurin"),
                (24, "JAVA_HOME", "Oracle OpenJDK"),
                (23, "IntelliJ IDEA", "Amazon Corretto"),
                (22, "Gradle", "Eclipse Temurin"),
                (21, "SDKMAN!", "Eclipse Temurin"),
                (17, "System", "OpenJDK"),
                (11, "asdf", "Azul Zulu"),
                (8, "jabba", "Oracle OpenJDK"),
            ]
        );
        assert_eq!(jdks[0].name, "temurin-25");
        assert!(jdks[7].home.ends_with("Contents/Home"));
        assert_eq!(pick(&jdks, None, 21).unwrap().major, 25);
        assert_eq!(pick(&jdks, Some(&jdks[4].home), 21).unwrap().major, 21);
        assert_eq!(
            pick(&jdks, Some(&jdks[5].home), 21).unwrap().major,
            25,
            "17 is too old"
        );
        assert!(pick(&jdks, None, 30).is_none());
        assert!(inspect(&t.join("missing"), "Added").is_none());
    }

    #[test]
    fn registry_output() {
        let out = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\JavaSoft\\JDK\\21\r\n    JavaHome    REG_SZ    C:\\Program Files\\Java\\jdk-21\\\r\n    MicroVersion    REG_SZ    0\r\n\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Eclipse Adoptium\\JDK\\17.0.12.7\\hotspot\\MSI\r\n    Path    REG_SZ    C:\\Program Files\\Eclipse Adoptium\\jdk-17.0.12.7-hotspot\\\r\n";
        assert_eq!(
            parse_registry(out),
            [
                PathBuf::from("C:\\Program Files\\Java\\jdk-21"),
                PathBuf::from("C:\\Program Files\\Eclipse Adoptium\\jdk-17.0.12.7-hotspot")
            ]
        );
        assert!(system_roots("linux", None).contains(&PathBuf::from("/usr/lib/jvm")));
        assert!(system_roots("macos", None)[0].ends_with("JavaVirtualMachines"));
    }
}
