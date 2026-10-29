//! Visual Studio installations from `vswhere -format json`, and the MSVC folders inside them
//! (CLion's "Visual Studio" toolchain): `cl.exe` of the newest MSVC tool set, `vcvarsall.bat`
//! for the build environment and the CMake and Ninja that Visual Studio bundles.

use serde::{Deserialize, Serialize};

use crate::version::version_key;

/// A Visual Studio (or Build Tools) installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VsInstallation {
    pub display_name: String,
    pub installation_path: String,
    pub installation_version: String,
    /// `2022`.
    #[serde(default)]
    pub product_line: String,
    pub prerelease: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCatalog {
    #[serde(default)]
    product_line_version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawInstance {
    #[serde(default)]
    display_name: String,
    installation_path: String,
    #[serde(default)]
    installation_version: String,
    #[serde(default)]
    is_prerelease: bool,
    #[serde(default)]
    catalog: Option<RawCatalog>,
}

/// The arguments for vswhere: every product with the x86/x64 C++ tools, as JSON.
pub const VSWHERE_ARGS: &[&str] = &[
    "-products",
    "*",
    "-requires",
    "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
    "-format",
    "json",
    "-utf8",
];

/// `%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe`.
pub fn vswhere_path(program_files_x86: &str) -> String {
    format!("{program_files_x86}\\Microsoft Visual Studio\\Installer\\vswhere.exe")
}

/// Parse vswhere's JSON, newest installation first.
pub fn parse(json: &str) -> Result<Vec<VsInstallation>, String> {
    let raw: Vec<RawInstance> =
        serde_json::from_str(json).map_err(|e| format!("bad vswhere output: {e}"))?;
    let mut out: Vec<VsInstallation> = raw
        .into_iter()
        .map(|r| VsInstallation {
            display_name: r.display_name,
            installation_path: r.installation_path,
            installation_version: r.installation_version,
            product_line: r
                .catalog
                .map(|c| c.product_line_version)
                .unwrap_or_default(),
            prerelease: r.is_prerelease,
        })
        .collect();
    out.sort_by_key(|i| std::cmp::Reverse(version_key(&i.installation_version)));
    Ok(out)
}

/// The newest MSVC tool set folder name (`14.41.34120`) among `VC\Tools\MSVC\*`.
pub fn newest_toolset<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<String> {
    names
        .into_iter()
        .filter(|n| n.starts_with(|c: char| c.is_ascii_digit()))
        .max_by_key(|n| version_key(n))
        .map(str::to_string)
}

/// Paths inside an installation (Windows separators).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsvcPaths {
    pub cl: String,
    pub vcvarsall: String,
    pub cmake: String,
    pub ninja: String,
}

/// `cl.exe` of `toolset` for the x64 host and `arch` (`x64`, `x86`, `arm64`), and the
/// bundled tools.
pub fn msvc_paths(installation: &str, toolset: &str, arch: &str) -> MsvcPaths {
    let root = installation.trim_end_matches(['\\', '/']);
    let cmake_dir = format!("{root}\\Common7\\IDE\\CommonExtensions\\Microsoft\\CMake");
    MsvcPaths {
        cl: format!("{root}\\VC\\Tools\\MSVC\\{toolset}\\bin\\Hostx64\\{arch}\\cl.exe"),
        vcvarsall: format!("{root}\\VC\\Auxiliary\\Build\\vcvarsall.bat"),
        cmake: format!("{cmake_dir}\\CMake\\bin\\cmake.exe"),
        ninja: format!("{cmake_dir}\\Ninja\\ninja.exe"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installations_newest_first() {
        let json = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vswhere.json"),
        )
        .unwrap();
        let list = parse(&json).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].display_name, "Visual Studio Community 2022");
        assert_eq!(list[0].product_line, "2022");
        assert_eq!(list[1].installation_version, "16.11.34931.43");
        assert!(parse("not json").is_err());
    }

    #[test]
    fn msvc_folders() {
        assert_eq!(
            newest_toolset(["14.38.33130", "14.41.34120", "14.29.30133", "readme.txt"]).as_deref(),
            Some("14.41.34120")
        );
        let p = msvc_paths(
            "C:\\Program Files\\Microsoft Visual Studio\\2022\\Community",
            "14.41.34120",
            "x64",
        );
        assert_eq!(
            p.cl,
            "C:\\Program Files\\Microsoft Visual Studio\\2022\\Community\\VC\\Tools\\MSVC\\14.41.34120\\bin\\Hostx64\\x64\\cl.exe"
        );
        assert!(p.vcvarsall.ends_with("VC\\Auxiliary\\Build\\vcvarsall.bat"));
        assert!(p.ninja.ends_with("CMake\\Ninja\\ninja.exe"));
        assert_eq!(
            vswhere_path("C:\\Program Files (x86)"),
            "C:\\Program Files (x86)\\Microsoft Visual Studio\\Installer\\vswhere.exe"
        );
    }
}
