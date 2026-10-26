//! The installed packages of an environment, read by a small script that uses
//! `importlib.metadata` (it works in environments without pip, such as uv's).

use pyforge_lang_python::InstalledPackage;
use serde::Deserialize;

/// `python -c <script>` prints `{"python": "3.12.4", "packages": [{name, version,
/// modules}]}`; modules come from `top_level.txt` or the distribution's files.
pub const LIST_SCRIPT: &str = r#"import json, sys
try:
    from importlib import metadata
except ImportError:
    import importlib_metadata as metadata
seen = {}
for d in metadata.distributions():
    name = d.metadata["Name"]
    if not name or name.lower() in seen:
        continue
    mods = set()
    top = d.read_text("top_level.txt")
    if top:
        mods.update(m.strip() for m in top.split() if m.strip())
    else:
        for f in (d.files or [])[:2000]:
            parts = str(f).replace("\\", "/").split("/")
            head = parts[0]
            if head.startswith("..") or head.endswith((".dist-info", ".egg-info", ".pth", ".data")) or head in ("__pycache__", "bin", "Scripts"):
                continue
            if len(parts) == 1:
                if head.endswith(".py"):
                    mods.add(head[:-3])
                elif head.endswith((".so", ".pyd")):
                    mods.add(head.split(".")[0])
            elif "." not in head:
                mods.add(head)
    seen[name.lower()] = {"name": name, "version": d.version, "modules": sorted(mods)}
print(json.dumps({"python": sys.version.split()[0], "packages": sorted(seen.values(), key=lambda p: p["name"].lower())}))"#;

#[derive(Deserialize)]
struct Listing {
    #[serde(default)]
    packages: Vec<InstalledPackage>,
}

/// The script's output (its last JSON line).
pub fn parse(stdout: &str) -> Result<Vec<InstalledPackage>, String> {
    stdout
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str::<Listing>(l.trim()).ok())
        .map(|l| l.packages)
        .ok_or_else(|| "The interpreter did not list its packages".to_string())
}

/// `pip list --format=json` (a fallback for Python 3.7 and older).
pub fn parse_pip_list(stdout: &str) -> Result<Vec<InstalledPackage>, String> {
    serde_json::from_str::<Vec<InstalledPackage>>(stdout.trim())
        .map_err(|e| format!("unexpected pip output: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_listings() {
        let out = "warning\n{\"python\": \"3.12.4\", \"packages\": [{\"name\": \"PyYAML\", \"version\": \"6.0.2\", \"modules\": [\"_yaml\", \"yaml\"]}]}\n";
        let p = parse(out).unwrap();
        assert_eq!(p[0].name, "PyYAML");
        assert_eq!(p[0].modules, vec!["_yaml", "yaml"]);
        assert!(parse("Traceback").is_err());
        let pip = parse_pip_list("[{\"name\": \"rich\", \"version\": \"13.9.2\"}]").unwrap();
        assert_eq!(pip[0].version, "13.9.2");
        assert!(pip[0].modules.is_empty());
    }
}
