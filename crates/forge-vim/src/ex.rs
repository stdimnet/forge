//! Ex commands (`:w`, `:q`, `:e`, `:s`, `:g`, `:normal`, `:set`, `:map`, `:action`, …).

use regex::Captures;

use crate::engine::{BufState, Effect, MapMode, Mapping, Vim};
use crate::keys::parse_keys;
use crate::pattern;
use crate::registers::{RegKind, Register};

/// Split `name[!] args` (no range).
pub(crate) fn split_command(text: &str) -> (String, bool, String) {
    let c: Vec<char> = text.chars().collect();
    let mut i = 0;
    if let Some(&ch) = c.first() {
        if "&<>=!~".contains(ch) {
            i = 1;
            while (ch == '<' || ch == '>' || ch == '&') && c.get(i) == Some(&ch) {
                i += 1;
            }
        } else {
            while i < c.len() && c[i].is_ascii_alphabetic() {
                i += 1;
            }
        }
    }
    let name: String = c[..i].iter().collect();
    let bang = c.get(i) == Some(&'!') && !name.is_empty() && name != "!";
    if bang {
        i += 1;
    }
    let args: String = c[i..].iter().collect();
    (name, bang, args.trim().to_string())
}

/// Whether `name` abbreviates `full` (at least `min` characters).
fn is(name: &str, full: &str, min: usize) -> bool {
    name.len() >= min && full.starts_with(name)
}

/// The string value of a `let` expression ('x', "x", "\<Space>").
fn string_value(v: &str) -> Option<String> {
    let v = v.trim();
    if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        return Some(v[1..v.len() - 1].replace("''", "'"));
    }
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        let inner = &v[1..v.len() - 1];
        let mut out = String::new();
        let mut it = inner.chars().peekable();
        while let Some(c) = it.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match it.next() {
                Some('<') => {
                    let name: String = it.by_ref().take_while(|c| *c != '>').collect();
                    let keys = parse_keys(&format!("<{name}>"), "\\");
                    out.push_str(&keys.concat());
                }
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some(o) => out.push(o),
                None => out.push('\\'),
            }
        }
        return Some(out);
    }
    None
}

impl Vim {
    pub(crate) fn ex(&mut self, st: &mut BufState, text: &str) {
        if let Err(e) = self.ex_inner(st, text) {
            self.error(e);
        }
    }

    fn address(
        &mut self,
        st: &BufState,
        c: &[char],
        i: &mut usize,
    ) -> Result<Option<usize>, String> {
        let b = &st.buf;
        let cur = st.line();
        let last = b.line_count() - 1;
        let mut base: Option<usize> = None;
        match c.get(*i) {
            Some(d) if d.is_ascii_digit() => {
                let mut n = 0usize;
                while let Some(d) = c.get(*i).and_then(|d| d.to_digit(10)) {
                    n = n.saturating_mul(10).saturating_add(d as usize);
                    *i += 1;
                }
                base = Some(n.saturating_sub(1).min(last));
            }
            Some('.') => {
                *i += 1;
                base = Some(cur);
            }
            Some('$') => {
                *i += 1;
                base = Some(last);
            }
            Some('\'') => {
                let m = *c.get(*i + 1).ok_or("E20: Mark not set")?;
                *i += 2;
                let p = *st.marks.get(&m).ok_or("E20: Mark not set")?;
                base = Some(b.line_of(p));
            }
            Some(&d @ ('/' | '?')) => {
                *i += 1;
                let mut pat = String::new();
                while let Some(&ch) = c.get(*i) {
                    *i += 1;
                    if ch == d {
                        break;
                    }
                    if ch == '\\' && c.get(*i) == Some(&d) {
                        pat.push(d);
                        *i += 1;
                        continue;
                    }
                    pat.push(ch);
                }
                let from = if d == '/' {
                    b.line_end(cur)
                } else {
                    b.line_start(cur)
                };
                let p = self
                    .search(st, &pat, d == '/', from, 1)
                    .ok_or("E486: Pattern not found")?;
                base = Some(b.line_of(p));
            }
            _ => {}
        }
        while let Some(&sign) = c.get(*i).filter(|c| **c == '+' || **c == '-') {
            *i += 1;
            let mut n = 0usize;
            let mut digits = false;
            while let Some(d) = c.get(*i).and_then(|d| d.to_digit(10)) {
                n = n * 10 + d as usize;
                digits = true;
                *i += 1;
            }
            if !digits {
                n = 1;
            }
            let from = base.unwrap_or(cur);
            base = Some(if sign == '+' {
                (from + n).min(last)
            } else {
                from.saturating_sub(n)
            });
        }
        Ok(base)
    }

