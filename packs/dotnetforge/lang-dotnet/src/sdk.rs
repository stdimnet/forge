//! .NET SDKs and runtimes: `dotnet --list-sdks` / `--list-runtimes`, `global.json` and the
//! SDK the dotnet host picks for a folder (its `rollForward` policies), and the install
//! commands Forge offers when no SDK is found.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::launch::strip_jsonc;

/// An installed SDK (`8.0.404 [/usr/share/dotnet/sdk]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sdk {
    pub version: String,
    /// The folder of the SDK (`/usr/share/dotnet/sdk/8.0.404`).
    pub path: String,
}

/// An installed shared runtime (`Microsoft.AspNetCore.App 8.0.11 [...]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    pub name: String,
    pub version: String,
    pub path: String,
}

fn bracketed(line: &str) -> Option<(&str, &str)> {
    let open = line.rfind('[')?;
    let close = line.rfind(']')?;
    (close > open).then(|| (line[..open].trim(), line[open + 1..close].trim()))
}

/// `dotnet --list-sdks`.
pub fn parse_list_sdks(text: &str) -> Vec<Sdk> {
    let mut out: Vec<Sdk> = text
        .lines()
        .filter_map(bracketed)
        .filter(|(v, _)| v.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(|(v, dir)| Sdk {
            path: format!("{}/{v}", dir.trim_end_matches(['/', '\\'])),
            version: v.to_string(),
        })
        .collect();
    out.sort_by(|a, b| compare_versions(&a.version, &b.version));
    out
}

/// `dotnet --list-runtimes`.
pub fn parse_list_runtimes(text: &str) -> Vec<Runtime> {
    text.lines()
        .filter_map(bracketed)
        .filter_map(|(head, dir)| {
            let (name, version) = head.rsplit_once(' ')?;
            Some(Runtime {
                name: name.trim().to_string(),
                version: version.trim().to_string(),
                path: format!("{}/{}", dir.trim_end_matches(['/', '\\']), version.trim()),
            })
        })
        .collect()
}

/// A numeric version with an optional prerelease label (`9.0.100-preview.7.24407.12`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkVersion {
    pub major: u64,
    pub minor: u64,
    /// The feature band (`4` of `8.0.404`).
    pub feature: u64,
    /// The patch inside the band (`04` of `8.0.404`).
    pub patch: u64,
    pub prerelease: String,
}

impl SdkVersion {
    pub fn parse(v: &str) -> Option<SdkVersion> {
        let (core, pre) = match v.trim().split_once('-') {
            Some((c, p)) => (c, p.to_string()),
            None => (v.trim(), String::new()),
        };
        let mut it = core.split('.');
        let major = it.next()?.parse().ok()?;
        let minor = it.next().unwrap_or("0").parse().ok()?;
        let third: u64 = it.next().unwrap_or("0").parse().ok()?;
        Some(SdkVersion {
            major,
            minor,
            feature: third / 100,
            patch: third % 100,
            prerelease: pre,
        })
    }

    fn key(&self) -> (u64, u64, u64, u64) {
        (self.major, self.minor, self.feature, self.patch)
    }
}

/// Compare SDK or runtime versions (a release sorts after its previews).
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    match (SdkVersion::parse(a), SdkVersion::parse(b)) {
        (Some(x), Some(y)) => x.key().cmp(&y.key()).then_with(|| {
            match (x.prerelease.is_empty(), y.prerelease.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ => crate::version::compare_labels(&x.prerelease, &y.prerelease),
            }
        }),
        _ => a.cmp(b),
    }
}

/// `global.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalJson {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `patch` (the default with a version), `feature`, `minor`, `major`, `latestPatch`,
    /// `latestFeature`, `latestMinor`, `latestMajor` (the default without one), `disable`.
    pub roll_forward: String,
    pub allow_prerelease: bool,
    /// `msbuild-sdks` (`Microsoft.Build.Traversal`: `4.1.0`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub msbuild_sdks: Vec<(String, String)>,
    /// The test runner (`Microsoft.Testing.Platform`) chosen in `test.runner`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_runner: Option<String>,
}

