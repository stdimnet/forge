//! `~/.ideavimrc` support: `set` options, `map`/`noremap` mappings (including
//! `:action <ActionId>` and `<Action>(Id)` right-hand sides), `let mapleader`, `sethandler`
//! and `if has('ide')` blocks. Everything else is reported and ignored.

use serde::Serialize;

use crate::engine::{Mappings, Vim};
use crate::ex::split_command;
use crate::options::Options;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RcProblem {
    /// 1-based line number.
    pub line: usize,
    pub text: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RcReport {
    /// Commands that were applied.
    pub applied: usize,
    pub problems: Vec<RcProblem>,
}

/// Commands of a vimrc that make no sense in Forge; skipped without a warning.
const QUIET: [&str; 12] = [
    "syntax",
    "filetype",
    "colorscheme",
    "colo",
    "scriptencoding",
    "runtime",
    "highlight",
    "hi",
    "augroup",
    "autocmd",
    "au",
    "redraw",
];

fn eval_condition(expr: &str) -> bool {
    let e = expr.trim();
    if let Some(inner) = e.strip_prefix('!') {
        return !eval_condition(inner);
    }
    let lower = e.to_ascii_lowercase().replace(' ', "");
    if lower.starts_with("has(") {
        let arg = lower
            .trim_start_matches("has(")
            .trim_end_matches(')')
            .trim_matches(['\'', '"'])
            .to_string();
        return arg == "ide";
    }
    matches!(
        lower.as_str(),
        "1" | "true" | "&ide" | "exists('&ide')" | "exists(\"&ide\")"
    )
}

/// Join continuation lines (`\` at the start of the next line); keeps original line numbers.
fn logical_lines(text: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let t = raw.trim_start();
        if let Some(cont) = t.strip_prefix('\\') {
            if let Some(last) = out.last_mut() {
                last.1.push_str(cont);
                continue;
            }
        }
        out.push((i + 1, raw.to_string()));
    }
    out
}

/// Drop a trailing `" comment` from commands that allow it (not from mappings).
fn strip_comment(name: &str, args: &str) -> String {
    if name.starts_with("se") {
        if let Some(i) = args.find(" \"") {
            return args[..i].trim_end().to_string();
        }
        if args.starts_with('"') {
            return String::new();
        }
    }
    args.to_string()
}

/// The file of a `source <file>` / `so <file>` line.
fn source_target(line: &str) -> Option<&str> {
    let t = line.trim().trim_start_matches(':').trim_start();
    let rest = t
        .strip_prefix("source ")
        .or_else(|| t.strip_prefix("so "))?;
    Some(rest.trim()).filter(|r| !r.is_empty())
}

/// Inline the files of `source <file>` lines (e.g. `source ~/.vimrc`) with `read`, up to
/// three levels deep; lines whose file cannot be read stay (and are reported on load).
pub fn expand_sources(text: &str, read: &dyn Fn(&str) -> Option<String>) -> String {
    fn go(text: &str, read: &dyn Fn(&str) -> Option<String>, depth: usize) -> String {
        let mut out = Vec::new();
        for line in text.lines() {
            match source_target(line).filter(|_| depth < 3).and_then(read) {
                Some(inner) => out.push(go(&inner, read, depth + 1)),
                None => out.push(line.to_string()),
            }
        }
        out.join("\n")
    }
    go(text, read, 0)
}

