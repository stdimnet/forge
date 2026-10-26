//! GoForge's Go language support, in the style of GoLand. Pure Rust (no I/O), shared by the
//! desktop backend and the WebAssembly build of browser mode.
//!
//! - [`scan`]: a tolerant Go tokenizer.
//! - [`gomod`]: `go.mod` / `go.work` with positions, problems and completion contexts.
//! - [`version`]: module versions (semver, pseudo-versions) and proxy path escaping.
//! - [`discovery`]: tests, benchmarks, fuzz tests, examples and literal subtests.
//! - [`provider`]: the Go Test provider of the shared test runner (`go test -json`,
//!   coverage, profiles, Rerun Failed Tests).
//! - [`build`]: Go Build configurations (`go run` / `go build`) and module commands.
//! - [`generate`]: `//go:generate` directives and their `go generate` commands.
//! - [`lint`]: `go vet -json` and golangci-lint reports as problems.
//! - [`env`]: the project's Go context (SDK, GOPATH, GOFLAGS, tags, modules) and `go env`.

pub mod build;
pub mod discovery;
pub mod env;
pub mod generate;
pub mod gomod;
pub mod lint;
pub mod provider;
pub mod scan;
pub mod version;

pub use build::{build_command, build_target, module_command, GoCommand, GO_BUILD};
pub use discovery::{discover, GoTest, GoTestFile, GoTestKind};
pub use env::{parse_go_env, GoContext, GoEnv, ModuleRoot, CONTEXT_KEY};
pub use generate::{directives as generate_directives, generate_args, GenerateDirective};
pub use gomod::{parse_go_mod, parse_go_work, GoMod, GoWork, ModCompletion};
pub use lint::{parse_golangci, parse_vet, GoProblem};
pub use provider::{GoTestProvider, GO_TEST};

/// Whether a project file belongs to Go (sources, `go.mod`, `go.work`, `go.sum`).
pub fn is_go_path(path: &str) -> bool {
    let name = path.replace('\\', "/");
    let name = name.rsplit('/').next().unwrap_or("");
    name.ends_with(".go") || matches!(name, "go.mod" | "go.work" | "go.sum" | "go.work.sum")
}

/// The modules of a project from its `go.mod` files (`(project-relative path, text)`): the
/// module path of each and its folder, outermost first.
pub fn modules_of<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<ModuleRoot> {
    let mut out: Vec<ModuleRoot> = files
        .into_iter()
        .filter_map(|(path, text)| {
            let path = path.replace('\\', "/");
            let module = parse_go_mod(text).module?.text;
            Some(ModuleRoot {
                path: module,
                dir: env::dir_of(&path).to_string(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.dir.len().cmp(&b.dir.len()).then(a.dir.cmp(&b.dir)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_paths_and_modules() {
        assert!(is_go_path("cmd/main.go") && is_go_path("go.mod") && is_go_path("x/go.work"));
        assert!(!is_go_path("go.mod.bak"));
        let m = modules_of([
            ("tools/go.mod", "module example.com/tools\n"),
            ("go.mod", "module example.com/app\n\ngo 1.23\n"),
            ("broken/go.mod", "go 1.22\n"),
        ]);
        assert_eq!(
            m,
            [
                ModuleRoot {
                    path: "example.com/app".into(),
                    dir: "".into()
                },
                ModuleRoot {
                    path: "example.com/tools".into(),
                    dir: "tools".into()
                },
            ]
        );
    }
}
