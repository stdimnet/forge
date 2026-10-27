//! Configuration metadata out of the dependency jars (feature `jars`): the jars come from
//! the Gradle / Maven sync (resolved coordinates looked up in the local Maven repository
//! and Gradle's module cache) or, without a sync, Spring Boot's own jars of the build's
//! Spring Boot version (else the newest one in the caches). Jars are read as zip files;
//! enum constants of property types are read from their class files.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::metadata::Metadata;

pub const METADATA: &str = "META-INF/spring-configuration-metadata.json";
pub const ADDITIONAL: &str = "META-INF/additional-spring-configuration-metadata.json";

/// A resolved dependency.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Coordinate {
    pub group: String,
    pub artifact: String,
    pub version: String,
}

/// Where the local caches are: `~/.m2/repository` and `~/.gradle/caches/modules-2/files-2.1`.
#[derive(Debug, Clone, Default)]
pub struct Caches {
    pub maven: Option<PathBuf>,
    pub gradle: Option<PathBuf>,
}

impl Caches {
    /// The default locations under `home` (`M2_REPO`-style overrides are the caller's).
    pub fn under_home(home: &Path) -> Self {
        Self {
            maven: Some(home.join(".m2").join("repository")),
            gradle: Some(home.join(".gradle").join("caches").join("modules-2").join("files-2.1")),
        }
    }

    /// The jar of a coordinate in one of the caches.
    pub fn jar(&self, c: &Coordinate) -> Option<PathBuf> {
        let file = format!("{}-{}.jar", c.artifact, c.version);
        if let Some(m2) = &self.maven {
            let p = m2
                .join(c.group.replace('.', "/"))
                .join(&c.artifact)
                .join(&c.version)
                .join(&file);
            if p.is_file() {
                return Some(p);
            }
        }
        let dir = self.gradle.as_ref()?.join(&c.group).join(&c.artifact).join(&c.version);
        fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|e| e.path().join(&file))
            .find(|p| p.is_file())
    }

    /// Versions of `group:artifact` in the caches.
    pub fn versions(&self, group: &str, artifact: &str) -> Vec<String> {
        let mut out = BTreeSet::new();
        let dirs = [
            self.maven.as_ref().map(|m| m.join(group.replace('.', "/")).join(artifact)),
            self.gradle.as_ref().map(|g| g.join(group).join(artifact)),
        ];
        for d in dirs.into_iter().flatten() {
            for e in fs::read_dir(d).into_iter().flatten().flatten() {
                if e.path().is_dir() {
                    out.insert(e.file_name().to_string_lossy().to_string());
                }
            }
        }
        out.into_iter().collect()
    }
}

/// Compare versions numerically where possible (`3.10.0` > `3.9.1`).
fn version_key(v: &str) -> Vec<(u64, String)> {
    v.split(['.', '-'])
        .map(|p| (p.parse::<u64>().unwrap_or(0), if p.parse::<u64>().is_ok() { String::new() } else { p.to_string() }))
        .collect()
}

/// Spring Boot artifacts that carry configuration metadata.
pub const BOOT_ARTIFACTS: &[&str] = &[
    "spring-boot",
    "spring-boot-autoconfigure",
    "spring-boot-actuator-autoconfigure",
    "spring-boot-actuator",
    "spring-boot-devtools",
];