    fn parse_range(
        &mut self,
        st: &BufState,
        c: &[char],
        i: &mut usize,
    ) -> Result<Option<(usize, usize)>, String> {
        while c.get(*i) == Some(&' ') {
            *i += 1;
        }
        if c.get(*i) == Some(&'%') {
            *i += 1;
            return Ok(Some((0, st.buf.line_count() - 1)));
        }
        let Some(a) = self.address(st, c, i)? else {
            if matches!(c.get(*i), Some(',' | ';')) {
                *i += 1;
                let b = self.address(st, c, i)?.unwrap_or(st.line());
                return Ok(Some((st.line(), b)));
            }
            return Ok(None);
        };
        if matches!(c.get(*i), Some(',' | ';')) {
            *i += 1;
            let b = self.address(st, c, i)?.unwrap_or(st.line());
            let (x, y) = if a <= b { (a, b) } else { (b, a) };
            return Ok(Some((x, y)));
        }
        Ok(Some((a, a)))
    }

    fn ex_inner(&mut self, st: &mut BufState, text: &str) -> Result<(), String> {
        let text = text.trim_start_matches([':', ' ', '\t']);
        if text.is_empty() || text.starts_with('"') {
            return Ok(());
        }
        let c: Vec<char> = text.chars().collect();
        let mut i = 0;
        let range = self.parse_range(st, &c, &mut i)?;
        let rest: String = c[i..].iter().collect();
        let (name, bang, args) = split_command(rest.trim_start());
        let last = st.buf.line_count() - 1;
        let cur = st.line();
        let (l1, l2) = range
            .map(|(a, b)| (a.min(last), b.min(last)))
            .unwrap_or((cur, cur));
        if name.is_empty() {
            if !rest.trim().is_empty() {
                return Err(format!("E492: Not an editor command: {text}"));
            }
            if range.is_some() {
                st.marks.insert('\'', st.cursor);
                st.cursor = st.buf.first_non_blank(l2);
            }
            return Ok(());
        }
        if let Some(r) = self.ex_global(&name, bang, &args) {
            if let Some(msg) = r? {
                self.message(msg);
            }
            return Ok(());
        }
        let n = name.as_str();
        let action = |s: &mut Self, id: &str| s.effect(Effect::Action { id: id.into() });
        if is(n, "write", 1) || is(n, "update", 2) || is(n, "wall", 2) {
            action(self, "SaveAll");
        } else if is(n, "quit", 1) || is(n, "close", 3) {
            action(self, "CloseContent");
        } else if is(n, "qall", 2) || is(n, "quitall", 5) {
            action(self, "CloseAllEditors");
        } else if n == "wq" || is(n, "xit", 1) || is(n, "exit", 3) {
            action(self, "SaveAll");
            action(self, "CloseContent");
        } else if is(n, "wqall", 3) || is(n, "xall", 2) {
            action(self, "SaveAll");
            action(self, "CloseAllEditors");
        } else if is(n, "edit", 1) {
            if args.is_empty() {
                if bang {
                    action(self, "Synchronize");
                }
            } else {
                self.effect(Effect::Open { path: args.clone() });
            }
        } else if is(n, "substitute", 1) || n == "&" || n == "&&" {
            self.substitute(st, l1, l2, &args, n == "&&")?;
        } else if is(n, "delete", 1) || is(n, "yank", 1) {
            let (reg, count) = reg_and_count(&args);
            let (a, b) = match count {
                Some(k) => (l2, (l2 + k - 1).min(last)),
                None => (l1, l2),
            };
            let mut t = st.buf.slice(st.buf.line_start(a), st.buf.line_end(b));
            t.push('\n');
            let yank = n.starts_with('y');
            let clip = self.options.clipboard_unnamed();
            self.registers
                .store(reg, Register::new(t, RegKind::Line), yank, clip);
            if !yank {
                self.delete_lines(st, a, b);
            }
        } else if is(n, "join", 1) {
            let (_, count) = reg_and_count(&args);
            let (a, b) = match count {
                Some(k) => (l2, l2 + k - 1),
                None if l1 == l2 => (l1, l1 + 1),
                None => (l1, l2),
            };
            self.join_lines(st, a, b.saturating_sub(a).max(1), !bang);
        } else if n.starts_with('>') || n.starts_with('<') {
            let times = n.len();
            self.shift_lines(st, l1, l2, n.starts_with('>'), times);
            st.cursor = st.buf.first_non_blank(l2);
        } else if is(n, "normal", 4) {
            let keys = parse_keys(&args, &self.leader.clone());
            if range.is_some() {
                st.tracked = (l1..=l2).map(|l| Some(st.buf.line_start(l))).collect();
                for k in 0..st.tracked.len() {
                    if let Some(p) = st.tracked[k] {
                        st.cursor = p;
                        self.run_normal(st, &keys, !bang);
                    }
                }
                st.tracked.clear();
            } else {
                self.run_normal(st, &keys, !bang);
            }
        } else if is(n, "move", 1) || is(n, "copy", 2) || n == "t" {
            self.move_copy(st, l1, l2, &args, n.starts_with('m'))?;
        } else if is(n, "global", 1) || is(n, "vglobal", 1) {
            let (a, b) = range
                .map(|(a, b)| (a.min(last), b.min(last)))
                .unwrap_or((0, last));
            self.global(st, a, b, &args, n.starts_with('v') || bang)?;
        } else if is(n, "registers", 3) || is(n, "display", 2) {
            let mut lines = vec!["Type Name Content".to_string()];
            for (r, t) in self.registers_listing() {
                let shown: String = t.replace('\n', "^J").chars().take(60).collect();
                lines.push(format!("  c  \"{r}   {shown}"));
            }
            self.message(lines.join("\n"));
        } else if n == "marks" {
            let mut marks: Vec<(char, usize)> = st.marks.iter().map(|(k, v)| (*k, *v)).collect();
            marks.sort();
            let mut lines = vec!["mark line  col text".to_string()];
            for (m, p) in marks {
                let l = st.buf.line_of(p);
                let text: String = st.buf.line_text(l).chars().take(50).collect();
                lines.push(format!(" {m} {:>6} {:>4} {text}", l + 1, st.buf.col_of(p)));
            }
            self.message(lines.join("\n"));
        } else if is(n, "bnext", 2) || is(n, "tabnext", 4) {
            action(self, "NextTab");
        } else if is(n, "bprevious", 2)
            || is(n, "bNext", 2)
            || is(n, "tabprevious", 4)
            || is(n, "tabNext", 4)
        {
            action(self, "PreviousTab");
        } else if is(n, "split", 2) {
            action(self, "SplitHorizontally");
        } else if is(n, "vsplit", 2) {
            action(self, "SplitVertically");
        } else if is(n, "only", 2) {
            action(self, "UnsplitAll");
        } else if is(n, "sort", 3) {
            let (a, b) = range
                .map(|(a, b)| (a.min(last), b.min(last)))
                .unwrap_or((0, last));
            self.sort(st, a, b, &args, bang);
        } else if n == "=" {
            self.message((l2 + 1).to_string());
        } else if is(n, "help", 1) {
            self.message("See Help | Forge Guide | Vim emulation");
        } else {
            return Err(format!("E492: Not an editor command: {text}"));
        }
        Ok(())
    }

