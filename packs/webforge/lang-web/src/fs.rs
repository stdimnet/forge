//! Loading [`Sources`] from disk (desktop app only; the browser mock passes its demo files).

use std::path::Path;

use crate::source::{is_vendored, SourceFile, Sources};

/// Files larger than this are listed but not read (bundles, generated code).
const MAX_FILE_SIZE: u64 = 512 * 1024;

/// Whether the index reads a file's content (scripts, templates and manifests).
pub fn wants_content(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if matches!(name, "package.json" | "jsconfig.json" | "angular.json")
        || (name.starts_with("tsconfig") && name.ends_with(".json"))
    {
        return true;
    }
    let ext = name.rsplit('.').next().unwrap_or("");
    matches!(
        ext,
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts" | "vue" | "html" | "htm"
    ) && !name.ends_with(".min.js")
        && !name.ends_with(".d.ts")
}

/// Walk the project (honouring `.gitignore`, skipping `node_modules` and build output) and
/// read up to `limit` source files.
pub fn load(root: &Path, limit: usize) -> Sources {
    let mut files = Vec::new();
    let mut others = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .require_git(false)
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !matches!(
                name.as_ref(),
                "node_modules"
                    | "vendor"
                    | "dist"
                    | ".git"
                    | ".nuxt"
                    | ".next"
                    | ".angular"
                    | ".output"
                    | "coverage"
                    | "target"
            )
        })
        .build();
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(root) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        if is_vendored(&rel) {
            continue;
        }
        let small = entry
            .metadata()
            .map(|m| m.len() <= MAX_FILE_SIZE)
            .unwrap_or(false);
        if files.len() < limit && small && wants_content(&rel) {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                files.push(SourceFile { path: rel, content });
                continue;
            }
        }
        others.push(rel);
    }
    Sources::new(files, others)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_sources_and_skips_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let write = |p: &str, c: &str| {
            let path = dir.path().join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, c).unwrap();
        };
        write("package.json", "{\"scripts\":{\"dev\":\"vite\"}}");
        write("pnpm-lock.yaml", "lockfileVersion: '9.0'");
        write(
            "src/App.tsx",
            "export default function App() { return <div/>; }",
        );
        write("src/app.css", "body{}");
        write("node_modules/react/package.json", "{}");
        write("dist/assets/index.js", "x");
        let s = load(dir.path(), 100);
        assert!(s.get("src/App.tsx").is_some_and(|c| c.contains("App")));
        assert!(s.exists("src/app.css") && s.get("src/app.css").is_none());
        assert!(s.exists("pnpm-lock.yaml"));
        assert!(!s.exists("node_modules/react/package.json"));
        assert!(!s.exists("dist/assets/index.js"));
    }
}
