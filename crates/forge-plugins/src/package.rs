//! `.forgeplugin` packages: a zip file with `forge-plugin.json` at its root and the files it
//! names (`main`, `README.md`, `icon.svg`, …).

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use crate::manifest::{describe, is_package_path, PluginManifest, MANIFEST_FILE};

/// File extension of plugin packages.
pub const EXTENSION: &str = "forgeplugin";
/// Largest package (compressed or unpacked) Forge installs.
pub const MAX_SIZE: u64 = 64 * 1024 * 1024;
/// Most files a package may hold.
pub const MAX_ENTRIES: usize = 4096;

/// A package that was read and validated.
#[derive(Debug, Clone)]
pub struct Package {
    pub manifest: PluginManifest,
    /// Files by package path (`/` separated), the manifest included.
    pub files: BTreeMap<String, Vec<u8>>,
    /// SHA-256 of the package bytes, lowercase hex.
    pub sha256: String,
}

pub use crate::sha256_hex;

impl Package {
    /// Read a package and validate its manifest for `forge_version`.
    pub fn read(bytes: &[u8], forge_version: &str) -> Result<Self, String> {
        let files = read_entries(bytes)?;
        let manifest_src = files
            .get(MANIFEST_FILE)
            .ok_or_else(|| format!("{MANIFEST_FILE} is missing at the package root"))?;
        let manifest = PluginManifest::parse(
            std::str::from_utf8(manifest_src)
                .map_err(|_| format!("{MANIFEST_FILE} is not UTF-8"))?,
        )?;
        let problems = manifest.validate(forge_version);
        if !problems.is_empty() {
            return Err(format!("invalid plugin manifest: {}", describe(&problems)));
        }
        if let Some(main) = &manifest.main {
            if !files.contains_key(main) {
                return Err(format!("`main` names {main}, which is not in the package"));
            }
        }
        // Icon themes are checked on install, so a broken theme never reaches the settings.
        for theme in &manifest.contributes.icon_themes {
            crate::icon_themes::load(&manifest, &theme.id, |p| {
                files
                    .get(p)
                    .cloned()
                    .ok_or_else(|| format!("{p} is not in the package"))
            })
            .map_err(|e| format!("icon theme `{}`: {e}", theme.id))?;
        }
        Ok(Self {
            manifest,
            files,
            sha256: sha256_hex(bytes),
        })
    }
}

/// The files of a package by package path, read safely: at most [`MAX_SIZE`] packed and
/// unpacked, at most [`MAX_ENTRIES`] entries, no absolute paths, `..`, drive letters or
/// symbolic links. The manifest is not looked at.
pub fn read_entries(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if bytes.len() as u64 > MAX_SIZE {
        return Err("the plugin package is larger than 64 MB".into());
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("not a .{EXTENSION} package: {e}"))?;
    if zip.len() > MAX_ENTRIES {
        return Err("the plugin package has too many files".into());
    }
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| format!("broken package: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        if !is_package_path(&name) || entry.enclosed_name().is_none() {
            return Err(format!("unsafe path in package: {name}"));
        }
        if entry.is_symlink() {
            return Err(format!("symbolic links are not allowed: {name}"));
        }
        let mut data = Vec::new();
        entry
            .by_ref()
            .take(MAX_SIZE + 1 - total)
            .read_to_end(&mut data)
            .map_err(|e| format!("broken package: {e}"))?;
        total += data.len() as u64;
        if total > MAX_SIZE {
            return Err("the unpacked plugin is larger than 64 MB".into());
        }
        files.insert(name, data);
    }
    Ok(files)
}