    /// Commands that need no editor (also allowed in `~/.ideavimrc`). `None`: not one of them.
    pub(crate) fn ex_global(
        &mut self,
        name: &str,
        bang: bool,
        args: &str,
    ) -> Option<Result<Option<String>, String>> {
        let n = name;
        if is(n, "set", 2) || is(n, "setlocal", 4) || is(n, "setglobal", 4) {
            if args.is_empty() {
                return Some(Ok(None));
            }
            return Some(self.options.apply_set(args));
        }
        if is(n, "nohlsearch", 3) {
            self.hl = false;
            return Some(Ok(None));
        }
        if n == "action" {
            if args.is_empty() {
                return Some(Err("E471: Argument required".into()));
            }
            self.effect(Effect::Action {
                id: args.to_string(),
            });
            return Some(Ok(None));
        }
        if n == "actionlist" {
            return Some(Ok(Some(
                "Use Find Action (Ctrl+Shift+A) to look up action ids".into(),
            )));
        }
        if is(n, "source", 2) {
            self.effect(Effect::ReloadRc);
            return Some(Ok(None));
        }
        if n == "let" {
            return Some(self.let_command(args));
        }
        if n == "sethandler" {
            return Some(self.handlers.sethandler(args).map(|_| None));
        }
        if is(n, "echo", 2) {
            return Some(Ok(Some(
                string_value(args).unwrap_or_else(|| args.to_string()),
            )));
        }
        if n == "Plug" || n == "plug" || n == "packadd" {
            return Some(Ok(None));
        }
        self.map_command(n, bang, args)
    }

