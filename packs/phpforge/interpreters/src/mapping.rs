//! Path mappings of PHP interpreters: the generic mapping model lives in
//! `forge-runtargets` (shared with the run targets of every pack); the mappings that follow
//! from an interpreter are derived here.

pub use forge_runtargets::mapping::{
    compose_mappings, is_absolute, join, normalize, parse_volume, wsl_path, Mapper, PathMapping,
    DOCKER_PROJECT_ROOT,
};

use crate::model::{InterpreterKind, PhpInterpreter};

/// The mappings that follow from the interpreter: Docker's `/opt/project` mount, the
/// compose service's volumes (`compose`: the compose file's directory and the service's
/// volumes) and WSL's drive paths.
pub fn automatic(
    interp: &PhpInterpreter,
    root: &str,
    compose: Option<(&str, &[String])>,
) -> Vec<PathMapping> {
    match interp.kind {
        InterpreterKind::Docker => vec![PathMapping {
            local: normalize(root),
            remote: DOCKER_PROJECT_ROOT.into(),
        }],
        InterpreterKind::Compose => compose
            .map(|(dir, volumes)| compose_mappings(root, dir, volumes))
            .unwrap_or_default(),
        InterpreterKind::Wsl => wsl_path(root)
            .map(|remote| {
                vec![PathMapping {
                    local: normalize(root),
                    remote,
                }]
            })
            .unwrap_or_default(),
        InterpreterKind::Local | InterpreterKind::Ssh => Vec::new(),
    }
}

/// The user's mappings followed by the automatic ones (the user's win for the same local
/// path, see [`Mapper::new`]).
pub fn effective(interp: &PhpInterpreter, automatic: Vec<PathMapping>) -> Vec<PathMapping> {
    let mut all = interp.mappings.clone();
    all.extend(automatic);
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(local: &str, remote: &str) -> PathMapping {
        PathMapping {
            local: local.into(),
            remote: remote.into(),
        }
    }

    #[test]
    fn user_mappings_win_over_automatic_ones() {
        let interp = PhpInterpreter {
            kind: InterpreterKind::Docker,
            mappings: vec![m("", "/app")],
            ..Default::default()
        };
        let all = effective(&interp, automatic(&interp, "/p", None));
        assert_eq!(all.len(), 2);
        let mapper = Mapper::new("/p", &all);
        assert_eq!(mapper.to_remote("/p/x.php").as_deref(), Some("/app/x.php"));
        let plain = PhpInterpreter {
            kind: InterpreterKind::Docker,
            ..Default::default()
        };
        let mapper = Mapper::new("/p", &effective(&plain, automatic(&plain, "/p", None)));
        assert_eq!(
            mapper.to_remote("/p/x.php").as_deref(),
            Some("/opt/project/x.php")
        );
    }

    #[test]
    fn wsl_interpreters_see_windows_drives() {
        let wsl = PhpInterpreter {
            kind: InterpreterKind::Wsl,
            ..Default::default()
        };
        assert_eq!(automatic(&wsl, "D:\\p", None), vec![m("D:/p", "/mnt/d/p")]);
    }
}
