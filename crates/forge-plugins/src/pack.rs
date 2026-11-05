//! Packing a plugin folder into a `.forgeplugin` package, with the same file selection as the
//! SDK's `sdk/scripts/pack.mjs`: `forge-plugin.json`, the [`DOCUMENTS`] that exist, `main`,
//! everything in `dist/` and the folder of each icon theme file. The zip is reproducible:
//! a fixed entry order, fixed 1980-01-01 timestamps and fixed permissions, so the same
//! sources always give the same bytes and the same SHA-256.

use std::path::{Path, PathBuf};

use crate::manifest::{is_package_path, PluginManifest, MANIFEST_FILE};
use crate::package::MAX_SIZE;

/// Files at the plugin root that are packed when present, so attribution of bundled
/// third-party material travels with the package.
pub const DOCUMENTS: &[&str] = &[
    "README.md",
    "LICENSE",
    "CHANGELOG.md",
    "THIRD_PARTY_NOTICES.md",
    "NOTICE",
];

/// The folder whose files are always packed (the built UI bundle and its assets).
pub const DIST: &str = "dist";

/// The files of a plugin folder that go into its package.
#[derive(Debug, Clone)]
pub struct Selection {
    pub manifest: PluginManifest,
    /// `(package path, file on disk)` in package order.
    pub files: Vec<(String, PathBuf)>,
}

/// A packed plugin.
#[derive(Debug, Clone)]
pub struct Packed {
    pub manifest: PluginManifest,
    pub bytes: Vec<u8>,
    /// Package paths in package order.
    pub paths: Vec<String>,
}

impl Packed {
    /// `<id>-<version>.forgeplugin`, the SDK's default file name.
    pub fn file_name(&self) -> String {
        format!(
            "{}-{}.{}",
            self.manifest.id,
            self.manifest.version,
            crate::package::EXTENSION
        )
    }
}

/// Every file below `dir`, sorted by name per folder (like `pack.mjs`). Symbolic links are
/// refused, because a package cannot contain them.
fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if meta.file_type().is_symlink() {
            return Err(format!(
                "{}: symbolic links are not allowed in a package",
                package_path(root, &path)?
            ));
        }
        if meta.is_dir() {
            walk(root, &path, out)?;
        } else {
            out.push((package_path(root, &path)?, path));
        }
    }
    Ok(())
}

fn package_path(root: &Path, file: &Path) -> Result<String, String> {
    let rel = file
        .strip_prefix(root)
        .map_err(|_| format!("{} is outside the plugin folder", file.display()))?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    Ok(parts.join("/"))
}

/// Read the manifest of the plugin folder `dir`.
pub fn read_manifest(dir: &Path) -> Result<PluginManifest, String> {
    let path = dir.join(MANIFEST_FILE);
    let text = std::fs::read(&path)
        .map_err(|e| format!("{MANIFEST_FILE} is missing in {}: {e}", dir.display()))?;
    let text = String::from_utf8(text).map_err(|_| format!("{MANIFEST_FILE} is not UTF-8"))?;
    PluginManifest::parse(&text)
}