    fn let_command(&mut self, args: &str) -> Result<Option<String>, String> {
        let (name, value) = args.split_once('=').ok_or("E15: Invalid expression")?;
        let name = name.trim().trim_start_matches("g:");
        let value = string_value(value)
            .ok_or_else(|| format!("E15: Invalid expression: {}", value.trim()))?;
        if name == "mapleader" {
            self.leader = value;
        }
        Ok(None)
    }

    fn map_command(
        &mut self,
        name: &str,
        bang: bool,
        args: &str,
    ) -> Option<Result<Option<String>, String>> {
        use MapMode::*;
        let nvo = vec![Normal, Visual, OpPending];
        let ic = vec![Insert, CmdLine];
        let all = if bang { ic.clone() } else { nvo.clone() };
        // (modes, noremap, kind: 0 map, 1 unmap, 2 mapclear)
        let (modes, noremap, kind): (Vec<MapMode>, bool, u8) = match name {
            "map" => (all, false, 0),
            "nm" | "nmap" => (vec![Normal], false, 0),
            "vm" | "vmap" | "xm" | "xmap" | "smap" => (vec![Visual], false, 0),
            "om" | "omap" => (vec![OpPending], false, 0),
            "im" | "imap" => (vec![Insert], false, 0),
            "cm" | "cmap" => (vec![CmdLine], false, 0),
            "no" | "nor" | "nore" | "norem" | "noremap" => (all, true, 0),
            "nn" | "nno" | "nnor" | "nnore" | "nnoremap" => (vec![Normal], true, 0),
            "vn" | "vno" | "vnoremap" | "xn" | "xno" | "xnoremap" | "snor" | "snoremap" => {
                (vec![Visual], true, 0)
            }
            "ono" | "onoremap" => (vec![OpPending], true, 0),
            "ino" | "inor" | "inoremap" => (vec![Insert], true, 0),
            "cno" | "cnor" | "cnoremap" => (vec![CmdLine], true, 0),
            "unm" | "unmap" => (all, false, 1),
            "nun" | "nunmap" => (vec![Normal], false, 1),
            "vu" | "vunmap" | "xu" | "xunmap" | "sunm" | "sunmap" => (vec![Visual], false, 1),
            "ou" | "ounmap" => (vec![OpPending], false, 1),
            "iu" | "iunmap" => (vec![Insert], false, 1),
            "cu" | "cunmap" => (vec![CmdLine], false, 1),
            "mapc" | "mapclear" => (all, false, 2),
            "nmapc" | "nmapclear" => (vec![Normal], false, 2),
            "vmapc" | "vmapclear" | "xmapc" | "xmapclear" => (vec![Visual], false, 2),
            "omapc" | "omapclear" => (vec![OpPending], false, 2),
            "imapc" | "imapclear" => (vec![Insert], false, 2),
            "cmapc" | "cmapclear" => (vec![CmdLine], false, 2),
            _ => return None,
        };
        if kind == 2 {
            for m in modes {
                self.mappings.clear(m);
            }
            return Some(Ok(None));
        }
        let mut rest = args.trim_start();
        loop {
            let lower = rest.to_ascii_lowercase();
            let special = [
                "<silent>",
                "<buffer>",
                "<nowait>",
                "<unique>",
                "<expr>",
                "<script>",
                "<special>",
            ]
            .iter()
            .find(|s| lower.starts_with(**s));
            match special {
                Some(s) => rest = rest[s.len()..].trim_start(),
                None => break,
            }
        }
        let (lhs, rhs) = match rest.find(char::is_whitespace) {
            Some(i) => (&rest[..i], rest[i..].trim_start()),
            None => (rest, ""),
        };
        let leader = self.leader.clone();
        if kind == 1 {
            if lhs.is_empty() {
                return Some(Err("E474: Invalid argument".into()));
            }
            let keys = parse_keys(lhs, &leader);
            let mut found = false;
            for m in modes {
                found |= self.mappings.remove(m, &keys);
            }
            return Some(if found {
                Ok(None)
            } else {
                Err("E31: No such mapping".into())
            });
        }
        if rhs.is_empty() {
            // List the mappings (starting with lhs).
            let prefix = parse_keys(lhs, &leader);
            let mut lines = Vec::new();
            for m in &modes {
                let tag = match m {
                    Normal => "n",
                    Visual => "x",
                    OpPending => "o",
                    Insert => "i",
                    CmdLine => "c",
                };
                for map in self.mappings.list(*m) {
                    if map.lhs.starts_with(&prefix) {
                        lines.push(format!(
                            "{tag}  {:<12} {}{}",
                            map.lhs.concat(),
                            if map.noremap { "* " } else { "  " },
                            map.text
                        ));
                    }
                }
            }
            return Some(Ok(Some(if lines.is_empty() {
                "No mapping found".into()
            } else {
                lines.join("\n")
            })));
        }
        let lhs_keys = parse_keys(lhs, &leader);
        if lhs_keys.is_empty() {
            return Some(Err("E474: Invalid argument".into()));
        }
        let rhs_keys = parse_keys(rhs, &leader);
        for m in modes {
            self.mappings.add(
                m,
                Mapping {
                    lhs: lhs_keys.clone(),
                    rhs: rhs_keys.clone(),
                    noremap,
                    text: rhs.to_string(),
                },
            );
        }
        Some(Ok(None))
    }