/// Build a package from files (tests and the SDK's packer use the same layout).
pub fn write(files: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in files {
        zip.start_file(*name, options).map_err(|e| e.to_string())?;
        zip.write_all(data).map_err(|e| e.to_string())?;
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::manifest::tests::WORD_COUNT;

    pub fn word_count_package(version: &str) -> Vec<u8> {
        let manifest = WORD_COUNT.replace("\"1.0.0\"", &format!("\"{version}\""));
        write(&[
            (MANIFEST_FILE, manifest.as_bytes()),
            ("dist/plugin.js", b"forge.log('hi');"),
            ("README.md", b"# Word Count"),
        ])
        .unwrap()
    }

    #[test]
    fn icon_themes_are_checked_on_install() {
        let files = crate::icon_themes::tests::shapes_files();
        let list: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .collect();
        assert!(Package::read(&write(&list).unwrap(), "0.1.0").is_ok());
        let broken: Vec<(&str, &[u8])> = list
            .iter()
            .copied()
            .filter(|(k, _)| !k.ends_with("php.svg"))
            .collect();
        let err = Package::read(&write(&broken).unwrap(), "0.1.0").unwrap_err();
        assert!(
            err.contains("icon theme `example.shapes-icons.shapes`"),
            "{err}"
        );
    }

    #[test]
    fn reads_a_valid_package() {
        let bytes = word_count_package("1.0.0");
        let p = Package::read(&bytes, "0.1.0").unwrap();
        assert_eq!(p.manifest.id, "example.word-count");
        assert_eq!(p.files.len(), 3);
        assert_eq!(p.sha256, sha256_hex(&bytes));
        assert_eq!(p.sha256.len(), 64);
    }

    #[test]
    fn refuses_broken_packages() {
        assert!(Package::read(b"nope", "0.1.0")
            .unwrap_err()
            .contains("not a .forgeplugin"));
        let no_manifest = write(&[("dist/plugin.js", b"x")]).unwrap();
        assert!(Package::read(&no_manifest, "0.1.0")
            .unwrap_err()
            .contains("missing"));
        let no_main = write(&[(MANIFEST_FILE, WORD_COUNT.as_bytes())]).unwrap();
        assert!(Package::read(&no_main, "0.1.0")
            .unwrap_err()
            .contains("not in the package"));
        let evil = write(&[
            (MANIFEST_FILE, WORD_COUNT.as_bytes()),
            ("dist/plugin.js", b"x"),
            ("../../evil.sh", b"x"),
        ])
        .unwrap();
        assert!(Package::read(&evil, "0.1.0")
            .unwrap_err()
            .contains("unsafe path"));
        let old_forge = word_count_package("1.0.0");
        assert!(Package::read(&old_forge, "0.0.9")
            .unwrap_err()
            .contains("engines.forge"));
    }
}

#[cfg(test)]
mod sdk_tests {
    use super::*;
    use std::path::Path;

    fn sdk(path: &str) -> Vec<u8> {
        let file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sdk")
            .join(path);
        std::fs::read(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
    }

    /// The SDK's example plugin, as `pnpm plugin pack` packs it, installs.
    #[test]
    fn the_word_count_example_is_a_valid_package() {
        let bytes = write(&[
            (MANIFEST_FILE, &sdk("examples/word-count/forge-plugin.json")),
            ("dist/plugin.js", &sdk("examples/word-count/dist/plugin.js")),
        ])
        .unwrap();
        let p = Package::read(&bytes, crate::FORGE_VERSION).unwrap();
        assert_eq!(p.manifest.id, "example.word-count");
        let bundle = String::from_utf8(p.files["dist/plugin.js"].clone()).unwrap();
        assert!(
            bundle.contains("var __forgePlugin"),
            "dist/plugin.js is an esbuild IIFE"
        );
    }

    /// `pnpm plugin new` fills the template's placeholders into a valid manifest.
    #[test]
    fn the_template_manifest_is_valid_once_filled() {
        let template = String::from_utf8(sdk("templates/basic/forge-plugin.json")).unwrap();
        let filled = template
            .replace("{{id}}", "acme.hello")
            .replace("{{name}}", "Hello")
            .replace("{{publisher}}", "Acme")
            .replace("{{description}}", "Hi")
            .replace("{{forgeVersion}}", crate::FORGE_VERSION);
        let m = PluginManifest::parse(&filled).unwrap();
        assert_eq!(m.validate(crate::FORGE_VERSION), vec![]);
    }
}
