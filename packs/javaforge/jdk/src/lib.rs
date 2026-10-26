//! JavaForge's JDKs, like IntelliJ IDEA's SDK table (Project Structure | SDKs):
//!
//! - [`detect`] finds the JDKs on the machine (`JAVA_HOME`, `PATH`, `/usr/lib/jvm`,
//!   `/Library/Java/JavaVirtualMachines`, Program Files and the Windows registry, SDKMAN!,
//!   asdf, jabba, Gradle toolchains, IntelliJ's `~/.jdks`, Forge's own downloads) and reads
//!   each one's `release` file ([`release`]) for its version and vendor.
//! - [`adoptium`] downloads Eclipse Temurin builds from the Adoptium API, checked against
//!   the published SHA-256, into `<data dir>/forge/jdks`.

pub mod adoptium;
pub mod detect;
pub mod release;

use std::path::PathBuf;

pub use detect::{detect, inspect, pick, DetectEnv, Jdk};

/// `<data dir>/forge/jdks`: JDKs Forge downloaded.
pub fn default_jdks_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("jdks"))
}

/// The `java` executable of a JDK home.
pub fn java_executable(home: &str, windows: bool) -> String {
    let sep = if windows { '\\' } else { '/' };
    let home = home.trim_end_matches(['/', '\\']);
    format!(
        "{home}{sep}bin{sep}{}",
        if windows { "java.exe" } else { "java" }
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn java_executables() {
        assert_eq!(super::java_executable("/jdk/", false), "/jdk/bin/java");
        assert_eq!(
            super::java_executable("C:\\jdk", true),
            "C:\\jdk\\bin\\java.exe"
        );
    }
}