    fn substitute(
        &mut self,
        st: &mut BufState,
        l1: usize,
        l2: usize,
        args: &str,
        keep_flags: bool,
    ) -> Result<(), String> {
        let first = args.chars().next();
        let (pattern, rep, flags) = match first {
            Some(d)
                if !d.is_alphanumeric()
                    && !d.is_whitespace()
                    && d != '"'
                    && d != '|'
                    && d != '&' =>
            {
                let c: Vec<char> = args.chars().collect();
                let mut i = 1;
                let mut part = |stop: char| {
                    let mut out = String::new();
                    while i < c.len() {
                        let ch = c[i];
                        i += 1;
                        if ch == stop {
                            return (out, true);
                        }
                        if ch == '\\' && i < c.len() {
                            let nx = c[i];
                            i += 1;
                            if nx == stop && stop != '/' {
                                out.push(nx);
                            } else {
                                out.push('\\');
                                out.push(nx);
                            }
                            continue;
                        }
                        out.push(ch);
                    }
                    (out, false)
                };
                let (pat, closed) = part(d);
                let (rep, _) = if closed {
                    part(d)
                } else {
                    (String::new(), false)
                };
                let rep = rep.replace("\\/", "/");
                let flags: String = c[i.min(c.len())..].iter().collect();
                let pat = if pat.is_empty() {
                    self.last_search
                        .as_ref()
                        .map(|s| s.0.clone())
                        .ok_or("E35: No previous regular expression")?
                } else {
                    pat
                };
                (pat, rep, flags.trim().to_string())
            }
            _ => {
                let (p, r, f) = self
                    .last_sub
                    .clone()
                    .ok_or("E35: No previous regular expression")?;
                let flags = format!(
                    "{}{}",
                    if keep_flags { f } else { String::new() },
                    args.trim().trim_start_matches('&')
                );
                (p, r, flags)
            }
        };
        self.last_sub = Some((pattern.clone(), rep.clone(), flags.clone()));
        self.last_search = Some((pattern.clone(), true));
        self.hl = true;
        let mut pat = pattern::translate(&pattern, self.options.ignorecase, self.options.smartcase);
        if flags.contains('i') {
            pat.ignore_case = true;
        }
        if flags.contains('I') {
            pat.ignore_case = false;
        }
        let re = pat.regex()?;
        let global = flags.contains('g');
        let count_only = flags.contains('n');
        let mut total = 0;
        let mut lines = 0;
        let mut last_line = None;
        for l in (l1..=l2).rev() {
            let line = st.buf.line_text(l);
            let mut k = 0;
            let new = re.replacen(&line, if global { 0 } else { 1 }, |caps: &Captures| {
                k += 1;
                pattern::expand_replacement(&rep, caps)
            });
            if k == 0 {
                continue;
            }
            total += k;
            lines += 1;
            last_line.get_or_insert(l);
            if !count_only && new != line {
                let (s, e) = (st.buf.line_start(l), st.buf.line_end(l));
                self.edit(st, s, e, &new);
            }
        }
        let Some(ll) = last_line else {
            if flags.contains('e') {
                return Ok(());
            }
            return Err(format!("E486: Pattern not found: {pattern}"));
        };
        if count_only {
            self.message(format!(
                "{total} match{} on {lines} line{}",
                if total == 1 { "" } else { "es" },
                if lines == 1 { "" } else { "s" }
            ));
            return Ok(());
        }
        st.marks.insert('\'', st.cursor);
        let target = ll.min(st.buf.line_count() - 1);
        st.cursor = st.buf.first_non_blank(target);
        if lines > 2 {
            self.message(format!("{total} substitutions on {lines} lines"));
        }
        Ok(())
    }

