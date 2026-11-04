//! Vim and Emacs modelines on the first line of a file, as PHPStorm honours them:
//! `# vim: set ft=php:`, `// vi: filetype=javascript`, `# -*- mode: python -*-` or
//! `;; -*- lisp -*-`. Used for files whose name and `#!` line select no type.

use crate::FileTypeSpec;

/// Editor mode names that differ from Forge's file type ids, extensions and interpreters.
const ALIASES: &[(&str, &str)] = &[
    ("sh", "shell"),
    ("shell-script", "shell"),
    ("bash", "shell"),
    ("zsh", "shell"),
    ("js", "javascript"),
    ("js2", "javascript"),
    ("javascriptreact", "jsx"),
    ("typescriptreact", "tsx"),
    ("c++", "cpp"),
    ("cperl", "perl"),
    ("ps1", "powershell"),
    ("make", "makefile"),
    ("conf", "properties"),
    ("dosini", "properties"),
    ("ini", "properties"),
    ("yml", "yaml"),
    ("md", "markdown"),
    ("gfm", "markdown"),
    ("tex", "latex"),
    ("emacs-lisp", "lisp"),
    ("elisp", "lisp"),
    ("cs", "csharp"),
    ("objective-c", "objc"),
    ("htmldjango", "django"),
    ("eruby", "erb"),
    ("jinja2", "jinja"),
    ("text", "text"),
];

/// The mode a modeline on `line` names, lower case: `vim: set ft=php:` → `php`.
pub fn mode_of(line: &str) -> Option<String> {
    emacs_mode(line).or_else(|| vim_mode(line))
}

/// `-*- mode: python; coding: utf-8 -*-` or the short form `-*- python -*-`.
fn emacs_mode(line: &str) -> Option<String> {
    let start = line.find("-*-")? + 3;
    let end = start + line[start..].find("-*-")?;
    let body = line[start..end].trim();
    if !body.contains(':') {
        return clean(body);
    }
    body.split(';').find_map(|part| {
        let (key, value) = part.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case("mode")
            .then(|| clean(value))?
    })
}

/// `vim: set ft=php:`, `vi: filetype=php`, `ex: syntax=php` (after a space or at the start).
fn vim_mode(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    let at = ["vim:", "vi:", "ex:"].iter().find_map(|tag| {
        lower
            .match_indices(tag)
            .map(|(i, _)| i)
            .find(|&i| {
                i == 0
                    || lower[..i].ends_with(|c: char| c.is_whitespace() || "#/;*\"-!%".contains(c))
            })
            .map(|i| i + tag.len())
    })?;
    let body = &lower[at..];
    body.split([' ', ':', '\t'])
        .filter_map(|w| w.split_once('='))
        .find(|(key, _)| matches!(*key, "ft" | "filetype" | "syntax" | "syn"))
        .and_then(|(_, value)| clean(value))
}

fn clean(value: &str) -> Option<String> {
    let v = value.trim().trim_end_matches("-mode").to_lowercase();
    // `ft=php.html` (Vim's compound types): the first one.
    let v = v.split('.').next().unwrap_or_default().to_string();
    (!v.is_empty() && v.chars().all(|c| c.is_alphanumeric() || "+-_#".contains(c))).then_some(v)
}

/// The active type a mode name selects: by alias, id, name, extension, then interpreter.
pub fn resolve<'a>(
    mode: &str,
    active: &[&'a FileTypeSpec],
    has_interpreter: impl Fn(&FileTypeSpec, &str) -> bool,
) -> Option<&'a FileTypeSpec> {
    let id = ALIASES
        .iter()
        .find(|(alias, _)| *alias == mode)
        .map_or(mode, |(_, id)| id);
    let newest =
        |pred: &dyn Fn(&FileTypeSpec) -> bool| active.iter().rev().find(|t| pred(t)).copied();
    newest(&|t| t.id == id)
        .or_else(|| newest(&|t| t.name.eq_ignore_ascii_case(mode)))
        .or_else(|| newest(&|t| t.extensions.iter().any(|e| e.eq_ignore_ascii_case(mode))))
        .or_else(|| newest(&|t| has_interpreter(t, mode)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_vim_and_emacs_modelines() {
        for (line, mode) in [
            ("# vim: set ft=php:", Some("php")),
            ("// vi: filetype=javascript", Some("javascript")),
            ("/* vim: set syntax=yaml ts=2: */", Some("yaml")),
            ("# -*- mode: python; coding: utf-8 -*-", Some("python")),
            (";; -*- Lisp -*-", Some("lisp")),
            ("# -*- mode: shell-script -*-", Some("shell-script")),
            ("# -*- coding: utf-8 -*-", None),
            ("export VIM=1", None),
            ("const evim: x = 'ft=php'", None),
            ("plain text", None),
        ] {
            assert_eq!(mode_of(line).as_deref(), mode, "{line}");
        }
    }
}