/// Pick the files of the plugin folder `dir` that go into its package. Fails on folder
/// layouts that cannot be packed (an icon theme file at the root, symbolic links, more than
/// 64 MB); a missing `main` is left to the package checks.
pub fn select(dir: &Path) -> Result<Selection, String> {
    let manifest = read_manifest(dir)?;
    let mut files = vec![(MANIFEST_FILE.to_string(), dir.join(MANIFEST_FILE))];
    for doc in DOCUMENTS {
        let path = dir.join(doc);
        if path.is_file() {
            files.push((doc.to_string(), path));
        }
    }
    if let Some(main) = manifest.main.as_deref().filter(|m| is_package_path(m)) {
        let path = dir.join(main);
        if path.is_file() && !main.starts_with(&format!("{DIST}/")) {
            files.push((main.to_string(), path));
        }
    }
    let dist = dir.join(DIST);
    if dist.is_dir() {
        walk(dir, &dist, &mut files)?;
    }
    for theme in &manifest.contributes.icon_themes {
        if !is_package_path(&theme.path) || !dir.join(&theme.path).is_file() {
            continue; // reported by the package checks
        }
        let Some((folder, _)) = theme.path.rsplit_once('/') else {
            return Err(format!(
                "put {} into a folder of its own (e.g. theme/), next to its SVGs",
                theme.path
            ));
        };
        walk(dir, &dir.join(folder), &mut files)?;
    }
    let mut seen = std::collections::HashSet::new();
    files.retain(|(p, _)| seen.insert(p.clone()));
    let mut total = 0u64;
    for (name, path) in &files {
        let len = std::fs::metadata(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?
            .len();
        total += len;
        if len > MAX_SIZE {
            return Err(format!("{name} is larger than 64 MB"));
        }
        if total > MAX_SIZE {
            return Err("the unpacked plugin is larger than 64 MB".into());
        }
    }
    Ok(Selection { manifest, files })
}

/// Pack the plugin folder `dir` into package bytes (without validating them; see
/// [`crate::validate`]).
pub fn pack(dir: &Path) -> Result<Packed, String> {
    let selection = select(dir)?;
    let mut entries = Vec::with_capacity(selection.files.len());
    for (name, path) in &selection.files {
        let data = std::fs::read(path).map_err(|e| format!("cannot read {name}: {e}"))?;
        entries.push((name.clone(), data));
    }
    let bytes = zip(&entries)?;
    Ok(Packed {
        manifest: selection.manifest,
        bytes,
        paths: entries.into_iter().map(|(n, _)| n).collect(),
    })
}

/// A reproducible zip of `entries` in the given order: deflated, 1980-01-01 timestamps,
/// `0644` permissions, no extra fields.
pub fn zip(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(9))
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);
    for (name, data) in entries {
        zip.start_file(name.as_str(), options)
            .map_err(|e| e.to_string())?;
        zip.write_all(data).map_err(|e| e.to_string())?;
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sdk(path: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sdk")
            .join(path)
    }

    #[test]
    fn packs_the_examples_like_the_sdk() {
        let p = pack(&sdk("examples/word-count")).unwrap();
        assert_eq!(p.file_name(), "example.word-count-1.0.0.forgeplugin");
        assert_eq!(p.paths, vec!["forge-plugin.json", "dist/plugin.js"]);
        let p = pack(&sdk("examples/shapes-icons")).unwrap();
        assert_eq!(p.paths[0], "forge-plugin.json");
        assert!(p.paths.contains(&"theme/icon-theme.json".to_string()));
        assert!(p.paths.iter().any(|f| f.starts_with("theme/icons/")));
        assert!(!p.paths.iter().any(|f| f == "package.json"));
        crate::Package::read(&p.bytes, crate::FORGE_VERSION).unwrap();
    }

    #[test]
    fn packing_is_reproducible() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join(MANIFEST_FILE),
            std::fs::read(sdk("examples/word-count/forge-plugin.json")).unwrap(),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("dist/assets")).unwrap();
        std::fs::write(root.join("dist/plugin.js"), "forge.log(1);").unwrap();
        std::fs::write(root.join("dist/assets/b.txt"), "b").unwrap();
        std::fs::write(root.join("dist/a.txt"), "a").unwrap();
        std::fs::write(root.join("NOTICE"), "notice").unwrap();
        std::fs::write(root.join("THIRD_PARTY_NOTICES.md"), "# notices").unwrap();
        std::fs::write(root.join("package.json"), "{}").unwrap();
        let a = pack(root).unwrap();
        assert_eq!(
            a.paths,
            vec![
                "forge-plugin.json",
                "THIRD_PARTY_NOTICES.md",
                "NOTICE",
                "dist/a.txt",
                "dist/assets/b.txt",
                "dist/plugin.js"
            ]
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(root.join("dist/a.txt"), "a").unwrap();
        let b = pack(root).unwrap();
        assert_eq!(a.bytes, b.bytes, "same sources, same bytes");
    }

    #[test]
    fn refuses_layouts_that_cannot_be_packed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert!(select(root).unwrap_err().contains("missing"));
        std::fs::write(
            root.join(MANIFEST_FILE),
            crate::icon_themes::tests::SHAPES_MANIFEST.replace("theme/icon-theme.json", "t.json"),
        )
        .unwrap();
        std::fs::write(root.join("t.json"), "{}").unwrap();
        assert!(select(root).unwrap_err().contains("folder of its own"));
        #[cfg(unix)]
        {
            std::fs::write(root.join(MANIFEST_FILE), crate::manifest::tests::WORD_COUNT).unwrap();
            std::fs::create_dir_all(root.join("dist")).unwrap();
            std::os::unix::fs::symlink("/etc/passwd", root.join("dist/plugin.js")).unwrap();
            assert!(select(root).unwrap_err().contains("symbolic links"));
        }
    }
}