    fn move_copy(
        &mut self,
        st: &mut BufState,
        l1: usize,
        l2: usize,
        args: &str,
        mv: bool,
    ) -> Result<(), String> {
        let c: Vec<char> = args.trim().chars().collect();
        // Target: insert after line `t` (`None`: before the first line).
        let target = if c.iter().collect::<String>() == "0" {
            None
        } else {
            let mut i = 0;
            Some(
                self.address(st, &c, &mut i)?
                    .ok_or("E14: Invalid address")?,
            )
        };
        let b = &st.buf;
        let lines: Vec<String> = (0..b.line_count()).map(|l| b.line_text(l)).collect();
        let block: Vec<String> = lines[l1..=l2].to_vec();
        if mv && target.is_some_and(|t| t >= l1 && t <= l2) {
            return Err("E134: Cannot move a range of lines into itself".into());
        }
        let mut out: Vec<String> = Vec::with_capacity(lines.len() + block.len());
        let insert_at = target.map(|t| t + 1).unwrap_or(0);
        for (idx, line) in lines.iter().enumerate() {
            if idx == insert_at {
                out.extend(block.iter().cloned());
            }
            if mv && idx >= l1 && idx <= l2 {
                continue;
            }
            out.push(line.clone());
        }
        if insert_at >= lines.len() {
            out.extend(block.iter().cloned());
        }
        let new = out.join("\n");
        let old_len = st.buf.len();
        if new != st.buf.text() {
            self.edit(st, 0, old_len, &new);
        }
        // The caret goes to the last moved or copied line.
        let moved_before = if mv && insert_at > l2 { l2 - l1 + 1 } else { 0 };
        let last_line = insert_at + block.len() - 1 - moved_before;
        st.cursor = st
            .buf
            .first_non_blank(last_line.min(st.buf.line_count() - 1));
        Ok(())
    }