impl Vim {
    /// Replace options, mappings, leader and rc handlers with those of `text`.
    pub fn load_rc(&mut self, text: &str) -> RcReport {
        self.options = Options::default();
        self.mappings = Mappings::default();
        self.leader = "\\".into();
        self.handlers.clear_rc();
        let mut report = RcReport::default();
        // (active, a branch was taken) per open `if`.
        let mut conds: Vec<(bool, bool)> = Vec::new();
        for (no, line) in logical_lines(text) {
            let t = line.trim().trim_start_matches(':').trim();
            if t.is_empty() || t.starts_with('"') {
                continue;
            }
            let (name, bang, args) = split_command(t);
            let outer = conds.iter().all(|c| c.0);
            match name.as_str() {
                "if" => {
                    let v = outer && eval_condition(&args);
                    conds.push((v, v));
                    continue;
                }
                "elseif" | "elsei" => {
                    let parent = conds.len() < 2 || conds[..conds.len() - 1].iter().all(|c| c.0);
                    if let Some(c) = conds.last_mut() {
                        let v = parent && !c.1 && eval_condition(&args);
                        *c = (v, c.1 || v);
                    }
                    continue;
                }
                "else" | "el" => {
                    let parent = conds.len() < 2 || conds[..conds.len() - 1].iter().all(|c| c.0);
                    if let Some(c) = conds.last_mut() {
                        *c = (parent && !c.1, true);
                    }
                    continue;
                }
                "endif" | "en" | "end" => {
                    conds.pop();
                    continue;
                }
                _ => {}
            }
            if !outer {
                continue;
            }
            let args = strip_comment(&name, &args);
            if QUIET.contains(&name.as_str()) {
                continue;
            }
            let problem = |message: String| RcProblem {
                line: no,
                text: t.to_string(),
                message,
            };
            if name == "source" || name == "so" {
                // Readable files were inlined before loading (`expand_sources`).
                report
                    .problems
                    .push(problem(format!("Cannot read {}", args.trim())));
                continue;
            }
            match self.ex_global(&name, bang, &args) {
                Some(Ok(_)) => report.applied += 1,
                Some(Err(e)) => report.problems.push(problem(e)),
                None => report.problems.push(problem(format!(
                    "`{name}` is not supported in ~/.ideavimrc"
                ))),
            }
        }
        // Effects (e.g. from `:action` in an rc file) are not run while loading.
        self.out = Default::default();
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::MapMode;
    use crate::handlers::{Handler, HandlerMode};

    const RC: &str = r#"" My IdeaVim settings
let mapleader = " "
set number relativenumber " line numbers
set ignorecase smartcase hlsearch incsearch
set surround
Plug 'tpope/vim-surround'
nmap <leader>r :action RenameElement<CR>
nnoremap <leader>f <Action>(ReformatCode)
inoremap jk <Esc>
vmap < <gv
map Q gq
sethandler <C-V> n-v:vim i:ide
if has('ide')
  nnoremap <leader>b :action ToggleLineBreakpoint<CR>
else
  nnoremap <leader>b :echo "vim"<CR>
endif
if has('nvim')
  set nonumber
endif
syntax on
colorscheme desert
autocmd BufRead * echo
wincmd l
set bogus
"#;

    #[test]
    fn loads_ideavimrc() {
        let mut v = Vim::new();
        let report = v.load_rc(RC);
        assert_eq!(v.leader, " ");
        assert!(v.options.number && v.options.relativenumber);
        assert!(
            v.options.ignorecase
                && v.options.smartcase
                && v.options.hlsearch
                && v.options.incsearch
        );
        let n = v.mappings.list(MapMode::Normal);
        let r = n.iter().find(|m| m.lhs == vec![" ", "r"]).unwrap();
        assert_eq!(r.rhs.concat(), ":action RenameElement<CR>");
        assert!(!r.noremap);
        let f = n.iter().find(|m| m.lhs == vec![" ", "f"]).unwrap();
        assert_eq!(f.rhs, vec!["<Action>(ReformatCode)"]);
        let b = n.iter().find(|m| m.lhs == vec![" ", "b"]).unwrap();
        assert!(b.text.contains("ToggleLineBreakpoint"));
        assert_eq!(v.mappings.list(MapMode::Insert)[0].lhs, vec!["j", "k"]);
        assert_eq!(v.mappings.list(MapMode::OpPending).len(), 1);
        assert_eq!(
            v.handlers.handler("<C-v>", HandlerMode::Insert),
            Handler::Ide
        );
        assert_eq!(
            v.handlers.handler("<C-v>", HandlerMode::Normal),
            Handler::Vim
        );
        let lines: Vec<usize> = report.problems.iter().map(|p| p.line).collect();
        assert_eq!(lines, vec![24, 25], "{:?}", report.problems);
        assert!(report.problems[1].message.contains("E518"));
    }

    #[test]
    fn inlines_sourced_files() {
        let read = |p: &str| (p == "~/.vimrc").then(|| "set hls\nso ~/.vimrc".to_string());
        assert_eq!(
            expand_sources("set nu\nsource ~/.vimrc\nso /nope", &read),
            "set nu\nset hls\nset hls\nset hls\nso ~/.vimrc\nso /nope"
        );
        let mut v = Vim::new();
        let report = v.load_rc("so /nope");
        assert_eq!(report.problems[0].message, "Cannot read /nope");
    }

    #[test]
    fn reloading_resets_previous_settings() {
        let mut v = Vim::new();
        v.load_rc("set hlsearch\nnmap x dd");
        v.load_rc("");
        assert!(!v.options.hlsearch);
        assert!(v.mappings.list(MapMode::Normal).is_empty());
    }
}
