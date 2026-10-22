//! Structural Search and Replace over a project (`forge-ssr`). The search runs where the
//! files are ([`Backend::structural_search`], the agent for remote projects); replacing reads
//! and writes the matched files through the backend and keeps them for Undo, like Replace in
//! Files.

use forge_core::{CollectRequest, ForgeError, Result, Workspace};
use forge_ssr::{Compiled, SsrRequest, SsrResult};

use crate::replace::{ReplaceDone, Restore};
use crate::Backend;

/// Files a structural search reads at most.
const MAX_FILES: usize = 100_000;
/// Files larger than this are skipped (bundles, generated code).
const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;
/// Text read per search at most.
const MAX_TOTAL: u64 = 1 << 30;
/// Matches a replace can touch in one run.
const MAX_MATCHES: usize = 1_000_000;

/// Search the files of a local project folder.
pub fn search_workspace(workspace: &Workspace, req: &SsrRequest) -> Result<SsrResult> {
    if let Err(e) = Compiled::new(req) {
        return Ok(SsrResult {
            error: Some(e),
            ..SsrResult::default()
        });
    }
    let files = workspace.collect_files(&CollectRequest {
        extensions: req
            .file_type
            .extensions()
            .iter()
            .map(|e| e.to_string())
            .collect(),
        max_files: MAX_FILES,
        max_file_size: MAX_FILE_SIZE,
        max_total: MAX_TOTAL,
    })?;
    Ok(forge_ssr::search_files(
        req,
        files.iter().map(|f| (f.path.as_str(), f.text.as_str())),
    ))
}

/// Replace every match of `req` (with its replacement template) in the project.
pub fn replace_all(backend: &dyn Backend, req: &SsrRequest) -> Result<ReplaceDone> {
    if req.replacement.is_none() {
        return Err(ForgeError::Other("No replacement template".into()));
    }
    let compiled = Compiled::new(req).map_err(ForgeError::Other)?;
    let mut search = req.clone();
    search.limit = MAX_MATCHES;
    let found = backend.structural_search(&search)?;
    if let Some(error) = found.error {
        return Err(ForgeError::Other(error));
    }
    let mut done = ReplaceDone::default();
    for file in found.files {
        let path = file.path;
        let text = match req.buffers.get(&path) {
            Some(t) => t.clone(),
            None => match backend.read_file(&path) {
                Ok(t) => t,
                Err(e) => {
                    done.skipped.push(format!("{path}: {e}"));
                    continue;
                }
            },
        };
        let Some((after, count)) = compiled.replace(&path, &text) else {
            continue;
        };
        if count == 0 || after == text {
            continue;
        }
        if let Err(e) = backend.write_file(&path, &after) {
            done.skipped.push(format!("{path}: {e}"));
            continue;
        }
        done.files += 1;
        done.replacements += count;
        done.undo.push(Restore {
            path,
            before: text,
            after,
        });
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalBackend;

    #[test]
    fn searches_and_replaces_a_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("app")).unwrap();
        std::fs::create_dir_all(dir.path().join("tests")).unwrap();
        std::fs::write(
            dir.path().join("app/a.php"),
            "<?php\n$order->save();\n$user->save(true);\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("tests/b.php"), "<?php\n$x->save();\n").unwrap();
        std::fs::write(dir.path().join("c.js"), "order.save()\n").unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();
        let mut req = SsrRequest {
            template: "$o$->save()".into(),
            ..Default::default()
        };
        let found = backend.structural_search(&req).unwrap();
        assert_eq!(found.match_count, 2);
        assert_eq!(found.searched_files, 2);
        req.scope = Some(forge_scopes::ScopeFilter {
            scope: forge_scopes::ScopeSpec::Predefined(
                forge_scopes::Predefined::ProjectProductionFiles,
            ),
            ..Default::default()
        });
        req.replacement = Some("$o$->persist()".into());
        let done = replace_all(&backend, &req).unwrap();
        assert_eq!((done.files, done.replacements), (1, 1));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("app/a.php")).unwrap(),
            "<?php\n$order->persist();\n$user->save(true);\n"
        );
        let (restored, _) = crate::replace::undo(&backend, &done.undo).unwrap();
        assert_eq!(restored, 1);
        req.template = "if (".into();
        assert!(backend.structural_search(&req).unwrap().error.is_some());
    }
}