    fn global(
        &mut self,
        st: &mut BufState,
        l1: usize,
        l2: usize,
        args: &str,
        invert: bool,
    ) -> Result<(), String> {
        let mut chars = args.chars();
        let d = chars.next().ok_or("E476: Invalid command")?;
        let rest: String = chars.collect();
        let (pat, cmd) = match rest.find(d) {
            Some(i) => (rest[..i].to_string(), rest[i + d.len_utf8()..].to_string()),
            None => (rest.clone(), String::new()),
        };
        let pat = if pat.is_empty() {
            self.last_search
                .as_ref()
                .map(|s| s.0.clone())
                .ok_or("E35: No previous regular expression")?
        } else {
            pat
        };
        let re =
            pattern::translate(&pat, self.options.ignorecase, self.options.smartcase).regex()?;
        let cmd = if cmd.trim().is_empty() {
            "p".to_string()
        } else {
            cmd
        };
        let matching: Vec<usize> = (l1..=l2)
            .filter(|l| re.is_match(&st.buf.line_text(*l)) != invert)
            .collect();
        if matching.is_empty() {
            return Err(format!("E486: Pattern not found: {pat}"));
        }
        if cmd.trim() == "p" || cmd.trim() == "print" {
            let lines: Vec<String> = matching
                .iter()
                .map(|l| format!("{:>4} {}", l + 1, st.buf.line_text(*l)))
                .collect();
            self.message(lines.join("\n"));
            return Ok(());
        }
        st.tracked = matching
            .iter()
            .map(|l| Some(st.buf.line_start(*l)))
            .collect();
        for k in 0..st.tracked.len() {
            let Some(p) = st.tracked[k] else {
                continue;
            };
            st.cursor = p;
            self.ex_inner(st, &cmd)?;
        }
        st.tracked.clear();
        Ok(())
    }

    fn sort(&mut self, st: &mut BufState, l1: usize, l2: usize, args: &str, reverse: bool) {
        let mut lines: Vec<String> = (l1..=l2).map(|l| st.buf.line_text(l)).collect();
        let ic = args.contains('i');
        if args.contains('n') {
            let num = |s: &str| -> Option<i64> {
                let start = s.find(|c: char| c.is_ascii_digit())?;
                let neg = start > 0 && s.as_bytes()[start - 1] == b'-';
                let digits: String = s[start..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                digits.parse::<i64>().ok().map(|v| if neg { -v } else { v })
            };
            lines.sort_by_key(|l| num(l).unwrap_or(i64::MIN));
        } else if ic {
            lines.sort_by_key(|l| l.to_lowercase());
        } else {
            lines.sort();
        }
        if reverse {
            lines.reverse();
        }
        if args.contains('u') {
            lines.dedup_by(|a, b| {
                if ic {
                    a.eq_ignore_ascii_case(b)
                } else {
                    a == b
                }
            });
        }
        let (s, e) = (st.buf.line_start(l1), st.buf.line_end(l2));
        let new = lines.join("\n");
        if new != st.buf.slice(s, e) {
            self.edit(st, s, e, &new);
        }
        st.cursor = st.buf.first_non_blank(l1);
    }
}

/// `:d [x] [count]` arguments.
fn reg_and_count(args: &str) -> (Option<char>, Option<usize>) {
    let mut a = args.trim();
    let mut reg = None;
    if let Some(c) = a.chars().next() {
        if !c.is_ascii_digit() && crate::registers::is_valid(c) {
            reg = Some(c);
            a = a[c.len_utf8()..].trim();
        }
    }
    let count = a.parse::<usize>().ok().filter(|n| *n > 0);
    (reg, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_commands() {
        assert_eq!(
            split_command("s/a/b/g"),
            ("s".into(), false, "/a/b/g".into())
        );
        assert_eq!(split_command("w!"), ("w".into(), true, String::new()));
        assert_eq!(split_command(">>"), (">>".into(), false, String::new()));
        assert_eq!(
            split_command("normal! dd"),
            ("normal".into(), true, "dd".into())
        );
        assert_eq!(string_value("\"\\<Space>\""), Some(" ".into()));
        assert_eq!(string_value("','"), Some(",".into()));
        assert_eq!(reg_and_count("a 3"), (Some('a'), Some(3)));
        assert_eq!(reg_and_count("2"), (None, Some(2)));
    }
}