pub fn parse_global_json(text: &str) -> Result<GlobalJson, String> {
    let v: Value =
        serde_json::from_str(&strip_jsonc(text)).map_err(|e| format!("global.json: {e}"))?;
    let sdk = v.get("sdk");
    let version = sdk
        .and_then(|s| s.get("version"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let roll_forward = sdk
        .and_then(|s| s.get("rollForward"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            if version.is_some() {
                "patch".into()
            } else {
                "latestMajor".into()
            }
        });
    let allow_prerelease = sdk
        .and_then(|s| s.get("allowPrerelease"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let msbuild_sdks = v
        .get("msbuild-sdks")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect()
        })
        .unwrap_or_default();
    let test_runner = v
        .get("test")
        .and_then(|t| t.get("runner"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(GlobalJson {
        version,
        roll_forward,
        allow_prerelease,
        msbuild_sdks,
        test_runner,
    })
}

/// The SDK the dotnet host resolves (`None`: global.json asks for one that is missing).
pub fn select_sdk<'a>(sdks: &'a [Sdk], global: Option<&GlobalJson>) -> Option<&'a Sdk> {
    let parsed: Vec<(&Sdk, SdkVersion)> = sdks
        .iter()
        .filter_map(|s| SdkVersion::parse(&s.version).map(|v| (s, v)))
        .collect();
    let newest = |list: Vec<&(&'a Sdk, SdkVersion)>| -> Option<&'a Sdk> {
        list.into_iter()
            .max_by(|a, b| compare_versions(&a.0.version, &b.0.version))
            .map(|(s, _)| *s)
    };
    let Some(g) = global else {
        // Without global.json: the newest release, previews only when nothing else exists.
        let releases: Vec<_> = parsed.iter().filter(|(_, v)| v.prerelease.is_empty()).collect();
        return if releases.is_empty() {
            newest(parsed.iter().collect())
        } else {
            newest(releases)
        };
    };
    let candidates: Vec<&(&Sdk, SdkVersion)> = parsed
        .iter()
        .filter(|(_, v)| g.allow_prerelease || v.prerelease.is_empty())
        .collect();
    let Some(want) = g.version.as_deref().and_then(SdkVersion::parse) else {
        return newest(candidates);
    };
    let at_least = |v: &SdkVersion| v.key() >= want.key();
    let pick = |f: &dyn Fn(&SdkVersion) -> bool| -> Option<&'a Sdk> {
        newest(candidates.iter().copied().filter(|(_, v)| at_least(v) && f(v)).collect())
    };
    let lowest = |f: &dyn Fn(&SdkVersion) -> bool| -> Option<&'a Sdk> {
        candidates
            .iter()
            .copied()
            .filter(|(_, v)| at_least(v) && f(v))
            .min_by(|a, b| compare_versions(&a.0.version, &b.0.version))
            .map(|(s, _)| *s)
    };
    let same_band = |v: &SdkVersion| v.major == want.major && v.minor == want.minor && v.feature == want.feature;
    let same_minor = |v: &SdkVersion| v.major == want.major && v.minor == want.minor;
    let same_major = |v: &SdkVersion| v.major == want.major;
    match g.roll_forward.to_ascii_lowercase().as_str() {
        "disable" => candidates
            .iter()
            .find(|(_, v)| v.key() == want.key())
            .map(|(s, _)| *s),
        "latestpatch" => pick(&same_band),
        "feature" => pick(&same_band).or_else(|| {
            // The lowest higher band of the same minor, then its newest patch.
            let band = lowest(&same_minor)?;
            let b = SdkVersion::parse(&band.version)?;
            pick(&|v: &SdkVersion| v.major == b.major && v.minor == b.minor && v.feature == b.feature)
        }),
        "latestfeature" => pick(&same_minor),
        "minor" => pick(&same_band).or_else(|| pick(&same_minor)).or_else(|| lowest(&same_major)),
        "latestminor" => pick(&same_major),
        "major" => pick(&same_band)
            .or_else(|| pick(&same_minor))
            .or_else(|| pick(&same_major))
            .or_else(|| lowest(&|_| true)),
        "latestmajor" => pick(&|_| true),
        // `patch` (default).
        _ => pick(&same_band),
    }
}

/// A way to install the .NET SDK on this platform, shown by "Install .NET SDK…".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOption {
    pub label: String,
    /// The command to run in the terminal (empty: open `url`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
}

pub const DOWNLOAD_PAGE: &str = "https://dotnet.microsoft.com/download";

