//! Unpacking downloaded archives (`.tgz` npm tarballs, Node.js `.tar.gz` and `.zip`
//! builds) into a directory. Entries are checked before anything is written: absolute
//! paths, `..` components and symlinks pointing outside the destination are refused.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// `path` without its first `strip` components, or `None` when nothing is left or the path
/// is not a plain relative path (absolute, `..`, drive prefixes).
fn stripped(path: &Path, strip: usize) -> Result<Option<PathBuf>, String> {
    let mut out = PathBuf::new();
    for (i, c) in path.components().enumerate() {
        match c {
            Component::Normal(part) => {
                if i >= strip {
                    out.push(part);
                }
            }
            Component::CurDir => {}
            _ => return Err(format!("unsafe path in archive: {}", path.display())),
        }
    }
    Ok((!out.as_os_str().is_empty()).then_some(out))
}

/// Resolve `.` and `..` without touching the file system.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Unpack a gzip compressed tar archive into `dest`, dropping the first `strip` path
/// components of every entry (`package/` of npm tarballs, `node-v22.11.0-linux-x64/`).
pub fn extract_tar_gz(bytes: &[u8], dest: &Path, strip: usize) -> Result<(), String> {
    let gz = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(gz);
    std::fs::create_dir_all(dest).map_err(|e| format!("cannot create {}: {e}", dest.display()))?;
    let entries = archive
        .entries()
        .map_err(|e| format!("not a tar archive: {e}"))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| format!("broken archive: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("broken archive: {e}"))?
            .into_owned();
        let Some(rel) = stripped(&path, strip)? else {
            continue;
        };
        let target = dest.join(&rel);
        let kind = entry.header().entry_type();
        if kind.is_hard_link() || kind.is_character_special() || kind.is_block_special() {
            continue;
        }
        if kind.is_symlink() {
            let link = entry
                .link_name()
                .map_err(|e| format!("broken archive: {e}"))?
                .ok_or_else(|| format!("symlink without target: {}", path.display()))?;
            let parent = target.parent().unwrap_or(dest);
            if link.is_absolute() || !normalize(&parent.join(&link)).starts_with(normalize(dest)) {
                return Err(format!("symlink leaves the archive: {}", path.display()));
            }
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        entry
            .unpack(&target)
            .map_err(|e| format!("cannot write {}: {e}", target.display()))?;
    }
    Ok(())
}

/// Unpack a zip archive (Node.js for Windows) into `dest`, dropping `strip` components.
pub fn extract_zip(bytes: &[u8], dest: &Path, strip: usize) -> Result<(), String> {
    extract_zip_filtered(bytes, dest, strip, "")
}

/// Unpack the entries of a zip archive below `prefix` (`content/LanguageServer/linux-x64/`
/// of a NuGet package) into `dest`, without the prefix.
pub fn extract_zip_prefix(bytes: &[u8], dest: &Path, prefix: &str) -> Result<(), String> {
    let prefix = prefix.trim_matches('/');
    let strip = prefix.split('/').filter(|p| !p.is_empty()).count();
    extract_zip_filtered(bytes, dest, strip, prefix)
}

fn extract_zip_filtered(
    bytes: &[u8],
    dest: &Path,
    strip: usize,
    prefix: &str,
) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("not a zip archive: {e}"))?;
    std::fs::create_dir_all(dest).map_err(|e| format!("cannot create {}: {e}", dest.display()))?;
    for i in 0..zip.len() {
        let mut file = zip
            .by_index(i)
            .map_err(|e| format!("broken archive: {e}"))?;
        if !prefix.is_empty() {
            let n = file.name().replace('\\', "/");
            if !n.starts_with(&format!("{prefix}/")) {
                continue;
            }
        }
        let name = file
            .enclosed_name()
            .ok_or_else(|| format!("unsafe path in archive: {}", file.name()))?;
        let Some(rel) = stripped(&name, strip)? else {
            continue;
        };
        let target = dest.join(rel);
        if file.is_dir() {
            std::fs::create_dir_all(&target)
                .map_err(|e| format!("cannot create {}: {e}", target.display()))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        let mut data = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut data)
            .map_err(|e| format!("broken archive: {e}"))?;
        std::fs::write(&target, data)
            .map_err(|e| format!("cannot write {}: {e}", target.display()))?;
        #[cfg(unix)]
        if let Some(mode) = file.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode & 0o777));
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// A `.tar.gz` with `(path, contents, mode)` file entries.
    pub fn tar_gz(files: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, data, mode) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(*mode);
            header.set_entry_type(tar::EntryType::Regular);
            builder.append_data(&mut header, path, *data).unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).unwrap();
        gz.finish().unwrap()
    }

    /// A `.tar.gz` with one entry whose path is written verbatim (no checks by `tar`).
    fn raw_tar_gz(path: &str, kind: tar::EntryType, link: Option<&str>) -> Vec<u8> {
        let mut header = tar::Header::new_old();
        {
            let bytes = header.as_old_mut();
            bytes.name[..path.len()].copy_from_slice(path.as_bytes());
            if let Some(l) = link {
                bytes.linkname[..l.len()].copy_from_slice(l.as_bytes());
            }
        }
        header.set_size(if link.is_some() { 0 } else { 2 });
        header.set_mode(0o644);
        header.set_entry_type(kind);
        header.set_cksum();
        let mut tar = header.as_bytes().to_vec();
        if link.is_none() {
            let mut block = vec![0u8; 512];
            block[..2].copy_from_slice(b"hi");
            tar.extend(block);
        }
        tar.extend(vec![0u8; 1024]);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).unwrap();
        gz.finish().unwrap()
    }

    pub fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (path, data) in files {
            w.start_file(*path, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn unpacks_tarballs_and_strips_the_top_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let tgz = tar_gz(&[
            ("package/package.json", b"{}", 0o644),
            ("package/lib/server.js", b"// js", 0o755),
        ]);
        extract_tar_gz(&tgz, tmp.path(), 1).unwrap();
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("lib/server.js")).unwrap(),
            "// js"
        );
        assert!(tmp.path().join("package.json").is_file());
        assert!(!tmp.path().join("package").exists());
    }

    #[test]
    fn refuses_paths_and_links_that_leave_the_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        let evil = raw_tar_gz("package/../../evil.txt", tar::EntryType::Regular, None);
        assert!(extract_tar_gz(&evil, &dest, 1)
            .unwrap_err()
            .contains("unsafe path"));
        assert!(!tmp.path().join("evil.txt").exists());

        let link = raw_tar_gz(
            "package/bin/x",
            tar::EntryType::Symlink,
            Some("../../../etc/passwd"),
        );
        assert!(extract_tar_gz(&link, &dest, 1)
            .unwrap_err()
            .contains("symlink"));
        // A link inside the archive is fine (Node.js ships bin/npm -> ../lib/...).
        let ok = raw_tar_gz(
            "node/bin/npm",
            tar::EntryType::Symlink,
            Some("../lib/node_modules/npm/bin/npm-cli.js"),
        );
        extract_tar_gz(&ok, &dest, 1).unwrap();
    }

    #[test]
    fn unpacks_zip_archives() {
        let tmp = tempfile::tempdir().unwrap();
        let data = zip(&[
            ("node-v22.0.0-win-x64/node.exe", b"MZ"),
            (
                "node-v22.0.0-win-x64/node_modules/npm/bin/npm-cli.js",
                b"//",
            ),
        ]);
        extract_zip(&data, tmp.path(), 1).unwrap();
        assert_eq!(std::fs::read(tmp.path().join("node.exe")).unwrap(), b"MZ");
        assert!(tmp.path().join("node_modules/npm/bin/npm-cli.js").is_file());
        assert!(extract_zip(b"not a zip", tmp.path(), 1).is_err());
    }
}
