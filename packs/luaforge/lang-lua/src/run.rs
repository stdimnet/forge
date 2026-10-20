//! Gutter run icons (▶) for main chunks: scripts that do not return a module table, and the
//! `main.lua` of a LÖVE game.

use serde::Serialize;

use crate::model::*;
use crate::project::{ProjectIndex, Table};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunKind {
    /// `lua file.lua`.
    Script,
    /// `love <dir>`.
    Love,
}

/// A run icon and the configuration it starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunMarker {
    /// 0-based line.
    pub line: usize,
    pub kind: RunKind,
    /// Configuration name.
    pub name: String,
    /// The file (or the game directory for LÖVE) passed to the interpreter.
    pub file: String,
    /// Interpreter override (`love`); `None` uses the configured Lua interpreter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpreter: Option<String>,
}

fn dir_of(path: &str) -> &str {
    path.rfind('/').map(|i| &path[..i]).unwrap_or("")
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Run icons of an indexed file.
pub fn run_markers(index: &ProjectIndex, path: &str) -> Vec<RunMarker> {
    let Some(file) = index.file(path) else {
        return Vec::new();
    };
    let Some(first) = file.first_statement else {
        return Vec::new();
    };
    let text = index.text(path).unwrap_or("");
    let line = text[..first.min(text.len())].matches('\n').count();
    let name = file_name(path);
    let dir = dir_of(path);
    let love_callbacks = file.decls.iter().any(|d| {
        d.kind == DeclKind::FieldFunction
            && d.container == "love"
            && matches!(d.name.as_str(), "load" | "update" | "draw")
    });
    let conf = if dir.is_empty() {
        "conf.lua".to_string()
    } else {
        format!("{dir}/conf.lua")
    };
    if name == "main.lua" && (love_callbacks || index.file(&conf).is_some()) {
        let game = if dir.is_empty() { "." } else { dir };
        let title = if dir.is_empty() {
            index.project_name().to_string()
        } else {
            file_name(dir).to_string()
        };
        return vec![RunMarker {
            line,
            kind: RunKind::Love,
            name: title,
            file: game.to_string(),
            interpreter: Some("love".into()),
        }];
    }
    // Modules return a table or a function; anything else is a main chunk.
    let is_module = match &file.returns {
        None => false,
        Some(Value::Function | Value::Table(_)) => true,
        Some(v) => {
            index
                .resolve_value(path, v, 0)
                .is_some_and(|t| !matches!(t, Table::Global(_)))
                || matches!(v, Value::Local(i) if file.decls.get(*i).is_some_and(|d| d.value == Value::Function))
        }
    };
    if is_module {
        return Vec::new();
    }
    vec![RunMarker {
        line,
        kind: RunKind::Script,
        name: name.trim_end_matches(".lua").to_string(),
        file: path.to_string(),
        interpreter: None,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn markers(files: &[(&str, &str)], path: &str) -> Vec<RunMarker> {
        let mut p = ProjectIndex::new(&[]);
        p.set_name("pixel-dungeon");
        for (f, t) in files {
            p.set_file(f, t);
        }
        run_markers(&p, path)
    }

    #[test]
    fn scripts_get_a_marker_modules_do_not() {
        let m = markers(
            &[("tools/build.lua", "-- build\n\nprint('hi')\n")],
            "tools/build.lua",
        );
        assert_eq!(m.len(), 1);
        assert_eq!(
            (m[0].line, m[0].kind, m[0].name.as_str()),
            (2, RunKind::Script, "build")
        );
        assert_eq!(m[0].file, "tools/build.lua");
        assert!(markers(&[("m.lua", "local M = {}\nreturn M\n")], "m.lua").is_empty());
        assert!(markers(&[("f.lua", "return function() end\n")], "f.lua").is_empty());
        assert_eq!(markers(&[("x.lua", "return 42\n")], "x.lua").len(), 1);
    }

    #[test]
    fn love_games_run_their_directory() {
        let m = markers(&[("main.lua", "function love.draw() end\n")], "main.lua");
        assert_eq!(m[0].kind, RunKind::Love);
        assert_eq!(
            (m[0].file.as_str(), m[0].name.as_str()),
            (".", "pixel-dungeon")
        );
        assert_eq!(m[0].interpreter.as_deref(), Some("love"));
        let m = markers(
            &[("game/main.lua", "print(1)\n"), ("game/conf.lua", "")],
            "game/main.lua",
        );
        assert_eq!((m[0].kind, m[0].file.as_str()), (RunKind::Love, "game"));
    }
}