/// Jars to read: every resolved dependency found in the caches; without a sync (no
/// dependencies), Spring Boot's own jars of `boot_version` or the newest cached version.
pub fn jars_for(deps: &[Coordinate], boot_version: Option<&str>, caches: &Caches) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = deps.iter().filter_map(|c| caches.jar(c)).collect();
    if !deps.iter().any(|d| d.artifact == "spring-boot-autoconfigure") {
        for artifact in BOOT_ARTIFACTS {
            let group = "org.springframework.boot";
            let mut versions = caches.versions(group, artifact);
            versions.sort_by_key(|v| version_key(v));
            let version = boot_version
                .filter(|b| versions.iter().any(|v| v == b))
                .map(str::to_string)
                .or_else(|| versions.last().cloned());
            if let Some(v) = version {
                if let Some(jar) = caches.jar(&Coordinate {
                    group: group.into(),
                    artifact: artifact.to_string(),
                    version: v,
                }) {
                    if !out.contains(&jar) {
                        out.push(jar);
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn read_entry(archive: &mut zip::ZipArchive<fs::File>, name: &str) -> Option<Vec<u8>> {
    let mut f = archive.by_name(name).ok()?;
    let mut buf = Vec::with_capacity(f.size() as usize);
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// The metadata files of a jar: (main, additional).
pub fn jar_metadata(jar: &Path) -> Result<(Option<String>, Option<String>), String> {
    let file = fs::File::open(jar).map_err(|e| format!("{}: {e}", jar.display()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("{}: {e}", jar.display()))?;
    let text = |a: &mut zip::ZipArchive<fs::File>, n: &str| read_entry(a, n).map(|b| String::from_utf8_lossy(&b).to_string());
    Ok((text(&mut archive, METADATA), text(&mut archive, ADDITIONAL)))
}

/// Load the metadata of `jars` into `meta`, then the enum constants of the property types
/// from the same jars. Returns the problems (unreadable jars, broken JSON).
pub fn load(jars: &[PathBuf], meta: &mut Metadata) -> Vec<String> {
    let mut problems = Vec::new();
    for jar in jars {
        let name = jar.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        match jar_metadata(jar) {
            Ok((main, additional)) => {
                for json in [main, additional].into_iter().flatten() {
                    if let Err(e) = meta.add_json(&json, &name) {
                        problems.push(e);
                    }
                }
            }
            Err(e) => problems.push(e),
        }
    }
    let types: BTreeSet<String> = meta
        .properties
        .values()
        .filter_map(|p| {
            let t = p.type_name.as_deref()?;
            // Map / collection element types too.
            Some(match p.element_type() {
                Some(e) if p.is_map() || p.is_collection() => e,
                _ => t.to_string(),
            })
        })
        .filter(|t| t.contains('.') && !t.starts_with("java.") && !t.contains('<') && !meta.enums.contains_key(t))
        .collect();
    if !types.is_empty() {
        for (t, consts) in enum_constants(jars, &types) {
            meta.enums.insert(t, consts);
        }
    }
    problems
}

/// Class file names a (possibly nested) type may have: `a.b.C.D` → `a/b/C$D.class`, …
fn class_paths(t: &str) -> Vec<String> {
    let t = t.replace('$', ".");
    let parts: Vec<&str> = t.split('.').collect();
    let mut out = Vec::new();
    for split in (1..parts.len()).rev() {
        let (pkg, nested) = parts.split_at(split);
        if nested.first().is_some_and(|n| n.chars().next().is_some_and(char::is_lowercase)) {
            continue;
        }
        out.push(format!("{}/{}.class", pkg.join("/"), nested.join("$")));
    }
    out
}

/// Enum constants of `types` found in `jars`.
pub fn enum_constants(jars: &[PathBuf], types: &BTreeSet<String>) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    let mut todo: Vec<&String> = types.iter().collect();
    for jar in jars {
        if todo.is_empty() {
            break;
        }
        let Ok(file) = fs::File::open(jar) else { continue };
        let Ok(mut archive) = zip::ZipArchive::new(file) else { continue };
        todo.retain(|t| {
            for path in class_paths(t) {
                if let Some(bytes) = read_entry(&mut archive, &path) {
                    if let Some(consts) = class_enum_constants(&bytes) {
                        out.insert((*t).clone(), consts);
                    }
                    return false;
                }
            }
            true
        });
    }
    out
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn u1(&mut self) -> Option<u8> {
        let v = *self.b.get(self.i)?;
        self.i += 1;
        Some(v)
    }
    fn u2(&mut self) -> Option<u16> {
        Some(u16::from(self.u1()?) << 8 | u16::from(self.u1()?))
    }
    fn u4(&mut self) -> Option<u32> {
        Some(u32::from(self.u2()?) << 16 | u32::from(self.u2()?))
    }
    fn skip(&mut self, n: usize) -> Option<()> {
        (self.i + n <= self.b.len()).then(|| self.i += n)
    }
}

const ACC_ENUM: u16 = 0x4000;

/// The constants of an enum class file (`None` when the class is not an enum).
pub fn class_enum_constants(bytes: &[u8]) -> Option<Vec<String>> {
    let mut r = Reader { b: bytes, i: 0 };
    if r.u4()? != 0xCAFE_BABE {
        return None;
    }
    r.skip(4)?;
    let count = r.u2()? as usize;
    let mut utf8: Vec<Option<String>> = vec![None; count];
    let mut idx = 1;
    while idx < count {
        let tag = r.u1()?;
        match tag {
            1 => {
                let len = r.u2()? as usize;
                let s = r.b.get(r.i..r.i + len)?;
                utf8[idx] = Some(String::from_utf8_lossy(s).to_string());
                r.skip(len)?;
            }
            3 | 4 | 9 | 10 | 11 | 12 | 17 | 18 => r.skip(4)?,
            5 | 6 => {
                r.skip(8)?;
                idx += 1;
            }
            7 | 8 | 16 | 19 | 20 => r.skip(2)?,
            15 => r.skip(3)?,
            _ => return None,
        }
        idx += 1;
    }
    let access = r.u2()?;
    if access & ACC_ENUM == 0 {
        return None;
    }
    r.skip(4)?;
    let interfaces = r.u2()? as usize;
    r.skip(interfaces * 2)?;
    let fields = r.u2()?;
    let mut out = Vec::new();
    for _ in 0..fields {
        let flags = r.u2()?;
        let name = r.u2()? as usize;
        r.skip(2)?;
        let attrs = r.u2()?;
        for _ in 0..attrs {
            r.skip(2)?;
            let len = r.u4()? as usize;
            r.skip(len)?;
        }
        if flags & ACC_ENUM != 0 {
            if let Some(Some(n)) = utf8.get(name) {
                out.push(n.clone());
            }
        }
    }
    Some(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// A minimal class file: `name` with `constants` as enum fields (or a plain class).
    pub(crate) fn class_file(name: &str, constants: &[&str], is_enum: bool) -> Vec<u8> {
        let mut pool: Vec<Vec<u8>> = Vec::new();
        let utf8 = |s: &str| {
            let mut v = vec![1u8];
            v.extend((s.len() as u16).to_be_bytes());
            v.extend(s.as_bytes());
            v
        };
        pool.push(utf8(name)); // 1
        pool.push(vec![7, 0, 1]); // 2: Class
        pool.push(utf8("java/lang/Enum")); // 3
        pool.push(vec![7, 0, 3]); // 4
        pool.push(vec![5, 0, 0, 0, 0, 0, 0, 0, 42]); // 5-6: Long (two slots)
        pool.push(utf8(&format!("L{name};"))); // 7
        let first_const = 8u16;
        for c in constants {
            pool.push(utf8(c));
        }
        let count = 8 + constants.len() as u16;
        let mut b = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 61];
        b.extend(count.to_be_bytes());
        for e in &pool {
            b.extend(e);
        }
        let access: u16 = if is_enum { 0x4031 } else { 0x0021 };
        b.extend(access.to_be_bytes());
        b.extend([0, 2, 0, 4, 0, 0]);
        b.extend((constants.len() as u16).to_be_bytes());
        for i in 0..constants.len() as u16 {
            b.extend(0x4019u16.to_be_bytes());
            b.extend((first_const + i).to_be_bytes());
            b.extend(7u16.to_be_bytes());
            b.extend(1u16.to_be_bytes()); // one attribute
            b.extend([0, 1, 0, 0, 0, 2, 0, 0]);
        }
        b.extend([0, 0, 0, 0]);
        b
    }

    /// Write a jar with `entries` (name, bytes).
    pub(crate) fn write_jar(path: &Path, entries: &[(&str, Vec<u8>)]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn reads_enum_constants_from_class_files() {
        let b = class_file("org/acme/Mode", &["OFF", "CONSOLE", "LOG"], true);
        assert_eq!(class_enum_constants(&b).unwrap(), ["OFF", "CONSOLE", "LOG"]);
        assert_eq!(class_enum_constants(&class_file("org/acme/Plain", &[], false)), None);
        assert_eq!(class_enum_constants(b"nope"), None);
        assert_eq!(class_paths("org.springframework.boot.Banner$Mode")[0], "org/springframework/boot/Banner/Mode.class");
        assert!(class_paths("org.springframework.boot.Banner.Mode").contains(&"org/springframework/boot/Banner$Mode.class".to_string()));
    }

    #[test]
    fn finds_jars_in_the_caches_and_loads_their_metadata() {
        let home = tempfile::tempdir().unwrap();
        let caches = Caches::under_home(home.path());
        let m2 = caches.maven.clone().unwrap();
        let gradle = caches.gradle.clone().unwrap();
        let meta = crate::metadata::TEST_METADATA.as_bytes().to_vec();
        // Spring Boot 3.3.2 in the Maven repository, 3.2.0 and 3.10.1 in Gradle's cache.
        write_jar(
            &m2.join("org/springframework/boot/spring-boot-autoconfigure/3.3.2/spring-boot-autoconfigure-3.3.2.jar"),
            &[(METADATA, meta.clone())],
        );
        write_jar(
            &m2.join("org/springframework/boot/spring-boot/3.3.2/spring-boot-3.3.2.jar"),
            &[
                (ADDITIONAL, br#"{"properties":[{"name":"spring.main.lazy-initialization","type":"java.lang.Boolean"}]}"#.to_vec()),
                ("org/springframework/boot/Banner$Mode.class", class_file("org/springframework/boot/Banner$Mode", &["OFF", "CONSOLE", "LOG"], true)),
                ("org/springframework/boot/logging/LogLevel.class", class_file("org/springframework/boot/logging/LogLevel", &["TRACE", "DEBUG", "INFO", "WARN", "ERROR", "FATAL", "OFF"], true)),
            ],
        );
        for v in ["3.2.0", "3.10.1"] {
            write_jar(
                &gradle.join(format!("org.springframework.boot/spring-boot-autoconfigure/{v}/0a1b2c/spring-boot-autoconfigure-{v}.jar")),
                &[(METADATA, meta.clone())],
            );
        }
        write_jar(&gradle.join("com.acme/lib/1.0/ff/lib-1.0.jar"), &[("README", b"no metadata".to_vec())]);
        fs::write(m2.join("org/springframework/boot/spring-boot/3.3.2/broken.jar"), b"not a zip").unwrap();

        // Without a sync: the build's Spring Boot version.
        let jars = jars_for(&[], Some("3.3.2"), &caches);
        assert_eq!(jars.len(), 2, "{jars:?}");
        // Unknown version: the newest cached one.
        let newest = jars_for(&[], Some("9.9.9"), &caches);
        assert!(newest.iter().any(|j| j.to_string_lossy().contains("3.10.1")), "{newest:?}");
        // From a sync: resolved coordinates.
        let synced = jars_for(
            &[
                Coordinate { group: "org.springframework.boot".into(), artifact: "spring-boot-autoconfigure".into(), version: "3.2.0".into() },
                Coordinate { group: "com.acme".into(), artifact: "lib".into(), version: "1.0".into() },
                Coordinate { group: "com.acme".into(), artifact: "missing".into(), version: "1.0".into() },
            ],
            None,
            &caches,
        );
        assert_eq!(synced.len(), 2);

        let mut m = Metadata::default();
        let mut with_broken = jars.clone();
        with_broken.push(m2.join("org/springframework/boot/spring-boot/3.3.2/broken.jar"));
        let problems = load(&with_broken, &mut m);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(m.get("server.port").is_some());
        assert!(m.get("spring.main.lazy-initialization").unwrap().is_boolean());
        assert_eq!(m.enums["org.springframework.boot.Banner$Mode"], ["OFF", "CONSOLE", "LOG"]);
        assert_eq!(m.enums["org.springframework.boot.logging.LogLevel"].len(), 7);
        let values: Vec<String> = m.values_of("spring.main.banner-mode").into_iter().map(|(v, _)| v).collect();
        assert_eq!(values, ["off", "console", "log"]);
        assert_eq!(m.sources.len(), 2);
    }
}
