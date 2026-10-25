//! The "Python" run configuration (script or module, parameters, interpreter options) and
//! the gutter run markers of runnable files.

use serde::{Deserialize, Serialize};

/// What a Python configuration runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Target {
    #[default]
    Script,
    Module,
}

impl Target {
    pub fn parse(s: &str) -> Self {
        if s.trim() == "module" {
            Target::Module
        } else {
            Target::Script
        }
    }
}

/// The arguments after the interpreter: `[options…] (<script> | -m <module>) [parameters…]`.
pub fn arguments(
    target: Target,
    name: &str,
    interpreter_options: &[String],
    parameters: &[String],
) -> Result<Vec<String>, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(match target {
            Target::Script => "Script path is not specified".into(),
            Target::Module => "Module name is not specified".into(),
        });
    }
    if target == Target::Module && !is_module_name(name) {
        return Err(format!("'{name}' is not a module name"));
    }
    let mut args: Vec<String> = interpreter_options.to_vec();
    if target == Target::Module {
        args.push("-m".into());
    }
    args.push(name.to_string());
    args.extend(parameters.iter().cloned());
    Ok(args)
}

/// `package.module` (identifiers separated by dots).
pub fn is_module_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|part| {
            let mut chars = part.chars();
            chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
                && chars.all(|c| c.is_alphanumeric() || c == '_')
        })
}

/// A runnable line: `if __name__ == "__main__":` (0-based).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunMarker {
    pub line: u32,
}

/// The `__main__` guards of a file.
pub fn run_markers(text: &str) -> Vec<RunMarker> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| {
            let t: String = l.chars().filter(|c| !c.is_whitespace()).collect();
            t.starts_with("if__name__==\"__main__\"")
                || t.starts_with("if__name__=='__main__'")
                || t.starts_with("if\"__main__\"==__name__")
                || t.starts_with("if'__main__'==__name__")
        })
        .map(|(i, _)| RunMarker { line: i as u32 })
        .collect()
}

/// The module name of a project file for `-m` (`src/shop/cli.py` → `shop.cli`), with
/// `src/` treated as a source root; `None` for files outside packages.
pub fn module_of(path: &str) -> Option<String> {
    let p = path.replace('\\', "/");
    let p = p.strip_prefix("src/").unwrap_or(&p);
    let p = p.strip_suffix(".py")?;
    let p = p.strip_suffix("/__main__").unwrap_or(p);
    let p = p.strip_suffix("/__init__").unwrap_or(p);
    let name = p.replace('/', ".");
    is_module_name(&name).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_arguments() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            arguments(
                Target::Script,
                "app/main.py",
                &s(&["-X", "dev"]),
                &s(&["--port", "8000"])
            )
            .unwrap(),
            s(&["-X", "dev", "app/main.py", "--port", "8000"])
        );
        assert_eq!(
            arguments(Target::Module, "uvicorn", &[], &s(&["app:api"])).unwrap(),
            s(&["-m", "uvicorn", "app:api"])
        );
        assert!(arguments(Target::Script, " ", &[], &[]).is_err());
        assert!(arguments(Target::Module, "my-module", &[], &[]).is_err());
        assert_eq!(Target::parse("module"), Target::Module);
    }

    #[test]
    fn finds_main_guards_and_modules() {
        let src = "def main():\n    pass\n\nif __name__ == \"__main__\":\n    main()\nif '__main__' == __name__: main()\n";
        assert_eq!(
            run_markers(src),
            vec![RunMarker { line: 3 }, RunMarker { line: 5 }]
        );
        assert_eq!(module_of("src/shop/cli.py").as_deref(), Some("shop.cli"));
        assert_eq!(module_of("shop/__main__.py").as_deref(), Some("shop"));
        assert_eq!(module_of("scripts/my-tool.py"), None);
        assert_eq!(module_of("README.md"), None);
    }
}