/// Install options for an OS (`linux`, `macos`, `windows`) and the wanted channel (`8.0`).
pub fn install_options(os: &str, channel: &str) -> Vec<InstallOption> {
    let ch = if channel.is_empty() { "8.0" } else { channel };
    let mut out = Vec::new();
    let cmd = |label: &str, command: String| InstallOption {
        label: label.into(),
        command,
        url: String::new(),
    };
    match os {
        "windows" => {
            out.push(cmd("winget", format!("winget install Microsoft.DotNet.SDK.{}", ch.split('.').next().unwrap_or("8"))));
        }
        "macos" => {
            out.push(cmd("Homebrew", "brew install --cask dotnet-sdk".into()));
        }
        _ => {
            out.push(cmd("Ubuntu / Debian (apt)", format!("sudo apt-get install -y dotnet-sdk-{ch}")));
            out.push(cmd("Fedora / RHEL (dnf)", format!("sudo dnf install dotnet-sdk-{ch}")));
            out.push(cmd("Arch Linux (pacman)", "sudo pacman -S dotnet-sdk".into()));
        }
    }
    out.push(InstallOption {
        label: "Download from dotnet.microsoft.com".into(),
        command: String::new(),
        url: DOWNLOAD_PAGE.into(),
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    fn sdks(versions: &[&str]) -> Vec<Sdk> {
        versions
            .iter()
            .map(|v| Sdk {
                version: v.to_string(),
                path: format!("/d/sdk/{v}"),
            })
            .collect()
    }

    #[test]
    fn lists() {
        let s = parse_list_sdks(&fixture("dotnet-list-sdks.txt"));
        let versions: Vec<&str> = s.iter().map(|s| s.version.as_str()).collect();
        assert_eq!(versions, vec!["6.0.428", "8.0.110", "8.0.404", "9.0.100-rc.2.24474.11", "9.0.101"]);
        assert_eq!(s[2].path, "/usr/share/dotnet/sdk/8.0.404");
        let r = parse_list_runtimes(&fixture("dotnet-list-runtimes.txt"));
        assert_eq!(r.len(), 4);
        assert_eq!(r[0].name, "Microsoft.AspNetCore.App");
        assert_eq!(r[0].version, "8.0.11");
        assert_eq!(r[3].path, "/usr/share/dotnet/shared/Microsoft.NETCore.App/9.0.0");
        assert!(parse_list_sdks("command not found").is_empty());
    }

    #[test]
    fn versions() {
        assert_eq!(compare_versions("8.0.404", "8.0.110"), Ordering::Greater);
        assert_eq!(compare_versions("9.0.100-rc.2.24474.11", "9.0.100"), Ordering::Less);
        assert_eq!(compare_versions("9.0.100-preview.7", "9.0.100-rc.1"), Ordering::Less);
        let v = SdkVersion::parse("8.0.404").unwrap();
        assert_eq!((v.feature, v.patch), (4, 4));
    }

    #[test]
    fn global_json_and_roll_forward() {
        let g = parse_global_json(&fixture("bookstore/global.json")).unwrap();
        assert_eq!(g.version.as_deref(), Some("8.0.400"));
        assert_eq!(g.roll_forward, "latestFeature");
        assert!(!g.allow_prerelease);
        assert_eq!(g.msbuild_sdks, vec![("Microsoft.Build.Traversal".to_string(), "4.1.0".to_string())]);
        let all = sdks(&["6.0.428", "8.0.110", "8.0.404", "8.0.405", "9.0.100-rc.2", "9.0.101"]);
        assert_eq!(select_sdk(&all, Some(&g)).unwrap().version, "8.0.405");
        let with = |rf: &str, v: &str| GlobalJson {
            version: Some(v.into()),
            roll_forward: rf.into(),
            allow_prerelease: false,
            ..GlobalJson::default()
        };
        let pick = |g: GlobalJson| select_sdk(&all, Some(&g)).map(|s| s.version.clone());
        assert_eq!(pick(with("patch", "8.0.400")).as_deref(), Some("8.0.405"));
        assert_eq!(pick(with("patch", "8.0.300")), None);
        assert_eq!(pick(with("feature", "8.0.300")).as_deref(), Some("8.0.405"));
        assert_eq!(pick(with("disable", "8.0.404")).as_deref(), Some("8.0.404"));
        assert_eq!(pick(with("disable", "8.0.403")), None);
        assert_eq!(pick(with("latestMajor", "6.0.100")).as_deref(), Some("9.0.101"));
        assert_eq!(pick(with("major", "7.0.100")).as_deref(), Some("8.0.110"));
        assert_eq!(pick(with("latestMinor", "8.0.100")).as_deref(), Some("8.0.405"));
        // Without global.json: the newest release.
        assert_eq!(select_sdk(&all, None).unwrap().version, "9.0.101");
        assert_eq!(select_sdk(&sdks(&["10.0.100-preview.1"]), None).unwrap().version, "10.0.100-preview.1");
        let defaults = parse_global_json("{}").unwrap();
        assert_eq!(defaults.roll_forward, "latestMajor");
        assert!(parse_global_json("{").is_err());
    }

    #[test]
    fn install_hints() {
        assert!(install_options("linux", "8.0")[0].command.contains("dotnet-sdk-8.0"));
        assert_eq!(install_options("windows", "9.0")[0].command, "winget install Microsoft.DotNet.SDK.9");
        assert_eq!(install_options("macos", "").last().unwrap().url, DOWNLOAD_PAGE);
    }
}
