//! The Vim state machine: one [`Vim`] holds the global state (options, registers, mappings,
//! macros, search, handlers) and a [`BufState`] per attached editor (text copy, mode, cursor,
//! marks, pending keys). The UI forwards every editor key press with the document changes
//! since the last call and applies the answer: edits, selection, mode and effects such as
//! undo or running an IDE action.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::buffer::{class_of, Buffer, CharClass};
use crate::command::{self, Action, CmdKind, Command, InsertAt, Motion, Op, Parse, Scroll, Target};
use crate::handlers::{Handler, HandlerMode, Handlers};
use crate::keys::{self, char_of, DomKey, Key};
use crate::motions as mo;
use crate::options::Options;
use crate::pattern::{self, Pattern};
use crate::registers::{RegKind, Register, Registers};
use crate::textobj;

pub type Handle = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VisualKind {
    Char,
    Line,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Replace,
    Visual(VisualKind),
}

/// An edit in UTF-16 offsets. Edits of a response are sequential: each applies to the
/// document left by the previous one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub from: usize,
    pub to: usize,
    #[serde(default)]
    pub insert: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelRange {
    pub anchor: usize,
    pub head: usize,
}

/// Fully visible lines of the editor (0-based).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewport {
    pub top: usize,
    pub bottom: usize,
}

/// What changed in the editor since the last call.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Sync {
    /// The whole document (first call, or after the copies went out of step).
    pub text: Option<String>,
    /// Batches of changes; the changes of a batch are in the coordinates of the document
    /// before the batch, in ascending order (CodeMirror's `iterChanges`).
    pub changes: Vec<Vec<Change>>,
    pub selection: Option<Vec<SelRange>>,
    pub main: usize,
    pub viewport: Option<Viewport>,
    /// The editor's indentation unit ("    " or "\t").
    pub indent_unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Effect {
    Undo {
        count: usize,
    },
    Redo {
        count: usize,
    },
    /// Run an IDE action (PHPStorm action id).
    Action {
        id: String,
    },
    /// Select `from..to`, run `id` and put the caret at `caret` (e.g. `=` → AutoIndentLines).
    ActionOnRange {
        id: String,
        from: usize,
        to: usize,
        caret: usize,
    },
    /// Open a file (`:e path`).
    Open {
        path: String,
    },
    /// Write the system clipboard.
    Clipboard {
        text: String,
    },
    /// Scroll `pos` into view (`start`, `center`, `end`).
    Reveal {
        pos: usize,
        align: String,
    },
    /// Scroll so that `line` (0-based) is the top line.
    ScrollTo {
        line: usize,
    },
    /// Read `~/.ideavimrc` again (`:source`).
    ReloadRc,
    Bell,
}

impl Effect {
    /// Effects the UI performs asynchronously; queued keys wait for them.
    fn pauses(&self) -> bool {
        matches!(
            self,
            Effect::Undo { .. }
                | Effect::Redo { .. }
                | Effect::Action { .. }
                | Effect::ActionOnRange { .. }
                | Effect::Open { .. }
                | Effect::ReloadRc
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Message {
    pub text: String,
    pub error: bool,
}

/// The answer to a key press.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    /// Whether Vim handled the key (the editor must not).
    pub consumed: bool,
    pub changes: Vec<Change>,
    /// New selection (UTF-16); `None` leaves the editor's selection alone.
    pub selection: Option<Vec<SelRange>>,
    pub main: usize,
    /// Where the block caret is drawn (UTF-16).
    pub cursor: usize,
    /// `normal`, `insert`, `replace`, `visual`, `visualLine`, `visualBlock`.
    pub mode: String,
    /// Pending keys of an unfinished command ("2d", "\"a").
    pub showcmd: String,
    /// The command line being typed (":s/a/b", "/foo").
    pub cmdline: Option<String>,
    pub message: Option<Message>,
    pub effects: Vec<Effect>,
    /// Search matches to highlight.
    pub highlight: Option<Pattern>,
    pub recording: Option<char>,
    /// UTF-16 length of the document after the changes (to detect copies out of step).
    pub doc_length: usize,
    /// More queued keys wait for the effects; call `resume` after performing them.
    pub resume: bool,
    /// A mapping may continue; call `timeout` after `timeoutlen` ms without a key.
    pub wait: bool,
    pub number: bool,
    pub relativenumber: bool,
    pub scrolloff: usize,
    pub timeoutlen: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapMode {
    Normal,
    Visual,
    OpPending,
    Insert,
    CmdLine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    pub lhs: Vec<Key>,
    pub rhs: Vec<Key>,
    pub noremap: bool,
    /// The rhs as written, for `:map` listings.
    pub text: String,
}

enum Lookup {
    None,
    Prefix,
    Exact(Vec<Key>, bool),
    ExactPrefix,
}

#[derive(Debug, Default, Clone)]
pub struct Mappings {
    pub(crate) by_mode: HashMap<MapMode, Vec<Mapping>>,
}

impl Mappings {
    pub fn add(&mut self, mode: MapMode, m: Mapping) {
        let list = self.by_mode.entry(mode).or_default();
        list.retain(|x| x.lhs != m.lhs);
        list.push(m);
    }

    pub fn remove(&mut self, mode: MapMode, lhs: &[Key]) -> bool {
        let list = self.by_mode.entry(mode).or_default();
        let before = list.len();
        list.retain(|x| x.lhs != lhs);
        before != list.len()
    }

    pub fn clear(&mut self, mode: MapMode) {
        self.by_mode.remove(&mode);
    }

    fn lookup(&self, mode: MapMode, keys: &[Key]) -> Lookup {
        let Some(list) = self.by_mode.get(&mode) else {
            return Lookup::None;
        };
        let mut exact = None;
        let mut longer = false;
        for m in list {
            if m.lhs == keys {
                exact = Some((m.rhs.clone(), m.noremap));
            } else if m.lhs.len() > keys.len() && m.lhs.starts_with(keys) {
                longer = true;
            }
        }
        match (exact, longer) {
            (Some((rhs, nore)), false) => Lookup::Exact(rhs, nore),
            (Some(_), true) => Lookup::ExactPrefix,
            (None, true) => Lookup::Prefix,
            (None, false) => Lookup::None,
        }
    }

    fn exact(&self, mode: MapMode, keys: &[Key]) -> Option<(Vec<Key>, bool)> {
        self.by_mode
            .get(&mode)?
            .iter()
            .find(|m| m.lhs == keys)
            .map(|m| (m.rhs.clone(), m.noremap))
    }

    pub fn list(&self, mode: MapMode) -> &[Mapping] {
        self.by_mode.get(&mode).map(Vec::as_slice).unwrap_or(&[])
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Typed {
    key: Key,
    remap: bool,
    /// The key event that typed it (keys from mappings and macros have none).
    event: Option<u64>,
}

#[derive(Debug, Clone, Default)]
struct InsertSession {
    start: usize,
    end: usize,
    text: String,
}

#[derive(Debug, Clone, Copy)]
struct BlockInsert {
    first: usize,
    last: usize,
    col: usize,
    append_eol: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct CmdLine {
    pub(crate) kind: char,
    pub(crate) text: String,
    origin: usize,
    /// The operator waiting for a `/` motion.
    op: Option<Command>,
    count: Option<usize>,
    reg_pending: bool,
    history: Option<usize>,
}

#[derive(Debug, Clone)]
enum LastChange {
    Cmd {
        cmd: Command,
        insert: Option<String>,
    },
    Visual {
        op: Op,
        kind: VisualKind,
        lines: usize,
        chars: usize,
        insert: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct BufState {
    pub(crate) buf: Buffer,
    pub(crate) mode: Mode,
    pub(crate) cursor: usize,
    pub(crate) anchor: usize,
    /// Wanted column for vertical moves (`usize::MAX`: end of line).
    curswant: Option<usize>,
    pending: Vec<Key>,
    map_pending: Vec<Typed>,
    pub(crate) marks: HashMap<char, usize>,
    pub(crate) cmdline: Option<CmdLine>,
    insert: Option<InsertSession>,
    /// The command that started Insert mode and how often to repeat the text.
    insert_cmd: Option<(Command, usize)>,
    insert_reg_pending: bool,
    one_command: bool,
    replace_stack: Vec<Option<char>>,
    block_insert: Option<BlockInsert>,
    visual_change: Option<(Op, VisualKind, usize, usize)>,
    last_emitted: Option<Vec<SelRange>>,
    collapse_next: bool,
    last_visual: Option<(usize, usize, VisualKind)>,
    pub(crate) viewport: Viewport,
    pub(crate) indent_unit: String,
    /// Positions tracked through edits by `:g`.
    pub(crate) tracked: Vec<Option<usize>>,
}

impl BufState {
    fn new(text: &str) -> Self {
        Self {
            buf: Buffer::new(text),
            mode: Mode::Normal,
            cursor: 0,
            anchor: 0,
            curswant: None,
            pending: Vec::new(),
            map_pending: Vec::new(),
            marks: HashMap::new(),
            cmdline: None,
            insert: None,
            insert_cmd: None,
            insert_reg_pending: false,
            one_command: false,
            replace_stack: Vec::new(),
            block_insert: None,
            visual_change: None,
            last_emitted: None,
            collapse_next: false,
            last_visual: None,
            viewport: Viewport { top: 0, bottom: 40 },
            indent_unit: "    ".into(),
            tracked: Vec::new(),
        }
    }

    pub(crate) fn line(&self) -> usize {
        self.buf.line_of(self.cursor)
    }

    fn is_visual(&self) -> bool {
        matches!(self.mode, Mode::Visual(_))
    }
}

#[derive(Debug, Default)]
pub(crate) struct Out {
    changes: Vec<Change>,
    effects: Vec<Effect>,
    pub(crate) message: Option<Message>,
    passthrough: bool,
    pause: bool,
    resume: bool,
    wait: bool,
    scrolled: bool,
}

#[derive(Debug, Clone, Copy)]
struct MotionResult {
    pos: usize,
    linewise: bool,
    inclusive: bool,
    curswant: Option<usize>,
    jump: bool,
}

impl MotionResult {
    fn excl(pos: usize) -> Self {
        Self {
            pos,
            linewise: false,
            inclusive: false,
            curswant: None,
            jump: false,
        }
    }
    fn incl(pos: usize) -> Self {
        Self {
            inclusive: true,
            ..Self::excl(pos)
        }
    }
    fn lines(pos: usize) -> Self {
        Self {
            linewise: true,
            ..Self::excl(pos)
        }
    }
    fn jump(self) -> Self {
        Self { jump: true, ..self }
    }
}

pub struct Vim {
    pub options: Options,
    pub(crate) registers: Registers,
    pub mappings: Mappings,
    pub leader: String,
    pub handlers: Handlers,
    pub enabled: bool,
    bufs: HashMap<Handle, BufState>,
    next: Handle,
    active: Option<Handle>,
    typeahead: VecDeque<Typed>,
    event: u64,
    pub(crate) out: Out,
    pub(crate) last_search: Option<(String, bool)>,
    pub(crate) hl: bool,
    last_find: Option<(char, bool, bool)>,
    last_change: Option<LastChange>,
    pub(crate) last_sub: Option<(String, String, String)>,
    recording: Option<(char, Vec<Key>)>,
    last_macro: Option<char>,
    pub(crate) last_ex: Option<String>,
    pub(crate) cmd_history: Vec<String>,
    search_history: Vec<String>,
    replaying: usize,
}

impl Default for Vim {
    fn default() -> Self {
        Self::new()
    }
}

const CHAR_ARG: [&str; 13] = [
    "f", "t", "F", "T", "r", "m", "q", "@", "`", "'", "\"", "g`", "g'",
];

fn op_changes_text(op: Op) -> bool {
    op != Op::Yank
}

fn ordered(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

impl Vim {
    pub fn new() -> Self {
        Self {
            options: Options::default(),
            registers: Registers::default(),
            mappings: Mappings::default(),
            leader: "\\".into(),
            handlers: Handlers::default(),
            enabled: true,
            bufs: HashMap::new(),
            next: 1,
            active: None,
            typeahead: VecDeque::new(),
            event: 0,
            out: Out::default(),
            last_search: None,
            hl: false,
            last_find: None,
            last_change: None,
            last_sub: None,
            recording: None,
            last_macro: None,
            last_ex: None,
            cmd_history: Vec::new(),
            search_history: Vec::new(),
            replaying: 0,
        }
    }

    // ---- Attaching editors

    /// Start tracking an editor with `text`; returns its handle.
    pub fn attach(&mut self, text: &str) -> Handle {
        let h = self.next;
        self.next += 1;
        self.bufs.insert(h, BufState::new(text));
        h
    }

    pub fn detach(&mut self, h: Handle) {
        self.bufs.remove(&h);
        if self.active == Some(h) {
            self.active = None;
        }
    }

    /// The focused editor (its keys may be claimed from the IDE keymap); `None` on blur.
    pub fn focus(&mut self, h: Option<Handle>) {
        self.active = h.filter(|h| self.bufs.contains_key(h));
    }

    pub fn active(&self) -> Option<Handle> {
        self.active
    }

    pub fn set_clipboard(&mut self, text: &str) {
        self.registers
            .set_clipboard_mirror(&text.replace("\r\n", "\n"));
    }

    // ---- Key handling

    /// Handle a key press (`None`: only sync, e.g. `resume` after effects).
    pub fn key(&mut self, h: Handle, key: Option<Key>, sync: &Sync) -> Response {
        let Some(mut st) = self.bufs.remove(&h) else {
            return Response::default();
        };
        self.out = Out::default();
        self.sync(&mut st, sync);
        let mut consumed = false;
        if let Some(k) = key {
            self.event += 1;
            if let Some((_, rec)) = self.recording.as_mut() {
                rec.push(k.clone());
            }
            self.typeahead.push_back(Typed {
                key: k,
                remap: true,
                event: Some(self.event),
            });
            self.run(&mut st);
            consumed = !self.out.passthrough;
        } else {
            self.run(&mut st);
        }
        let r = self.respond(&mut st, consumed);
        self.bufs.insert(h, st);
        r
    }

    /// Handle a DOM key event.
    pub fn key_event(&mut self, h: Handle, e: &DomKey, sync: &Sync) -> Response {
        match keys::key_of_event(e) {
            Some(k) => self.key(h, Some(k), sync),
            None => {
                let mut r = self.key(h, None, sync);
                r.consumed = false;
                r
            }
        }
    }

    /// Feed notation keys ("dw", "ciwfoo<Esc>") as if typed; keys Vim leaves to the editor
    /// (text typed in Insert mode) are applied like the editor would. Used by tests.
    pub fn feed(&mut self, h: Handle, text: &str) -> Response {
        let mut last = Response::default();
        for k in keys::parse_keys(text, &self.leader.clone()) {
            last = self.key(h, Some(k.clone()), &Sync::default());
            if !last.consumed {
                if let Some(mut st) = self.bufs.remove(&h) {
                    self.out = Out::default();
                    self.replaying += 1;
                    self.insert_internal(&mut st, &k);
                    self.replaying -= 1;
                    self.bufs.insert(h, st);
                }
            }
        }
        last
    }

    /// No key arrived within `timeoutlen`: resolve a pending mapping.
    pub fn timeout(&mut self, h: Handle, sync: &Sync) -> Response {
        let Some(mut st) = self.bufs.remove(&h) else {
            return Response::default();
        };
        self.out = Out::default();
        self.sync(&mut st, sync);
        if !st.map_pending.is_empty() {
            let keys: Vec<Key> = st.map_pending.iter().map(|t| t.key.clone()).collect();
            let mode = self.map_mode(&st).unwrap_or(MapMode::Normal);
            let pending: Vec<Typed> = std::mem::take(&mut st.map_pending);
            match self.mappings.exact(mode, &keys) {
                Some((rhs, noremap)) => self.expand(&keys, rhs, noremap),
                None => {
                    for t in pending.into_iter().rev() {
                        self.typeahead.push_front(Typed {
                            remap: false,
                            event: None,
                            ..t
                        });
                    }
                }
            }
            self.run(&mut st);
        }
        let r = self.respond(&mut st, true);
        self.bufs.insert(h, st);
        r
    }

    /// The current state of an editor (mode, caret) without a key.
    pub fn state(&mut self, h: Handle) -> Response {
        let Some(mut st) = self.bufs.remove(&h) else {
            return Response::default();
        };
        self.out = Out::default();
        let mut r = self.respond(&mut st, false);
        r.selection = None;
        self.bufs.insert(h, st);
        r
    }

    /// Whether the IDE keymap should leave this key to the focused editor's Vim.
    pub fn claims(&self, e: &DomKey) -> bool {
        if !self.enabled || e.alt_key || e.meta_key {
            return false;
        }
        let Some(st) = self.active.and_then(|h| self.bufs.get(&h)) else {
            return false;
        };
        let Some(k) = keys::key_of_event(e) else {
            return false;
        };
        if k.starts_with("<F") || k == "<S-Esc>" {
            return false;
        }
        if st.cmdline.is_some() {
            return !e.ctrl_key
                || matches!(
                    k.as_str(),
                    "<C-r>" | "<C-w>" | "<C-u>" | "<C-h>" | "<Esc>" | "<C-c>" | "<C-j>" | "<C-m>"
                );
        }
        if e.ctrl_key {
            let mode = match st.mode {
                Mode::Insert | Mode::Replace => HandlerMode::Insert,
                Mode::Visual(_) => HandlerMode::Visual,
                Mode::Normal => HandlerMode::Normal,
            };
            let key = if k == "<Esc>" { "<C-[>".to_string() } else { k };
            return self.handlers.handler(&key, mode) == Handler::Vim;
        }
        match st.mode {
            Mode::Insert | Mode::Replace => {
                k == "<Esc>"
                    || k == "<Insert>"
                    || st.insert_reg_pending
                    || !st.map_pending.is_empty()
            }
            _ => true,
        }
    }

    // ---- Sync with the editor

    fn sync(&mut self, st: &mut BufState, s: &Sync) {
        if let Some(u) = &s.indent_unit {
            st.indent_unit = u.clone();
        }
        if let Some(v) = s.viewport {
            st.viewport = v;
        }
        if let Some(text) = &s.text {
            st.buf = Buffer::new(text);
            st.insert = None;
            let len = st.buf.len();
            for m in st.marks.values_mut() {
                *m = (*m).min(len);
            }
        }
        for batch in &s.changes {
            for ch in batch.iter().rev() {
                let from = st.buf.from_utf16(ch.from);
                let to = st.buf.from_utf16(ch.to);
                self.apply(st, from, to, &ch.insert);
            }
        }
        if let Some(sel) = &s.selection {
            self.adopt_selection(st, sel, s.main);
        }
        st.cursor = st.cursor.min(st.buf.len());
        st.anchor = st.anchor.min(st.buf.len());
    }

    fn adopt_selection(&mut self, st: &mut BufState, sel: &[SelRange], main: usize) {
        let Some(m) = sel.get(main).or(sel.first()).copied() else {
            return;
        };
        let insertish = matches!(st.mode, Mode::Insert | Mode::Replace);
        if !insertish && st.last_emitted.as_deref() == Some(sel) {
            return;
        }
        let a = st.buf.from_utf16(m.anchor);
        let h = st.buf.from_utf16(m.head);
        if st.cmdline.is_some() {
            return;
        }
        match st.mode {
            Mode::Insert | Mode::Replace => st.cursor = h,
            Mode::Normal => {
                if st.collapse_next || a == h || !self.enabled {
                    st.cursor = h;
                } else {
                    st.mode = Mode::Visual(VisualKind::Char);
                    st.anchor = if h > a { a } else { a.saturating_sub(1) };
                    st.cursor = if h > a { h - 1 } else { h };
                }
                st.curswant = None;
            }
            Mode::Visual(kind) => {
                if a == h {
                    st.mode = Mode::Normal;
                    st.cursor = h;
                } else if kind == VisualKind::Char {
                    st.anchor = if h > a { a } else { a.saturating_sub(1) };
                    st.cursor = if h > a { h - 1 } else { h };
                } else {
                    st.anchor = a;
                    st.cursor = if h > a { h.saturating_sub(1) } else { h };
                }
            }
        }
        st.collapse_next = false;
        st.pending.clear();
    }

    /// Apply an edit (char indices) to the copy: marks, tracked positions and the insert
    /// session follow it.
    fn apply(&mut self, st: &mut BufState, from: usize, to: usize, text: &str) {
        let len = text.chars().count();
        let map = |p: usize| -> usize {
            if p < from {
                p
            } else if p >= to {
                p - (to - from) + len
            } else {
                from
            }
        };
        for m in st.marks.values_mut() {
            *m = map(*m);
        }
        for t in st.tracked.iter_mut() {
            *t = t.and_then(|p| {
                if p >= from && p < to && to > from && !(p == from && len > 0) {
                    None
                } else {
                    Some(map(p))
                }
            });
        }
        if st.mode == Mode::Insert {
            if let Some(s) = st.insert.as_mut() {
                if from >= s.start && to <= s.end {
                    let mut chars: Vec<char> = s.text.chars().collect();
                    let (a, b) = (from - s.start, (to - s.start).min(chars.len()));
                    chars.splice(a..b, text.chars());
                    s.text = chars.into_iter().collect();
                    s.end = s.end - (to - from) + len;
                } else if to <= s.start {
                    s.start = s.start - (to - from) + len;
                    s.end = s.end - (to - from) + len;
                } else if from < s.end {
                    s.start = from + len;
                    s.end = s.start;
                    s.text.clear();
                }
            }
        }
        st.buf.replace(from, to, text);
        st.anchor = map(st.anchor).min(st.buf.len());
    }

    /// Edit the document: the change goes to the editor and to the copy.
    pub(crate) fn edit(&mut self, st: &mut BufState, from: usize, to: usize, text: &str) {
        let (from, to) = (from.min(st.buf.len()), to.min(st.buf.len()));
        if from == to && text.is_empty() {
            return;
        }
        self.out.changes.push(Change {
            from: st.buf.to_utf16(from),
            to: st.buf.to_utf16(to),
            insert: text.to_string(),
        });
        self.apply(st, from, to, text);
        st.marks.insert('[', from);
        st.marks.insert('.', from);
        st.marks.insert(
            ']',
            (from + text.chars().count()).saturating_sub(1).max(from),
        );
    }

    // ---- Output

    pub(crate) fn message(&mut self, text: impl Into<String>) {
        self.out.message = Some(Message {
            text: text.into(),
            error: false,
        });
    }

    pub(crate) fn error(&mut self, text: impl Into<String>) {
        self.out.message = Some(Message {
            text: text.into(),
            error: true,
        });
    }

    pub(crate) fn effect(&mut self, e: Effect) {
        if e.pauses() {
            self.out.pause = true;
        }
        if matches!(e, Effect::ScrollTo { .. } | Effect::Reveal { .. }) {
            self.out.scrolled = true;
        }
        self.out.effects.push(e);
    }

    fn bell(&mut self) {
        if self.options.visualbell {
            self.effect(Effect::Bell);
        }
    }

    fn mode_name(st: &BufState) -> &'static str {
        match st.mode {
            Mode::Normal => "normal",
            Mode::Insert => "insert",
            Mode::Replace => "replace",
            Mode::Visual(VisualKind::Char) => "visual",
            Mode::Visual(VisualKind::Line) => "visualLine",
            Mode::Visual(VisualKind::Block) => "visualBlock",
        }
    }

    fn clamp_cursor(st: &mut BufState) {
        let b = &st.buf;
        st.cursor = st.cursor.min(b.len());
        if st.mode == Mode::Normal {
            let line = b.line_of(st.cursor);
            st.cursor = st.cursor.min(b.line_last(line)).max(b.line_start(line));
        }
    }

    /// Selection ranges (UTF-16) showing the state.
    fn selection_of(&self, st: &BufState) -> Vec<SelRange> {
        let b = &st.buf;
        let u = |p: usize| b.to_utf16(p);
        let caret = |p| {
            vec![SelRange {
                anchor: u(p),
                head: u(p),
            }]
        };
        match st.mode {
            Mode::Visual(VisualKind::Char) => {
                let (a, c) = (st.anchor, st.cursor);
                if c >= a {
                    vec![SelRange {
                        anchor: u(a),
                        head: u((c + 1).min(b.len())),
                    }]
                } else {
                    vec![SelRange {
                        anchor: u((a + 1).min(b.len())),
                        head: u(c),
                    }]
                }
            }
            Mode::Visual(VisualKind::Line) => {
                let (la, lc) = (b.line_of(st.anchor), b.line_of(st.cursor));
                let line_to = |l: usize| {
                    if l + 1 < b.line_count() {
                        b.line_end(l) + 1
                    } else {
                        b.line_end(l)
                    }
                };
                if lc >= la {
                    vec![SelRange {
                        anchor: u(b.line_start(la)),
                        head: u(line_to(lc)),
                    }]
                } else {
                    vec![SelRange {
                        anchor: u(line_to(la)),
                        head: u(b.line_start(lc)),
                    }]
                }
            }
            Mode::Visual(VisualKind::Block) => {
                let (l1, l2, c1, c2) = self.block_bounds(st);
                let down = b.line_of(st.cursor) >= b.line_of(st.anchor);
                let mut out: Vec<SelRange> = (l1..=l2)
                    .map(|l| {
                        let s = b.line_start(l);
                        let e = b.line_end(l);
                        let from = (s + c1).min(e);
                        let to = if c2 == usize::MAX {
                            e
                        } else {
                            (s + c2 + 1).min(e)
                        };
                        SelRange {
                            anchor: u(from),
                            head: u(to.max(from)),
                        }
                    })
                    .collect();
                if !down {
                    out.reverse();
                }
                out
            }
            _ => caret(st.cursor),
        }
    }

    fn block_bounds(&self, st: &BufState) -> (usize, usize, usize, usize) {
        let b = &st.buf;
        let (l1, l2) = ordered(b.line_of(st.anchor), b.line_of(st.cursor));
        let (c1, mut c2) = ordered(b.col_of(st.anchor), b.col_of(st.cursor));
        if st.curswant == Some(usize::MAX) {
            c2 = usize::MAX;
        }
        (l1, l2, c1, c2)
    }

    fn respond(&mut self, st: &mut BufState, consumed: bool) -> Response {
        if st.cmdline.is_none() {
            Self::clamp_cursor(st);
        }
        let out = std::mem::take(&mut self.out);
        let selection = if out.passthrough && out.changes.is_empty() {
            None
        } else {
            Some(self.selection_of(st))
        };
        if selection.is_some() {
            st.last_emitted = selection.clone();
        }
        let main = match st.mode {
            Mode::Visual(VisualKind::Block) => {
                selection.as_ref().map_or(0, |s| s.len().saturating_sub(1))
            }
            _ => 0,
        };
        let mut effects = out.effects;
        for text in self.registers.clipboard_writes.drain(..) {
            effects.push(Effect::Clipboard { text });
        }
        if !out.scrolled && selection.is_some() {
            // The caret stays visible (with `scrolloff`).
            effects.push(Effect::Reveal {
                pos: st.buf.to_utf16(st.cursor),
                align: "nearest".into(),
            });
        }
        let highlight = match &st.cmdline {
            Some(cl)
                if (cl.kind == '/' || cl.kind == '?')
                    && self.options.incsearch
                    && !cl.text.is_empty() =>
            {
                Some(pattern::translate(
                    &cl.text,
                    self.options.ignorecase,
                    self.options.smartcase,
                ))
            }
            _ if self.options.hlsearch && self.hl => self.last_search.as_ref().map(|(p, _)| {
                pattern::translate(p, self.options.ignorecase, self.options.smartcase)
            }),
            _ => None,
        };
        let mut showcmd: Vec<Key> = st.pending.clone();
        showcmd.extend(st.map_pending.iter().map(|t| t.key.clone()));
        if st.insert_reg_pending {
            showcmd.push("\"".into());
        }
        Response {
            consumed,
            changes: out.changes,
            selection,
            main,
            cursor: st.buf.to_utf16(st.cursor),
            mode: Self::mode_name(st).into(),
            showcmd: keys::display_keys(&showcmd),
            cmdline: st.cmdline.as_ref().map(|c| format!("{}{}", c.kind, c.text)),
            message: out.message,
            effects,
            highlight,
            recording: self.recording.as_ref().map(|r| r.0),
            doc_length: st.buf.utf16_len(),
            resume: out.resume,
            wait: out.wait,
            number: self.options.number,
            relativenumber: self.options.relativenumber,
            scrolloff: self.options.scrolloff,
            timeoutlen: if self.options.timeout {
                self.options.timeoutlen
            } else {
                0
            },
        }
    }

    // ---- Typeahead and mappings

    fn map_mode(&self, st: &BufState) -> Option<MapMode> {
        if let Some(cl) = &st.cmdline {
            return (!cl.reg_pending).then_some(MapMode::CmdLine);
        }
        match st.mode {
            Mode::Insert | Mode::Replace => (!st.insert_reg_pending).then_some(MapMode::Insert),
            Mode::Normal | Mode::Visual(_) => {
                if let Some(last) = st.pending.last() {
                    let stop_recording = last == "q" && self.recording.is_some();
                    if CHAR_ARG.contains(&last.as_str()) && !stop_recording {
                        return None;
                    }
                }
                if st.is_visual() {
                    return Some(MapMode::Visual);
                }
                let op_pending = st.pending.iter().any(|k| {
                    matches!(
                        k.as_str(),
                        "d" | "c" | "y" | ">" | "<" | "=" | "g~" | "gu" | "gU"
                    )
                }) || st
                    .pending
                    .windows(2)
                    .any(|w| w[0] == "g" && matches!(w[1].as_str(), "~" | "u" | "U" | "?"));
                Some(if op_pending {
                    MapMode::OpPending
                } else {
                    MapMode::Normal
                })
            }
        }
    }

    fn expand(&mut self, lhs: &[Key], rhs: Vec<Key>, noremap: bool) {
        let keep = !noremap && rhs.starts_with(lhs);
        for (i, k) in rhs.into_iter().enumerate().rev() {
            self.typeahead.push_front(Typed {
                key: k,
                remap: !noremap && !(keep && i < lhs.len()),
                event: None,
            });
        }
    }

    fn run(&mut self, st: &mut BufState) {
        let mut steps = 0;
        while let Some(t) = self.typeahead.pop_front() {
            steps += 1;
            if steps > 20_000 {
                self.error("E223: Recursive mapping");
                self.typeahead.clear();
                st.map_pending.clear();
                break;
            }
            match (t.remap, self.map_mode(st)) {
                (true, Some(mode)) => {
                    st.map_pending.push(t);
                    let lhs: Vec<Key> = st.map_pending.iter().map(|t| t.key.clone()).collect();
                    match self.mappings.lookup(mode, &lhs) {
                        Lookup::Exact(rhs, noremap) => {
                            st.map_pending.clear();
                            self.expand(&lhs, rhs, noremap);
                        }
                        Lookup::Prefix | Lookup::ExactPrefix => {
                            if self.typeahead.is_empty() {
                                self.out.wait = true;
                                return;
                            }
                        }
                        Lookup::None => {
                            let first = st.map_pending.remove(0);
                            let rest: Vec<Typed> = st.map_pending.drain(..).collect();
                            for r in rest.into_iter().rev() {
                                self.typeahead.push_front(r);
                            }
                            self.process(st, first);
                        }
                    }
                }
                _ => self.process(st, t),
            }
            if self.out.pause && !self.typeahead.is_empty() {
                self.out.resume = true;
                return;
            }
        }
    }

    fn process(&mut self, st: &mut BufState, t: Typed) {
        let native = t.event == Some(self.event) && self.replaying == 0;
        let k = t.key;
        if let Some(id) = k
            .strip_prefix("<Action>(")
            .and_then(|r| r.strip_suffix(')'))
        {
            st.pending.clear();
            self.effect(Effect::Action { id: id.to_string() });
            return;
        }
        if st.cmdline.is_some() {
            self.cmdline_key(st, &k);
            return;
        }
        match st.mode {
            Mode::Insert => self.insert_key(st, &k, native),
            Mode::Replace => self.replace_key(st, &k, native),
            Mode::Normal | Mode::Visual(_) => self.normal_key(st, k),
        }
    }

    // ---- Normal and Visual mode

    fn normal_key(&mut self, st: &mut BufState, k: Key) {
        st.pending.push(k);
        let visual = st.is_visual();
        match command::parse(&st.pending, visual, self.recording.is_some()) {
            Parse::Incomplete => {}
            Parse::Invalid => {
                let esc = st.pending.last().map(String::as_str) == Some("<Esc>");
                st.pending.clear();
                if !esc {
                    self.bell();
                }
            }
            Parse::Done(cmd) => {
                st.pending.clear();
                self.exec(st, cmd);
                if st.one_command && st.mode == Mode::Normal && st.cmdline.is_none() {
                    st.one_command = false;
                    st.mode = Mode::Insert;
                }
            }
        }
    }

    fn exec(&mut self, st: &mut BufState, cmd: Command) {
        match cmd.kind.clone() {
            CmdKind::Motion(m) => self.exec_motion(st, &cmd, &m),
            CmdKind::Operator { op, target } => self.exec_operator(st, &cmd, op, &target),
            CmdKind::VisualOp(op) => self.visual_op(st, &cmd, op),
            CmdKind::VisualObject { inner, ch } => self.visual_object(st, inner, ch, cmd.count1()),
            CmdKind::Action(a) => self.exec_action(st, &cmd, a),
        }
    }

    fn exec_motion(&mut self, st: &mut BufState, cmd: &Command, m: &Motion) {
        if let Motion::Search { pattern, forward } = m {
            if pattern.is_empty() {
                self.open_cmdline(st, if *forward { '/' } else { '?' }, None, cmd.count);
                return;
            }
        }
        match self.eval_motion(st, m, cmd.count, false) {
            None => self.bell(),
            Some(r) => {
                if r.jump {
                    st.marks.insert('\'', st.cursor);
                    st.marks.insert('`', st.cursor);
                }
                st.cursor = r.pos;
                st.curswant = r.curswant;
                if st.mode == Mode::Normal {
                    Self::clamp_cursor(st);
                } else if st.curswant != Some(usize::MAX) {
                    let line = st.buf.line_of(st.cursor);
                    st.cursor = st.cursor.min(st.buf.line_last(line));
                }
            }
        }
    }

    fn eval_motion(
        &mut self,
        st: &BufState,
        m: &Motion,
        count: Option<usize>,
        op: bool,
    ) -> Option<MotionResult> {
        let b = &st.buf;
        let c = st.cursor;
        let n = count.unwrap_or(1).max(1);
        let line = b.line_of(c);
        let last = b.line_count() - 1;
        let col = st.curswant.unwrap_or_else(|| b.col_of(c));
        let vertical = |tl: usize| {
            let pos = if col == usize::MAX {
                b.line_last(tl)
            } else {
                b.pos_at(tl, col, false)
            };
            MotionResult {
                curswant: Some(col),
                ..MotionResult::lines(pos)
            }
        };
        Some(match m {
            Motion::Left => {
                let s = b.line_start(line);
                if c == s {
                    return None;
                }
                MotionResult::excl(c.saturating_sub(n).max(s))
            }
            Motion::Right => {
                let end = if op || st.is_visual() {
                    b.line_end(line)
                } else {
                    b.line_last(line)
                };
                let pos = (c + n).min(end);
                if pos == c && !(op && b.line_len(line) > 0) {
                    return None;
                }
                MotionResult::excl(pos)
            }
            Motion::Down => {
                if line == last {
                    return None;
                }
                vertical((line + n).min(last))
            }
            Motion::Up => {
                if line == 0 {
                    return None;
                }
                vertical(line.saturating_sub(n))
            }
            Motion::WordFwd(big) => {
                let mut p = c;
                for _ in 0..n {
                    p = mo::word_forward(b, p, *big);
                }
                MotionResult::excl(if op {
                    p
                } else {
                    p.min(b.len().saturating_sub(1))
                })
            }
            Motion::WordBack(big) => {
                let mut p = c;
                for _ in 0..n {
                    p = mo::word_back(b, p, *big);
                }
                MotionResult::excl(p)
            }
            Motion::WordEnd(big) => {
                let mut p = c;
                for _ in 0..n {
                    p = mo::word_end(b, p, *big);
                }
                MotionResult::incl(p)
            }
            Motion::WordEndBack(big) => {
                let mut p = c;
                for _ in 0..n {
                    p = mo::word_end_back(b, p, *big);
                }
                MotionResult::incl(p)
            }
            Motion::LineStart => MotionResult::excl(b.line_start(line)),
            Motion::FirstNonBlank => MotionResult::excl(b.first_non_blank(line)),
            Motion::LineEnd => {
                let l = (line + n - 1).min(last);
                let pos = if op || st.is_visual() {
                    b.line_last(l).max(b.line_start(l))
                } else {
                    b.line_last(l)
                };
                let mut r = MotionResult::incl(pos);
                if b.line_len(l) == 0 {
                    r.inclusive = false;
                }
                r.curswant = Some(usize::MAX);
                r
            }
            Motion::LastNonBlank => {
                let l = (line + n - 1).min(last);
                let (s, mut e) = (b.line_start(l), b.line_end(l));
                while e > s && matches!(b.at(e - 1), Some(' ' | '\t')) {
                    e -= 1;
                }
                MotionResult::incl(e.saturating_sub(1).max(s))
            }
            Motion::LastLine => {
                let l = count.map(|x| x.saturating_sub(1).min(last)).unwrap_or(last);
                MotionResult::lines(b.first_non_blank(l)).jump()
            }
            Motion::FirstLine => {
                let l = count.map(|x| x.saturating_sub(1).min(last)).unwrap_or(0);
                MotionResult::lines(b.first_non_blank(l)).jump()
            }
            Motion::Find { ch, forward, till } => {
                self.last_find = Some((*ch, *forward, *till));
                let p = mo::find_char(b, c, *ch, *forward, *till, n, false)?;
                if *forward {
                    MotionResult::incl(p)
                } else {
                    MotionResult::excl(p)
                }
            }
            Motion::RepeatFind { reverse } => {
                let (ch, f, till) = self.last_find?;
                let forward = f != *reverse;
                let p = mo::find_char(b, c, ch, forward, till, n, true)?;
                if forward {
                    MotionResult::incl(p)
                } else {
                    MotionResult::excl(p)
                }
            }
            Motion::Percent => match count {
                Some(pc) => {
                    let l = ((pc.min(100) * b.line_count()).div_ceil(100)).saturating_sub(1);
                    MotionResult::lines(b.first_non_blank(l.min(last))).jump()
                }
                None => MotionResult::incl(mo::percent(b, c)?).jump(),
            },
            Motion::ParaFwd | Motion::ParaBack => {
                let mut p = c;
                for _ in 0..n {
                    p = if *m == Motion::ParaFwd {
                        mo::paragraph_forward(b, p)
                    } else {
                        mo::paragraph_back(b, p)
                    };
                }
                let mut r = MotionResult::excl(p).jump();
                if *m == Motion::ParaFwd && p + 1 >= b.len() && b.line_len(b.line_of(p)) > 0 {
                    r.inclusive = true;
                }
                r
            }
            Motion::ScreenTop | Motion::ScreenMiddle | Motion::ScreenBottom => {
                let vp = st.viewport;
                let (top, bottom) = (vp.top.min(last), vp.bottom.min(last).max(vp.top.min(last)));
                let so = self.options.scrolloff;
                let l = match m {
                    Motion::ScreenTop => {
                        let min = if top == 0 { 0 } else { top + so };
                        (top + n - 1).max(min).min(bottom)
                    }
                    Motion::ScreenBottom => {
                        let max = if bottom == last {
                            last
                        } else {
                            bottom.saturating_sub(so)
                        };
                        bottom.saturating_sub(n - 1).min(max).max(top)
                    }
                    _ => (top + bottom) / 2,
                };
                MotionResult::lines(b.first_non_blank(l)).jump()
            }
            Motion::NextLine => {
                if line + n > last {
                    return None;
                }
                MotionResult::lines(b.first_non_blank(line + n))
            }
            Motion::PrevLine => {
                if n > line {
                    return None;
                }
                MotionResult::lines(b.first_non_blank(line - n))
            }
            Motion::CurrentLine => MotionResult::lines(b.first_non_blank((line + n - 1).min(last))),
            Motion::Column => {
                let mut r = MotionResult::excl(b.pos_at(line, n - 1, false));
                r.curswant = Some(n - 1);
                r
            }
            Motion::SearchNext { reverse } => {
                let Some((pat, fwd)) = self.last_search.clone() else {
                    self.error("E35: No previous regular expression");
                    return None;
                };
                let forward = fwd != *reverse;
                let p = self.search(st, &pat, forward, c, n)?;
                MotionResult::excl(p).jump()
            }
            Motion::Star { forward, whole } => {
                let (s, e) = mo::word_under_cursor(b, c)?;
                let word = escape_pattern(&b.slice(s, e));
                let pat = if *whole {
                    format!("\\<{word}\\>")
                } else {
                    word
                };
                self.last_search = Some((pat.clone(), *forward));
                self.search_history.push(pat.clone());
                self.registers.set_special('/', pat.clone());
                self.hl = true;
                let p = self.search(st, &pat, *forward, s, n)?;
                MotionResult::excl(p).jump()
            }
            Motion::Mark { ch, linewise } => {
                let p = match ch {
                    '\'' | '`' => st.marks.get(&'\'').copied().unwrap_or(0),
                    _ => match st.marks.get(ch) {
                        Some(p) => *p,
                        None => {
                            self.error("E20: Mark not set");
                            return None;
                        }
                    },
                };
                let p = p.min(b.len());
                if *linewise {
                    MotionResult::lines(b.first_non_blank(b.line_of(p))).jump()
                } else {
                    MotionResult::excl(p).jump()
                }
            }
            Motion::Search { pattern, forward } => {
                let p = self.search(st, pattern, *forward, c, n)?;
                MotionResult::excl(p).jump()
            }
        })
    }

    /// Find `pattern` `count` times from `from`; reports errors and wrap-around.
    pub(crate) fn search(
        &mut self,
        st: &BufState,
        pattern: &str,
        forward: bool,
        from: usize,
        count: usize,
    ) -> Option<usize> {
        let pat = pattern::translate(pattern, self.options.ignorecase, self.options.smartcase);
        let re = match pat.regex() {
            Ok(r) => r,
            Err(e) => {
                self.error(e);
                return None;
            }
        };
        let b = &st.buf;
        let text = b.text();
        let starts: Vec<usize> = re.find_iter(text).map(|m| b.from_byte(m.start())).collect();
        if starts.is_empty() {
            self.error(format!("E486: Pattern not found: {pattern}"));
            return None;
        }
        let mut pos = from;
        let mut wrapped = false;
        for _ in 0..count.max(1) {
            let next = if forward {
                starts.iter().find(|s| **s > pos).copied()
            } else {
                starts.iter().rev().find(|s| **s < pos).copied()
            };
            pos = match next {
                Some(p) => p,
                None if self.options.wrapscan => {
                    wrapped = true;
                    if forward {
                        starts[0]
                    } else {
                        *starts.last().unwrap()
                    }
                }
                None => {
                    self.error(format!(
                        "E38{}: Search hit {} without match for: {pattern}",
                        if forward { 5 } else { 4 },
                        if forward { "BOTTOM" } else { "TOP" }
                    ));
                    return None;
                }
            };
        }
        if wrapped {
            self.message(if forward {
                "search hit BOTTOM, continuing at TOP"
            } else {
                "search hit TOP, continuing at BOTTOM"
            });
        }
        Some(pos)
    }

    // ---- Operators

    /// The range of an operator's motion or text object: (from, to, linewise, block_lines).
    fn operator_range(
        &mut self,
        st: &BufState,
        op: Op,
        target: &Target,
        count: Option<usize>,
    ) -> Option<(usize, usize, bool, bool)> {
        let b = &st.buf;
        let c = st.cursor;
        let n = count.unwrap_or(1).max(1);
        match target {
            Target::Lines => {
                let l1 = b.line_of(c);
                let l2 = l1 + n - 1;
                if l2 >= b.line_count() && n > 1 {
                    return Some((
                        b.line_start(l1),
                        b.line_end(b.line_count() - 1),
                        true,
                        false,
                    ));
                }
                Some((b.line_start(l1), b.line_end(l2), true, false))
            }
            Target::Object { inner, ch } => {
                let r = textobj::text_object(b, c, *inner, *ch, n)?;
                if r.linewise {
                    let (l1, l2) = (
                        b.line_of(r.from),
                        b.line_of(r.to.saturating_sub(1).max(r.from)),
                    );
                    return Some((b.line_start(l1), b.line_end(l2), true, false));
                }
                Some((r.from, r.to, r.linewise, r.block_lines))
            }
            Target::Motion(m) => {
                // `cw` on a word changes to its end, like `ce`.
                if let (Op::Change, Motion::WordFwd(big)) = (op, m) {
                    if b.at(c)
                        .is_some_and(|ch| class_of(ch, *big) != CharClass::Blank && ch != '\n')
                    {
                        let mut e = c;
                        for k in 0..n {
                            let at_end = b.at(e + 1).is_none_or(|x| {
                                class_of(x, *big) != class_of(b.at(e).unwrap_or(' '), *big)
                            });
                            if !(k == 0 && at_end) {
                                e = mo::word_end(b, e, *big);
                            }
                        }
                        return Some((c, (e + 1).min(b.len()), false, false));
                    }
                }
                // `dw` on the last word of a line stops at the line end.
                if let Motion::WordFwd(big) = m {
                    let mut p = c;
                    let mut prev = c;
                    for _ in 0..n {
                        prev = p;
                        p = mo::word_forward(b, p, *big);
                    }
                    if b.line_of(p) > b.line_of(prev) || p >= b.len() {
                        let end = b.line_end(b.line_of(prev));
                        if end > prev || p >= b.len() {
                            p = end.max(prev).min(p);
                        }
                    }
                    return Some((c, p, false, false));
                }
                let r = self.eval_motion(st, m, count, true)?;
                let b = &st.buf;
                if r.linewise {
                    let (a, z) = ordered(c, r.pos);
                    return Some((
                        b.line_start(b.line_of(a)),
                        b.line_end(b.line_of(z)),
                        true,
                        false,
                    ));
                }
                let (a, z) = ordered(c, r.pos);
                let mut to = if r.inclusive { (z + 1).min(b.len()) } else { z };
                let mut linewise = false;
                if !r.inclusive && to > a {
                    let (la, lt) = (b.line_of(a), b.line_of(to));
                    if lt > la && b.line_start(lt) == to {
                        to = b.line_end(lt - 1);
                        if a <= b.first_non_blank(la) && b.line_len(la) > 0 {
                            linewise = true;
                        }
                    }
                }
                if linewise {
                    return Some((
                        b.line_start(b.line_of(a)),
                        b.line_end(b.line_of(to)),
                        true,
                        false,
                    ));
                }
                Some((a, to, false, false))
            }
        }
    }

    fn exec_operator(&mut self, st: &mut BufState, cmd: &Command, op: Op, target: &Target) {
        if let Target::Motion(Motion::Search { pattern, forward }) = target {
            if pattern.is_empty() {
                self.open_cmdline(
                    st,
                    if *forward { '/' } else { '?' },
                    Some(cmd.clone()),
                    None,
                );
                return;
            }
        }
        let Some((from, to, linewise, block_lines)) =
            self.operator_range(st, op, target, cmd.count)
        else {
            self.bell();
            return;
        };
        if op_changes_text(op) {
            self.last_change = Some(LastChange::Cmd {
                cmd: cmd.clone(),
                insert: None,
            });
        }
        self.apply_operator(
            st,
            op,
            cmd.reg,
            from,
            to,
            linewise,
            block_lines,
            Some(cmd.clone()),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_operator(
        &mut self,
        st: &mut BufState,
        op: Op,
        reg: Option<char>,
        from: usize,
        to: usize,
        linewise: bool,
        block_lines: bool,
        cmd: Option<Command>,
    ) {
        let clip = self.options.clipboard_unnamed();
        let (l1, l2) = if linewise {
            (st.buf.line_of(from), st.buf.line_of(to))
        } else if block_lines {
            (st.buf.line_of(from), st.buf.line_of(to.saturating_sub(1)))
        } else {
            (st.buf.line_of(from), st.buf.line_of(to))
        };
        let lines_text = |b: &Buffer| -> String {
            let mut t = b.slice(b.line_start(l1), b.line_end(l2));
            t.push('\n');
            t
        };
        match op {
            Op::Delete | Op::Yank if linewise || block_lines => {
                let text = lines_text(&st.buf);
                self.registers.store(
                    reg,
                    Register::new(text, RegKind::Line),
                    op == Op::Yank,
                    clip,
                );
                if op == Op::Yank {
                    let line = st.line();
                    if l1 < line {
                        let col = st.buf.col_of(st.cursor);
                        st.cursor = st.buf.pos_at(l1, col, false);
                    }
                    if l2 - l1 >= 2 {
                        self.message(format!("{} lines yanked", l2 - l1 + 1));
                    }
                    return;
                }
                self.delete_lines(st, l1, l2);
            }
            Op::Delete | Op::Yank => {
                let text = st.buf.slice(from, to);
                self.registers.store(
                    reg,
                    Register::new(text, RegKind::Char),
                    op == Op::Yank,
                    clip,
                );
                if op == Op::Delete {
                    self.edit(st, from, to, "");
                }
                st.cursor = from;
            }
            Op::Change => {
                if linewise {
                    let text = lines_text(&st.buf);
                    self.registers
                        .store(reg, Register::new(text, RegKind::Line), false, clip);
                    let indent = st.buf.indent_of(l1);
                    let (s, e) = (st.buf.line_start(l1), st.buf.line_end(l2));
                    self.edit(st, s, e, &indent);
                    st.cursor = s + indent.chars().count();
                } else if block_lines {
                    let text = lines_text(&st.buf);
                    self.registers
                        .store(reg, Register::new(text, RegKind::Line), false, clip);
                    let indent = st.buf.indent_of(l1);
                    let e = st.buf.line_end(l2);
                    self.edit(st, from, e, &indent);
                    st.cursor = from + indent.chars().count();
                } else {
                    let text = st.buf.slice(from, to);
                    self.registers
                        .store(reg, Register::new(text, RegKind::Char), false, clip);
                    self.edit(st, from, to, "");
                    st.cursor = from;
                }
                let n = 1;
                self.start_insert(st, st.cursor, cmd.map(|c| (c, n)));
            }
            Op::Indent | Op::Outdent => {
                self.shift_lines(st, l1, l2, op == Op::Indent, 1);
                st.cursor = st.buf.first_non_blank(l1);
            }
            Op::AutoIndent => {
                let b = &st.buf;
                let caret = b.to_utf16(b.first_non_blank(l1));
                let (f, t) = (b.to_utf16(b.line_start(l1)), b.to_utf16(b.line_end(l2)));
                self.effect(Effect::ActionOnRange {
                    id: "AutoIndentLines".into(),
                    from: f,
                    to: t,
                    caret,
                });
                st.collapse_next = true;
            }
            Op::ToggleCase | Op::Lower | Op::Upper | Op::Rot13 => {
                let (a, z) = if linewise {
                    (st.buf.line_start(l1), st.buf.line_end(l2))
                } else {
                    (from, to)
                };
                let old = st.buf.slice(a, z);
                let new = convert_case(&old, op);
                if new != old {
                    self.edit(st, a, z, &new);
                }
                if !linewise {
                    st.cursor = from;
                }
            }
        }
    }

    pub(crate) fn delete_lines(&mut self, st: &mut BufState, l1: usize, l2: usize) {
        let b = &st.buf;
        let last = b.line_count() - 1;
        let (from, to) = if l2 < last {
            (b.line_start(l1), b.line_start(l2 + 1))
        } else if l1 > 0 {
            (b.line_end(l1 - 1), b.len())
        } else {
            (0, b.len())
        };
        self.edit(st, from, to, "");
        let line = l1.min(st.buf.line_count() - 1);
        st.cursor = st.buf.first_non_blank(line);
        if l2 - l1 >= 2 {
            self.message(format!("{} fewer lines", l2 - l1 + 1));
        }
    }

    fn shift_width(&self, st: &BufState) -> (usize, bool) {
        let unit = &st.indent_unit;
        let tabs = self
            .options
            .expandtab
            .map(|e| !e)
            .unwrap_or(unit.contains('\t'));
        let tabstop = self.options.tabstop.unwrap_or(4).max(1);
        let sw = self
            .options
            .shiftwidth
            .filter(|n| *n > 0)
            .unwrap_or(if unit.contains('\t') {
                tabstop
            } else {
                unit.chars().count().max(1)
            });
        (sw, tabs)
    }

    pub(crate) fn shift_lines(
        &mut self,
        st: &mut BufState,
        l1: usize,
        l2: usize,
        right: bool,
        times: usize,
    ) {
        let (sw, tabs) = self.shift_width(st);
        let tabstop = self.options.tabstop.unwrap_or(4).max(1);
        for l in (l1..=l2).rev() {
            let b = &st.buf;
            let s = b.line_start(l);
            let indent = b.indent_of(l);
            if right && b.line_len(l) == 0 {
                continue;
            }
            let width: usize = indent
                .chars()
                .map(|c| if c == '\t' { tabstop } else { 1 })
                .sum();
            let new_width = if right {
                width + sw * times
            } else {
                width.saturating_sub(sw * times)
            };
            let new_indent = if tabs {
                format!(
                    "{}{}",
                    "\t".repeat(new_width / tabstop),
                    " ".repeat(new_width % tabstop)
                )
            } else {
                " ".repeat(new_width)
            };
            if new_indent != indent {
                self.edit(st, s, s + indent.chars().count(), &new_indent);
            }
        }
    }

    // ---- Insert mode

    fn start_insert(&mut self, st: &mut BufState, pos: usize, cmd: Option<(Command, usize)>) {
        st.cursor = pos.min(st.buf.len());
        st.mode = Mode::Insert;
        st.insert = Some(InsertSession {
            start: st.cursor,
            end: st.cursor,
            text: String::new(),
        });
        st.insert_cmd = cmd;
        st.curswant = None;
    }

    fn insert_key(&mut self, st: &mut BufState, k: &str, native: bool) {
        if st.insert_reg_pending {
            st.insert_reg_pending = false;
            if let Some(c) = char_of(k) {
                match self.registers.get(Some(c), false).or_else(|| {
                    (c == '"')
                        .then(|| self.registers.get(None, self.options.clipboard_unnamed()))
                        .flatten()
                }) {
                    Some(r) => self.insert_text(st, &r.text),
                    None => self.bell(),
                }
            }
            return;
        }
        match k {
            "<Esc>" | "<C-c>" => self.finish_insert(st),
            "<C-w>" => {
                let b = &st.buf;
                let c = st.cursor;
                let s = b.line_start(b.line_of(c));
                if c == s {
                    if c > 0 {
                        self.edit(st, c - 1, c, "");
                        st.cursor = c - 1;
                    }
                    return;
                }
                let mut i = c;
                while i > s && matches!(b.at(i - 1), Some(' ' | '\t')) {
                    i -= 1;
                }
                if i > s {
                    let cls = class_of(b.at(i - 1).unwrap(), false);
                    while i > s && class_of(b.at(i - 1).unwrap(), false) == cls {
                        i -= 1;
                    }
                }
                self.edit(st, i, c, "");
                st.cursor = i;
            }
            "<C-u>" => {
                let b = &st.buf;
                let c = st.cursor;
                let line = b.line_of(c);
                let fnb = b.first_non_blank(line);
                let from = if c > fnb { fnb } else { b.line_start(line) };
                self.edit(st, from, c, "");
                st.cursor = from;
            }
            "<C-h>" => self.insert_internal(st, "<BS>"),
            "<C-j>" => self.insert_internal(st, "<CR>"),
            "<C-r>" => st.insert_reg_pending = true,
            "<C-o>" => {
                st.one_command = true;
                st.mode = Mode::Normal;
            }
            "<C-t>" | "<C-d>" => {
                let line = st.line();
                let before = st.buf.line_len(line);
                let col = st.buf.col_of(st.cursor);
                self.shift_lines(st, line, line, k == "<C-t>", 1);
                let after = st.buf.line_len(line);
                let col = (col + after).saturating_sub(before);
                st.cursor = st.buf.pos_at(line, col, true);
            }
            "<C-e>" | "<C-y>" => {
                let line = st.line();
                let other = if k == "<C-e>" {
                    line + 1
                } else {
                    line.wrapping_sub(1)
                };
                if other >= st.buf.line_count() {
                    self.bell();
                    return;
                }
                let col = st.buf.col_of(st.cursor);
                match (col < st.buf.line_len(other))
                    .then(|| st.buf.at(st.buf.line_start(other) + col))
                    .flatten()
                {
                    Some(ch) => self.insert_text(st, &ch.to_string()),
                    None => self.bell(),
                }
            }
            "<Insert>" => {
                st.mode = Mode::Replace;
                st.replace_stack.clear();
            }
            _ => {
                if native {
                    self.out.passthrough = true;
                } else {
                    self.insert_internal(st, k);
                }
            }
        }
    }

    /// Apply an Insert mode key ourselves (macros, mappings, repeat).
    fn insert_internal(&mut self, st: &mut BufState, k: &str) {
        let c = st.cursor;
        match k {
            "<CR>" | "<C-m>" => {
                let indent = st.buf.indent_of(st.line());
                self.insert_text(st, &format!("\n{indent}"));
            }
            "<BS>" => {
                if c > 0 {
                    self.edit(st, c - 1, c, "");
                    st.cursor = c - 1;
                }
            }
            "<Del>" => {
                if c < st.buf.len() {
                    self.edit(st, c, c + 1, "");
                }
            }
            "<Tab>" => {
                let unit = if self.options.expandtab == Some(true)
                    || (self.options.expandtab.is_none() && !st.indent_unit.contains('\t'))
                {
                    " ".repeat(self.shift_width(st).0)
                } else {
                    "\t".into()
                };
                self.insert_text(st, &unit);
            }
            "<Left>" => st.cursor = c.saturating_sub(1).max(st.buf.line_start(st.line())),
            "<Right>" => st.cursor = (c + 1).min(st.buf.line_end(st.line())),
            "<Up>" | "<Down>" => {
                let line = st.line();
                let col = st.buf.col_of(c);
                let tl = if k == "<Up>" {
                    line.saturating_sub(1)
                } else {
                    (line + 1).min(st.buf.line_count() - 1)
                };
                st.cursor = st.buf.pos_at(tl, col, true);
            }
            "<Home>" => st.cursor = st.buf.line_start(st.line()),
            "<End>" => st.cursor = st.buf.line_end(st.line()),
            _ => {
                if let Some(ch) = char_of(k) {
                    if st.mode == Mode::Replace {
                        self.replace_char_at_cursor(st, ch);
                    } else {
                        self.insert_text(st, &ch.to_string());
                    }
                }
            }
        }
    }

    pub(crate) fn insert_text(&mut self, st: &mut BufState, text: &str) {
        let c = st.cursor;
        self.edit(st, c, c, text);
        st.cursor = c + text.chars().count();
    }

    fn finish_insert(&mut self, st: &mut BufState) {
        let replace = st.mode == Mode::Replace;
        let text = st.insert.take().map(|s| s.text).unwrap_or_default();
        if let Some((cmd, n)) = st.insert_cmd.take() {
            if n > 1 && !text.is_empty() {
                let below = matches!(
                    cmd.kind,
                    CmdKind::Action(Action::Insert(InsertAt::Below | InsertAt::Above))
                );
                for _ in 1..n {
                    if below {
                        let line = st.line();
                        let indent = st.buf.indent_of(line);
                        let at = st.buf.line_end(line);
                        self.edit(st, at, at, &format!("\n{indent}{text}"));
                        st.cursor = at + 1 + indent.chars().count() + text.chars().count();
                    } else if replace {
                        for ch in text.chars() {
                            self.replace_char_at_cursor(st, ch);
                        }
                    } else {
                        self.insert_text(st, &text);
                    }
                }
            }
            self.last_change = Some(LastChange::Cmd {
                cmd,
                insert: Some(text.clone()),
            });
        } else if let Some((op, kind, lines, chars)) = st.visual_change.take() {
            self.last_change = Some(LastChange::Visual {
                op,
                kind,
                lines,
                chars,
                insert: Some(text.clone()),
            });
        }
        if let Some(bi) = st.block_insert.take() {
            if !text.is_empty() && !text.contains('\n') {
                for l in (bi.first + 1..=bi.last).rev() {
                    let b = &st.buf;
                    let (s, len) = (b.line_start(l), b.line_len(l));
                    if bi.append_eol {
                        self.edit(st, s + len, s + len, &text);
                    } else if len >= bi.col {
                        self.edit(st, s + bi.col, s + bi.col, &text);
                    }
                }
                let s = st.buf.line_start(bi.first);
                st.cursor = s.saturating_add(bi.col).min(st.buf.line_end(bi.first));
            }
        }
        self.registers.set_special('.', text);
        st.marks.insert('^', st.cursor);
        st.mode = Mode::Normal;
        st.replace_stack.clear();
        st.one_command = false;
        let s = st.buf.line_start(st.line());
        if st.cursor > s {
            st.cursor -= 1;
        }
    }

    fn replace_char_at_cursor(&mut self, st: &mut BufState, ch: char) {
        let c = st.cursor;
        if c < st.buf.line_end(st.line()) {
            st.replace_stack.push(st.buf.at(c));
            self.edit(st, c, c + 1, &ch.to_string());
        } else {
            st.replace_stack.push(None);
            self.edit(st, c, c, &ch.to_string());
        }
        st.cursor = c + 1;
        if let Some(s) = st.insert.as_mut() {
            s.text.push(ch);
        }
    }

    fn replace_key(&mut self, st: &mut BufState, k: &str, native: bool) {
        match k {
            "<Esc>" | "<C-c>" => self.finish_insert(st),
            "<Insert>" => st.mode = Mode::Insert,
            "<BS>" | "<C-h>" => {
                let c = st.cursor;
                if c == 0 {
                    return;
                }
                match st.replace_stack.pop() {
                    Some(Some(orig)) => {
                        self.edit(st, c - 1, c, &orig.to_string());
                        if let Some(s) = st.insert.as_mut() {
                            s.text.pop();
                        }
                    }
                    Some(None) => {
                        self.edit(st, c - 1, c, "");
                        if let Some(s) = st.insert.as_mut() {
                            s.text.pop();
                        }
                    }
                    None => {}
                }
                st.cursor = c - 1;
            }
            "<CR>" => self.insert_text(st, "\n"),
            _ => match char_of(k) {
                Some(ch) => self.replace_char_at_cursor(st, ch),
                None if native => self.out.passthrough = true,
                None => self.insert_internal(st, k),
            },
        }
    }

    // ---- Actions

    fn exec_action(&mut self, st: &mut BufState, cmd: &Command, a: Action) {
        let n = cmd.count1();
        let clip = self.options.clipboard_unnamed();
        let line = st.line();
        let b = &st.buf;
        let repeatable = |s: &mut Self| {
            s.last_change = Some(LastChange::Cmd {
                cmd: cmd.clone(),
                insert: None,
            })
        };
        match a {
            Action::Insert(at) => {
                let pos = match at {
                    InsertAt::Before => st.cursor,
                    InsertAt::After => (st.cursor + 1).min(b.line_end(line)),
                    InsertAt::LineStart => b.first_non_blank(line),
                    InsertAt::LineStartCol0 => b.line_start(line),
                    InsertAt::LineEnd => b.line_end(line),
                    InsertAt::LastInsert => st.marks.get(&'^').copied().unwrap_or(st.cursor),
                    InsertAt::Below => {
                        let indent = b.indent_of(line);
                        let at = b.line_end(line);
                        self.edit(st, at, at, &format!("\n{indent}"));
                        at + 1 + indent.chars().count()
                    }
                    InsertAt::Above => {
                        let indent = b.indent_of(line);
                        let at = b.line_start(line);
                        self.edit(st, at, at, &format!("{indent}\n"));
                        at + indent.chars().count()
                    }
                };
                self.start_insert(st, pos, Some((cmd.clone(), n)));
            }
            Action::Visual(kind) => match st.mode {
                Mode::Visual(k) if k == kind => self.exit_visual(st),
                Mode::Visual(_) => st.mode = Mode::Visual(kind),
                _ => {
                    st.anchor = st.cursor;
                    st.mode = Mode::Visual(kind);
                    if kind != VisualKind::Block {
                        st.curswant = None;
                    }
                }
            },
            Action::DeleteChar | Action::Substitute => {
                let e = b.line_end(line);
                if st.cursor >= e {
                    if a == Action::Substitute {
                        self.start_insert(st, st.cursor, Some((cmd.clone(), 1)));
                    }
                    return;
                }
                let to = (st.cursor + n).min(e);
                let op = if a == Action::DeleteChar {
                    Op::Delete
                } else {
                    Op::Change
                };
                if op == Op::Delete {
                    repeatable(self);
                }
                self.apply_operator(
                    st,
                    op,
                    cmd.reg,
                    st.cursor,
                    to,
                    false,
                    false,
                    Some(cmd.clone()),
                );
            }
            Action::DeleteCharBack => {
                let s = b.line_start(line);
                if st.cursor == s {
                    self.bell();
                    return;
                }
                let from = st.cursor.saturating_sub(n).max(s);
                repeatable(self);
                self.apply_operator(st, Op::Delete, cmd.reg, from, st.cursor, false, false, None);
            }
            Action::SubstituteLine => {
                let target = Target::Lines;
                self.exec_operator(st, cmd, Op::Change, &target);
            }
            Action::ChangeToEnd | Action::DeleteToEnd => {
                let l2 = (line + n - 1).min(b.line_count() - 1);
                let to = b.line_end(l2);
                let op = if a == Action::ChangeToEnd {
                    Op::Change
                } else {
                    Op::Delete
                };
                if op == Op::Delete {
                    repeatable(self);
                }
                self.apply_operator(
                    st,
                    op,
                    cmd.reg,
                    st.cursor,
                    to,
                    false,
                    false,
                    Some(cmd.clone()),
                );
            }
            Action::YankLine => {
                let l2 = (line + n - 1).min(b.line_count() - 1);
                let (s, e) = (b.line_start(line), b.line_end(l2));
                self.apply_operator(st, Op::Yank, cmd.reg, s, e, true, false, None);
            }
            Action::Put {
                before,
                cursor_after,
            } => {
                repeatable(self);
                self.put(st, cmd.reg, before, cursor_after, n, clip);
            }
            Action::Join { spaces } => {
                repeatable(self);
                let count = n.max(2) - 1;
                if self.options.ideajoin && spaces {
                    for _ in 0..count {
                        self.effect(Effect::Action {
                            id: "EditorJoinLines".into(),
                        });
                    }
                    return;
                }
                self.join_lines(st, line, count, spaces);
            }
            Action::ReplaceChar(k) => {
                let e = b.line_end(line);
                if st.cursor + n > e {
                    self.bell();
                    return;
                }
                repeatable(self);
                let c = st.cursor;
                if k == "<CR>" {
                    self.edit(st, c, c + n, "\n");
                    st.cursor = c + 1;
                    return;
                }
                let ch = if k == "<Tab>" {
                    Some('\t')
                } else {
                    char_of(&k)
                };
                let Some(ch) = ch else {
                    return;
                };
                self.edit(st, c, c + n, &ch.to_string().repeat(n));
                st.cursor = c + n - 1;
            }
            Action::ReplaceMode => {
                self.start_insert(st, st.cursor, Some((cmd.clone(), n)));
                st.mode = Mode::Replace;
                st.replace_stack.clear();
            }
            Action::Undo | Action::Redo => {
                self.effect(if a == Action::Undo {
                    Effect::Undo { count: n }
                } else {
                    Effect::Redo { count: n }
                });
                st.collapse_next = true;
            }
            Action::Repeat => self.repeat_last(st, cmd.count),
            Action::ToggleCaseChar => {
                let e = b.line_end(line);
                if st.cursor >= e {
                    return;
                }
                repeatable(self);
                let c = st.cursor;
                let to = (c + n).min(e);
                let old = b.slice(c, to);
                let new = convert_case(&old, Op::ToggleCase);
                if new != old {
                    self.edit(st, c, to, &new);
                }
                st.cursor = to.min(st.buf.line_last(line));
            }
            Action::Record(c) => {
                let reg = if c.is_ascii_uppercase() {
                    c
                } else {
                    c.to_ascii_lowercase()
                };
                self.recording = Some((reg, Vec::new()));
            }
            Action::StopRecording => {
                if let Some((reg, mut keys)) = self.recording.take() {
                    if keys.last().map(String::as_str) == Some("q") {
                        keys.pop();
                    }
                    let text = keys::keys_to_text(&keys);
                    if reg.is_ascii_uppercase() {
                        let old = self
                            .registers
                            .get(Some(reg), false)
                            .map(|r| r.text)
                            .unwrap_or_default();
                        self.registers.set_special(reg, format!("{old}{text}"));
                    } else {
                        self.registers.set_special(reg, text);
                    }
                }
            }
            Action::PlayMacro(c) => {
                let reg = if c == '@' {
                    match self.last_macro {
                        Some(r) => r,
                        None => {
                            self.error("E748: No previously used register");
                            return;
                        }
                    }
                } else {
                    c
                };
                if reg == ':' {
                    if let Some(ex) = self.last_ex.clone() {
                        for _ in 0..n {
                            self.ex(st, &ex);
                        }
                    }
                    self.last_macro = Some(':');
                    return;
                }
                let Some(r) = self.registers.get(Some(reg), clip && reg == '"') else {
                    self.bell();
                    return;
                };
                self.last_macro = Some(reg);
                let keys = keys::parse_keys(&r.text, &self.leader.clone());
                for _ in 0..n {
                    for k in keys.iter().rev() {
                        self.typeahead.push_front(Typed {
                            key: k.clone(),
                            remap: true,
                            event: None,
                        });
                    }
                }
            }
            Action::SetMark(c) => {
                let c = c.to_ascii_lowercase();
                let c = if c == '`' { '\'' } else { c };
                st.marks.insert(c, st.cursor);
            }
            Action::CmdLine(kind) => {
                let text = if st.is_visual() {
                    "'<,'>".to_string()
                } else if let Some(c) = cmd.count {
                    if c > 1 {
                        format!(".,.+{}", c - 1)
                    } else {
                        ".".into()
                    }
                } else {
                    String::new()
                };
                if st.is_visual() {
                    self.exit_visual(st);
                }
                self.open_cmdline(st, kind, None, None);
                if let Some(cl) = st.cmdline.as_mut() {
                    cl.text = text;
                }
            }
            Action::Increment(delta) => {
                if st.is_visual() {
                    self.visual_increment(st, delta * n as i64);
                } else if self
                    .increment(st, line, st.cursor, delta * n as i64)
                    .is_some()
                {
                    repeatable(self);
                } else {
                    self.bell();
                }
            }
            Action::Scroll(s) => self.scroll(st, s, cmd.count),
            Action::Reselect => {
                let Some((a, c, k)) = st.last_visual else {
                    self.bell();
                    return;
                };
                if st.is_visual() {
                    self.exit_visual(st);
                }
                st.anchor = a.min(st.buf.len());
                st.cursor = c.min(st.buf.len());
                st.mode = Mode::Visual(k);
            }
            Action::IdeAction(id) => {
                if st.is_visual() && id != "GotoDeclaration" {
                    self.exit_visual(st);
                }
                self.effect(Effect::Action { id: id.into() });
            }
            Action::Window(w) => {
                let id = match w.as_str() {
                    "w" | "<C-w>" | "l" | "j" | "<Right>" | "<Down>" | "<C-l>" | "<C-j>" => {
                        "NextSplitter"
                    }
                    "W" | "h" | "k" | "p" | "<Left>" | "<Up>" | "<C-h>" | "<C-k>" | "<C-p>" => {
                        "PrevSplitter"
                    }
                    "v" | "<C-v>" => "SplitVertically",
                    "s" | "S" | "<C-s>" => "SplitHorizontally",
                    "c" | "q" | "<C-c>" => "Unsplit",
                    "o" | "<C-o>" => "UnsplitAll",
                    _ => {
                        self.bell();
                        return;
                    }
                };
                self.effect(Effect::Action { id: id.into() });
            }
            Action::Escape => {
                if st.is_visual() {
                    self.exit_visual(st);
                }
                st.pending.clear();
            }
            Action::RepeatSubstitute => self.ex(st, "s"),
            Action::SaveQuit => {
                self.effect(Effect::Action {
                    id: "SaveAll".into(),
                });
                self.effect(Effect::Action {
                    id: "CloseContent".into(),
                });
            }
            Action::Quit => self.effect(Effect::Action {
                id: "CloseContent".into(),
            }),
            Action::SwapEnds => std::mem::swap(&mut st.anchor, &mut st.cursor),
            Action::VisualInsert { append } => self.visual_insert(st, append),
            Action::VisualReplace(k) => self.visual_replace(st, &k),
            Action::VisualPut { before } => self.visual_put(st, cmd.reg, before, clip),
            Action::VisualJoin { spaces } => {
                let (l1, l2) = ordered(st.buf.line_of(st.anchor), st.buf.line_of(st.cursor));
                self.exit_visual(st);
                self.join_lines(st, l1, (l2 - l1).max(1), spaces);
            }
            Action::VisualLineOp(op) => {
                if let Mode::Visual(k) = st.mode {
                    if k != VisualKind::Block || op != Op::Delete {
                        st.mode = Mode::Visual(VisualKind::Line);
                    } else {
                        st.curswant = Some(usize::MAX);
                    }
                }
                self.visual_op(st, cmd, op);
            }
        }
    }

    fn open_cmdline(
        &mut self,
        st: &mut BufState,
        kind: char,
        op: Option<Command>,
        count: Option<usize>,
    ) {
        st.cmdline = Some(CmdLine {
            kind,
            text: String::new(),
            origin: st.cursor,
            op,
            count,
            reg_pending: false,
            history: None,
        });
    }

    fn put(
        &mut self,
        st: &mut BufState,
        reg: Option<char>,
        before: bool,
        cursor_after: bool,
        n: usize,
        clip: bool,
    ) {
        let Some(r) = self.registers.get(reg, clip) else {
            self.error(format!("E353: Nothing in register {}", reg.unwrap_or('"')));
            return;
        };
        let line = st.line();
        let b = &st.buf;
        match r.kind {
            RegKind::Line => {
                let mut text = r.text.clone();
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                let text = text.repeat(n);
                let count = text.matches('\n').count();
                if before {
                    let at = b.line_start(line);
                    self.edit(st, at, at, &text);
                    st.cursor = if cursor_after {
                        st.buf.line_start(line + count)
                    } else {
                        st.buf.first_non_blank(line)
                    };
                } else if line + 1 < b.line_count() {
                    let at = b.line_start(line + 1);
                    self.edit(st, at, at, &text);
                    st.cursor = if cursor_after {
                        st.buf
                            .line_start((line + 1 + count).min(st.buf.line_count() - 1))
                    } else {
                        st.buf.first_non_blank(line + 1)
                    };
                } else {
                    let at = b.len();
                    let body = &text[..text.len() - 1];
                    self.edit(st, at, at, &format!("\n{body}"));
                    st.cursor = st.buf.first_non_blank(line + 1);
                }
            }
            RegKind::Block => {
                let rows: Vec<&str> = r.text.split('\n').collect();
                let col = if before {
                    b.col_of(st.cursor)
                } else {
                    (b.col_of(st.cursor) + 1).min(b.line_len(line))
                };
                for (i, row) in rows.iter().enumerate().rev() {
                    let l = line + i;
                    let row = row.repeat(n);
                    if l >= st.buf.line_count() {
                        let at = st.buf.len();
                        self.edit(st, at, at, &format!("\n{}{row}", " ".repeat(col)));
                        continue;
                    }
                    let (s, len) = (st.buf.line_start(l), st.buf.line_len(l));
                    if len < col {
                        self.edit(
                            st,
                            s + len,
                            s + len,
                            &format!("{}{row}", " ".repeat(col - len)),
                        );
                    } else {
                        self.edit(st, s + col, s + col, &row);
                    }
                }
                st.cursor = st.buf.line_start(line) + col;
            }
            RegKind::Char => {
                let text = r.text.repeat(n);
                let len = text.chars().count();
                let at = if before || b.line_len(line) == 0 {
                    st.cursor
                } else {
                    (st.cursor + 1).min(b.line_end(line))
                };
                self.edit(st, at, at, &text);
                st.cursor = if cursor_after {
                    at + len
                } else if text.contains('\n') {
                    at
                } else {
                    (at + len).saturating_sub(1).max(at)
                };
            }
        }
    }

    pub(crate) fn join_lines(
        &mut self,
        st: &mut BufState,
        line: usize,
        count: usize,
        spaces: bool,
    ) {
        let mut done = 0;
        for _ in 0..count {
            let b = &st.buf;
            if line + 1 >= b.line_count() {
                break;
            }
            let e = b.line_end(line);
            let next = line + 1;
            if spaces {
                let fnb = b.first_non_blank(next);
                let next_empty = fnb == b.line_end(next);
                let ends_blank = e > b.line_start(line) && matches!(b.at(e - 1), Some(' ' | '\t'));
                let sep = if next_empty
                    || ends_blank
                    || b.at(fnb) == Some(')')
                    || b.line_len(line) == 0
                {
                    ""
                } else {
                    " "
                };
                self.edit(st, e, fnb, sep);
                st.cursor = if sep.is_empty() {
                    e.saturating_sub(usize::from(next_empty && e > st.buf.line_start(line)))
                } else {
                    e
                };
            } else {
                self.edit(st, e, e + 1, "");
                st.cursor = e.saturating_sub(1).max(st.buf.line_start(line));
            }
            done += 1;
        }
        if done == 0 {
            self.bell();
        }
    }

    /// Add `delta` to the number at or after `pos` on `line`; returns its new end.
    fn increment(
        &mut self,
        st: &mut BufState,
        line: usize,
        pos: usize,
        delta: i64,
    ) -> Option<usize> {
        let b = &st.buf;
        let (s, e) = (b.line_start(line), b.line_end(line));
        let ch = b.chars();
        // Inside a number: back to its start.
        let mut i = pos.max(s);
        while i > s && ch[i - 1].is_ascii_digit() && ch.get(i).is_some_and(|c| c.is_ascii_digit()) {
            i -= 1;
        }
        while i < e && !ch[i].is_ascii_digit() {
            i += 1;
        }
        if i >= e {
            return None;
        }
        // Hexadecimal.
        if i + 1 < e && ch[i] == '0' && matches!(ch[i + 1], 'x' | 'X') {
            let mut j = i + 2;
            while j < e && ch[j].is_ascii_hexdigit() {
                j += 1;
            }
            if j > i + 2 {
                let digits = b.slice(i + 2, j);
                let v = i64::from_str_radix(&digits, 16).ok()?;
                let upper = digits.chars().any(|c| c.is_ascii_uppercase());
                let nv = v.wrapping_add(delta).max(0);
                let mut text = format!("{:0width$x}", nv, width = digits.len());
                if upper {
                    text = text.to_uppercase();
                }
                self.edit(st, i + 2, j, &text);
                st.cursor = i + 2 + text.len() - 1;
                return Some(st.cursor);
            }
        }
        let start = if i > s && ch[i - 1] == '-' { i - 1 } else { i };
        let mut j = i;
        while j < e && ch[j].is_ascii_digit() {
            j += 1;
        }
        let v: i64 = b.slice(start, j).parse().ok()?;
        let text = v.saturating_add(delta).to_string();
        self.edit(st, start, j, &text);
        st.cursor = start + text.len() - 1;
        Some(st.cursor)
    }

    fn visual_increment(&mut self, st: &mut BufState, delta: i64) {
        let (l1, l2) = ordered(st.buf.line_of(st.anchor), st.buf.line_of(st.cursor));
        let from = if st.mode == Mode::Visual(VisualKind::Line) {
            None
        } else {
            Some(st.anchor.min(st.cursor))
        };
        self.exit_visual(st);
        for l in l1..=l2 {
            let pos = if l == l1 {
                from.unwrap_or(st.buf.line_start(l))
            } else {
                st.buf.line_start(l)
            };
            self.increment(st, l, pos, delta);
        }
        st.cursor = st.buf.first_non_blank(l1);
    }

    fn scroll(&mut self, st: &mut BufState, s: Scroll, count: Option<usize>) {
        let b = &st.buf;
        let last = b.line_count() - 1;
        let vp = st.viewport;
        let height = vp.bottom.saturating_sub(vp.top) + 1;
        let half = (height / 2).max(1);
        let line = st.line();
        let set_line = |st: &mut BufState, l: usize, keep_col: bool| {
            let l = l.min(last);
            st.cursor = if keep_col {
                let col = st.buf.col_of(st.cursor);
                st.buf.pos_at(l, col, false)
            } else {
                st.buf.first_non_blank(l)
            };
        };
        match s {
            Scroll::HalfDown | Scroll::HalfUp => {
                let lines = count.unwrap_or(half);
                if s == Scroll::HalfDown {
                    if line == last {
                        self.bell();
                        return;
                    }
                    let top = (vp.top + lines).min(last);
                    set_line(st, line + lines, false);
                    self.effect(Effect::ScrollTo { line: top });
                } else {
                    if line == 0 {
                        self.bell();
                        return;
                    }
                    let top = vp.top.saturating_sub(lines);
                    set_line(st, line.saturating_sub(lines), false);
                    self.effect(Effect::ScrollTo { line: top });
                }
            }
            Scroll::PageDown => {
                let pages = count.unwrap_or(1);
                let top = (vp.top + pages * height.saturating_sub(2).max(1)).min(last);
                set_line(st, line.max(top), false);
                self.effect(Effect::ScrollTo { line: top });
            }
            Scroll::PageUp => {
                let pages = count.unwrap_or(1);
                let top = vp
                    .top
                    .saturating_sub(pages * height.saturating_sub(2).max(1));
                let bottom = top + height - 1;
                set_line(st, line.min(bottom), false);
                self.effect(Effect::ScrollTo { line: top });
            }
            Scroll::LineDown | Scroll::LineUp => {
                let n = count.unwrap_or(1);
                let top = if s == Scroll::LineDown {
                    (vp.top + n).min(last)
                } else {
                    vp.top.saturating_sub(n)
                };
                let bottom = top + height - 1;
                if line < top {
                    set_line(st, top, true);
                } else if line > bottom {
                    set_line(st, bottom, true);
                }
                self.effect(Effect::ScrollTo { line: top });
            }
            Scroll::CursorTop
            | Scroll::CursorCenter
            | Scroll::CursorBottom
            | Scroll::CursorTopFirst
            | Scroll::CursorCenterFirst
            | Scroll::CursorBottomFirst => {
                if let Some(c) = count {
                    set_line(st, c.saturating_sub(1), true);
                }
                if matches!(
                    s,
                    Scroll::CursorTopFirst | Scroll::CursorCenterFirst | Scroll::CursorBottomFirst
                ) {
                    let l = st.line();
                    set_line(st, l, false);
                }
                let align = match s {
                    Scroll::CursorTop | Scroll::CursorTopFirst => "start",
                    Scroll::CursorCenter | Scroll::CursorCenterFirst => "center",
                    _ => "end",
                };
                self.effect(Effect::Reveal {
                    pos: st.buf.to_utf16(st.cursor),
                    align: align.into(),
                });
            }
        }
    }

    fn repeat_last(&mut self, st: &mut BufState, count: Option<usize>) {
        let Some(lc) = self.last_change.clone() else {
            self.bell();
            return;
        };
        self.replaying += 1;
        match lc {
            LastChange::Cmd { mut cmd, insert } => {
                if count.is_some() {
                    cmd.count = count;
                }
                // Numbered registers advance on repeat ("1p... → "2p).
                if let Some(r) = cmd
                    .reg
                    .filter(|r| r.is_ascii_digit() && *r != '0' && *r != '9')
                {
                    cmd.reg = Some((r as u8 + 1) as char);
                }
                self.exec(st, cmd.clone());
                if matches!(st.mode, Mode::Insert | Mode::Replace) {
                    let text = insert.clone().unwrap_or_default();
                    if st.mode == Mode::Replace {
                        for ch in text.chars() {
                            self.replace_char_at_cursor(st, ch);
                        }
                    } else {
                        self.insert_text(st, &text);
                    }
                    self.finish_insert(st);
                }
                self.last_change = Some(LastChange::Cmd { cmd, insert });
            }
            LastChange::Visual {
                op,
                kind,
                lines,
                chars,
                insert,
            } => {
                let b = &st.buf;
                let line = st.line();
                let last = b.line_count() - 1;
                st.anchor = st.cursor;
                match kind {
                    VisualKind::Line => st.cursor = b.line_start((line + lines).min(last)),
                    _ if lines == 0 => {
                        st.cursor = (st.cursor + chars.saturating_sub(1)).min(b.line_last(line))
                    }
                    _ => {
                        let col = b.col_of(st.cursor);
                        st.cursor = b.pos_at(
                            (line + lines).min(last),
                            col + chars.saturating_sub(1),
                            false,
                        );
                    }
                }
                st.mode = Mode::Visual(kind);
                let cmd = Command {
                    reg: None,
                    count: None,
                    kind: CmdKind::VisualOp(op),
                };
                self.visual_op(st, &cmd, op);
                if st.mode == Mode::Insert {
                    let text = insert.clone().unwrap_or_default();
                    self.insert_text(st, &text);
                    self.finish_insert(st);
                }
                self.last_change = Some(LastChange::Visual {
                    op,
                    kind,
                    lines,
                    chars,
                    insert,
                });
            }
        }
        self.replaying -= 1;
    }

    // ---- Visual mode

    fn exit_visual(&mut self, st: &mut BufState) {
        if let Mode::Visual(k) = st.mode {
            let (a, z) = ordered(st.anchor, st.cursor);
            st.marks.insert('<', a);
            st.marks.insert('>', z);
            st.last_visual = Some((st.anchor, st.cursor, k));
            st.mode = Mode::Normal;
            if st.curswant == Some(usize::MAX) {
                st.curswant = None;
            }
        }
    }

    fn visual_object(&mut self, st: &mut BufState, inner: bool, ch: char, n: usize) {
        let Some(r) = textobj::text_object(&st.buf, st.cursor, inner, ch, n) else {
            self.bell();
            return;
        };
        if r.to <= r.from {
            return;
        }
        if st.anchor == st.cursor || !matches!(ch, 'w' | 'W') {
            st.anchor = r.from;
        }
        st.cursor = r.to - 1;
        if r.linewise {
            st.mode = Mode::Visual(VisualKind::Line);
        } else if st.mode == Mode::Visual(VisualKind::Line)
            && !r.linewise
            && !matches!(ch, 'w' | 'W')
        {
            st.mode = Mode::Visual(VisualKind::Char);
        }
    }

    fn visual_op(&mut self, st: &mut BufState, cmd: &Command, op: Op) {
        let Mode::Visual(kind) = st.mode else {
            return;
        };
        let clip = self.options.clipboard_unnamed();
        let b = &st.buf;
        let (a, z) = ordered(st.anchor, st.cursor);
        let (l1, l2) = (b.line_of(a), b.line_of(z));
        let chars = if l1 == l2 { z - a + 1 } else { b.col_of(z) + 1 };
        let block = self.block_bounds(st);
        let block_col = b.col_of(a).min(b.col_of(z));
        self.exit_visual(st);
        if op_changes_text(op) && op != Op::AutoIndent {
            let rec = (op, kind, l2 - l1, chars);
            if op == Op::Change {
                st.visual_change = Some(rec);
            } else {
                self.last_change = Some(LastChange::Visual {
                    op,
                    kind,
                    lines: rec.2,
                    chars: rec.3,
                    insert: None,
                });
            }
        }
        match kind {
            VisualKind::Char => {
                let to = (z + 1).min(st.buf.len());
                match op {
                    Op::Indent | Op::Outdent => {
                        self.shift_lines(st, l1, l2, op == Op::Indent, cmd.count1());
                        st.cursor = st.buf.first_non_blank(l1);
                    }
                    _ => self.apply_operator(st, op, cmd.reg, a, to, false, false, None),
                }
            }
            VisualKind::Line => {
                let (s, e) = (st.buf.line_start(l1), st.buf.line_end(l2));
                match op {
                    Op::Indent | Op::Outdent => {
                        self.shift_lines(st, l1, l2, op == Op::Indent, cmd.count1());
                        st.cursor = st.buf.first_non_blank(l1);
                    }
                    Op::Yank => {
                        self.apply_operator(st, op, cmd.reg, s, e, true, false, None);
                        st.cursor =
                            st.buf.line_start(l1) + st.buf.col_of(a).min(st.buf.line_len(l1));
                    }
                    _ => self.apply_operator(st, op, cmd.reg, s, e, true, false, None),
                }
            }
            VisualKind::Block => {
                let (bl1, bl2, c1, c2) = block;
                match op {
                    Op::Indent | Op::Outdent => {
                        self.shift_lines(st, bl1, bl2, op == Op::Indent, cmd.count1());
                    }
                    Op::Delete | Op::Yank | Op::Change => {
                        let mut rows = Vec::new();
                        for l in bl1..=bl2 {
                            let b = &st.buf;
                            let s = b.line_start(l);
                            let e = b.line_end(l);
                            let from = (s + c1).min(e);
                            let to = if c2 == usize::MAX {
                                e
                            } else {
                                (s + c2 + 1).min(e)
                            };
                            rows.push((from, to.max(from)));
                        }
                        let text: Vec<String> =
                            rows.iter().map(|(f, t)| st.buf.slice(*f, *t)).collect();
                        self.registers.store(
                            cmd.reg,
                            Register::new(text.join("\n"), RegKind::Block),
                            op == Op::Yank,
                            clip,
                        );
                        if op != Op::Yank {
                            for (f, t) in rows.iter().rev() {
                                self.edit(st, *f, *t, "");
                            }
                        }
                        st.cursor = (st.buf.line_start(bl1) + c1).min(st.buf.line_end(bl1));
                        if op == Op::Change {
                            st.block_insert = Some(BlockInsert {
                                first: bl1,
                                last: bl2,
                                col: c1,
                                append_eol: false,
                            });
                            let pos = st.cursor;
                            self.start_insert(st, pos, None);
                        }
                    }
                    _ => {
                        for l in (bl1..=bl2).rev() {
                            let b = &st.buf;
                            let s = b.line_start(l);
                            let e = b.line_end(l);
                            let from = (s + c1).min(e);
                            let to = if c2 == usize::MAX {
                                e
                            } else {
                                (s + c2 + 1).min(e)
                            };
                            self.apply_operator(
                                st,
                                op,
                                None,
                                from,
                                to.max(from),
                                false,
                                false,
                                None,
                            );
                        }
                        st.cursor = (st.buf.line_start(bl1) + block_col).min(st.buf.line_end(bl1));
                    }
                }
            }
        }
    }

    fn visual_insert(&mut self, st: &mut BufState, append: bool) {
        let Mode::Visual(kind) = st.mode else {
            return;
        };
        let (a, z) = ordered(st.anchor, st.cursor);
        let block = self.block_bounds(st);
        self.exit_visual(st);
        match kind {
            VisualKind::Block => {
                let (l1, l2, c1, c2) = block;
                let eol = append && c2 == usize::MAX;
                let col = if append {
                    if eol {
                        usize::MAX
                    } else {
                        c2 + 1
                    }
                } else {
                    c1
                };
                let s = st.buf.line_start(l1);
                let pos = if eol {
                    st.buf.line_end(l1)
                } else {
                    (s + col).min(st.buf.line_end(l1))
                };
                if append && !eol {
                    // Pad short lines so the text lines up.
                    for l in (l1..=l2).rev() {
                        let (ls, len) = (st.buf.line_start(l), st.buf.line_len(l));
                        if len < col && l != l1 {
                            self.edit(st, ls + len, ls + len, &" ".repeat(col - len));
                        }
                    }
                }
                st.block_insert = Some(BlockInsert {
                    first: l1,
                    last: l2,
                    col: if eol { usize::MAX } else { col },
                    append_eol: eol,
                });
                if let Some(bi) = st.block_insert.as_mut() {
                    if !eol {
                        bi.col = col;
                    }
                }
                let pos = if eol {
                    pos
                } else {
                    pos.min(st.buf.line_start(l1) + col)
                };
                self.start_insert(st, pos, None);
            }
            VisualKind::Line => {
                let line = st.buf.line_of(if append { z } else { a });
                let pos = if append {
                    st.buf.line_end(line)
                } else {
                    st.buf.first_non_blank(line)
                };
                self.start_insert(st, pos, None);
            }
            VisualKind::Char => {
                let pos = if append { (z + 1).min(st.buf.len()) } else { a };
                self.start_insert(st, pos, None);
            }
        }
    }

    fn visual_replace(&mut self, st: &mut BufState, k: &str) {
        let Some(ch) = (if k == "<Tab>" { Some('\t') } else { char_of(k) }) else {
            return;
        };
        let Mode::Visual(kind) = st.mode else {
            return;
        };
        let (a, z) = ordered(st.anchor, st.cursor);
        let block = self.block_bounds(st);
        self.exit_visual(st);
        let ranges: Vec<(usize, usize)> = match kind {
            VisualKind::Char => vec![(a, (z + 1).min(st.buf.len()))],
            VisualKind::Line => {
                let (l1, l2) = (st.buf.line_of(a), st.buf.line_of(z));
                vec![(st.buf.line_start(l1), st.buf.line_end(l2))]
            }
            VisualKind::Block => {
                let (l1, l2, c1, c2) = block;
                (l1..=l2)
                    .map(|l| {
                        let (s, e) = (st.buf.line_start(l), st.buf.line_end(l));
                        (
                            (s + c1).min(e),
                            if c2 == usize::MAX {
                                e
                            } else {
                                (s + c2 + 1).min(e)
                            },
                        )
                    })
                    .collect()
            }
        };
        for (f, t) in ranges.iter().rev() {
            let old = st.buf.slice(*f, *t);
            let new: String = old
                .chars()
                .map(|c| if c == '\n' { c } else { ch })
                .collect();
            if new != old {
                self.edit(st, *f, *t, &new);
            }
        }
        st.cursor = ranges.first().map(|r| r.0).unwrap_or(a);
    }

    fn visual_put(&mut self, st: &mut BufState, reg: Option<char>, before: bool, clip: bool) {
        let Some(r) = self.registers.get(reg, clip) else {
            self.error(format!("E353: Nothing in register {}", reg.unwrap_or('"')));
            return;
        };
        let Mode::Visual(kind) = st.mode else {
            return;
        };
        let (a, z) = ordered(st.anchor, st.cursor);
        self.exit_visual(st);
        let b = &st.buf;
        let (l1, l2) = (b.line_of(a), b.line_of(z));
        let (from, to, deleted) = match kind {
            VisualKind::Line => {
                let mut t = b.slice(b.line_start(l1), b.line_end(l2));
                t.push('\n');
                (
                    b.line_start(l1),
                    b.line_end(l2),
                    Register::new(t, RegKind::Line),
                )
            }
            _ => {
                let to = (z + 1).min(b.len());
                (a, to, Register::new(b.slice(a, to), RegKind::Char))
            }
        };
        let text = match (kind, r.kind) {
            (VisualKind::Line, RegKind::Line) => {
                r.text.strip_suffix('\n').unwrap_or(&r.text).to_string()
            }
            (VisualKind::Line, _) => r.text.clone(),
            (_, RegKind::Line) => {
                format!("\n{}", r.text.strip_suffix('\n').unwrap_or(&r.text)).to_string() + "\n"
            }
            _ => r.text.clone(),
        };
        self.edit(st, from, to, &text);
        if !before {
            self.registers.store(None, deleted, false, clip);
        }
        st.cursor = if r.kind == RegKind::Line && kind != VisualKind::Line {
            from + 1
        } else {
            from
        };
        if r.kind == RegKind::Char && kind != VisualKind::Line && !text.contains('\n') {
            st.cursor = (from + text.chars().count()).saturating_sub(1).max(from);
        }
    }

    // ---- Command line

    fn cmdline_key(&mut self, st: &mut BufState, k: &str) {
        let incsearch = self.options.incsearch;
        let Some(cl) = st.cmdline.as_mut() else {
            return;
        };
        if cl.reg_pending {
            cl.reg_pending = false;
            if let Some(c) = char_of(k) {
                let text = if c == '"' {
                    self.registers.get(None, false)
                } else {
                    self.registers.get(Some(c), false)
                };
                if let Some(r) = text {
                    let line = r.text.lines().next().unwrap_or("").to_string();
                    if let Some(cl) = st.cmdline.as_mut() {
                        cl.text.push_str(&line);
                    }
                }
            }
            return;
        }
        match k {
            "<Esc>" | "<C-c>" => {
                let origin = cl.origin;
                st.cmdline = None;
                st.cursor = origin;
                return;
            }
            "<CR>" | "<C-j>" | "<C-m>" => {
                let cl = st.cmdline.take().unwrap();
                st.cursor = cl.origin;
                if cl.kind == ':' {
                    let text = cl.text.clone();
                    if !text.trim().is_empty() {
                        self.cmd_history.retain(|h| *h != text);
                        self.cmd_history.push(text.clone());
                        self.registers.set_special(':', text.clone());
                        self.last_ex = Some(text.clone());
                    }
                    self.ex(st, &text);
                } else {
                    self.finish_search(st, cl);
                }
                return;
            }
            "<BS>" | "<C-h>" => {
                if cl.text.pop().is_none() {
                    let origin = cl.origin;
                    st.cmdline = None;
                    st.cursor = origin;
                    return;
                }
            }
            "<C-u>" => cl.text.clear(),
            "<C-w>" => {
                let t = cl.text.trim_end().to_string();
                let cut = t
                    .char_indices()
                    .rev()
                    .find(|(_, c)| !crate::buffer::is_word_char(*c))
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(0);
                cl.text.truncate(cut);
            }
            "<C-r>" => cl.reg_pending = true,
            "<Up>" | "<Down>" => {
                let list = if cl.kind == ':' {
                    &self.cmd_history
                } else {
                    &self.search_history
                };
                if list.is_empty() {
                    return;
                }
                let idx = match (cl.history, k) {
                    (None, "<Up>") => Some(list.len() - 1),
                    (Some(i), "<Up>") => Some(i.saturating_sub(1)),
                    (Some(i), _) if i + 1 < list.len() => Some(i + 1),
                    _ => None,
                };
                cl.history = idx;
                cl.text = idx.map(|i| list[i].clone()).unwrap_or_default();
            }
            "<Tab>" => cl.text.push('\t'),
            _ => {
                if let Some(c) = char_of(k) {
                    cl.text.push(c);
                }
            }
        }
        // Incremental search: show the next match while typing.
        let Some(cl) = st.cmdline.as_ref() else {
            return;
        };
        if incsearch && (cl.kind == '/' || cl.kind == '?') {
            let (text, origin, forward) = (cl.text.clone(), cl.origin, cl.kind == '/');
            st.cursor = origin;
            if !text.is_empty() {
                let saved = self.out.message.take();
                if let Some(p) = self.search(st, &text, forward, origin, 1) {
                    st.cursor = p;
                }
                self.out.message = saved;
            }
        }
    }

    fn finish_search(&mut self, st: &mut BufState, cl: CmdLine) {
        let forward = cl.kind == '/';
        let pattern = if cl.text.is_empty() {
            match &self.last_search {
                Some((p, _)) => p.clone(),
                None => {
                    self.error("E35: No previous regular expression");
                    return;
                }
            }
        } else {
            cl.text.clone()
        };
        self.last_search = Some((pattern.clone(), forward));
        self.search_history.retain(|h| *h != pattern);
        self.search_history.push(pattern.clone());
        self.registers.set_special('/', pattern.clone());
        self.hl = true;
        let motion = Motion::Search { pattern, forward };
        match cl.op {
            Some(mut cmd) => {
                if let CmdKind::Operator { op, .. } = cmd.kind {
                    cmd.kind = CmdKind::Operator {
                        op,
                        target: Target::Motion(motion),
                    };
                }
                self.exec(st, cmd);
            }
            None => {
                let cmd = Command {
                    reg: None,
                    count: cl.count,
                    kind: CmdKind::Motion(motion.clone()),
                };
                self.exec_motion(st, &cmd, &motion);
            }
        }
    }

    // ---- Introspection for `:registers`, `:marks` and tests

    pub fn register(&self, name: char) -> Option<Register> {
        self.registers.get(Some(name), false)
    }

    pub(crate) fn registers_listing(&self) -> Vec<(char, String)> {
        self.registers.listing()
    }

    pub fn text(&self, h: Handle) -> Option<String> {
        self.bufs.get(&h).map(|s| s.buf.text().to_string())
    }

    pub fn cursor(&self, h: Handle) -> Option<usize> {
        self.bufs.get(&h).map(|s| s.cursor)
    }

    pub fn mode(&self, h: Handle) -> Option<&'static str> {
        self.bufs.get(&h).map(Self::mode_name)
    }

    /// Run `keys` as Normal mode commands (`:normal`); a command left in Insert mode ends.
    pub(crate) fn run_normal(&mut self, st: &mut BufState, keys: &[Key], remap: bool) {
        let saved: VecDeque<Typed> = std::mem::take(&mut self.typeahead);
        self.replaying += 1;
        for k in keys {
            self.typeahead.push_back(Typed {
                key: k.clone(),
                remap,
                event: None,
            });
        }
        self.run(st);
        self.typeahead.clear();
        if st.cmdline.is_some() {
            st.cmdline = None;
        }
        if matches!(st.mode, Mode::Insert | Mode::Replace) {
            self.finish_insert(st);
        }
        if st.is_visual() {
            self.exit_visual(st);
        }
        st.pending.clear();
        self.replaying -= 1;
        self.typeahead = saved;
    }
}

/// Escape a literal for a search pattern.
pub(crate) fn escape_pattern(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if "\\/.*$^~[]".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn convert_case(text: &str, op: Op) -> String {
    text.chars()
        .map(|c| match op {
            Op::Upper => c.to_uppercase().collect::<String>(),
            Op::Lower => c.to_lowercase().collect(),
            Op::Rot13 => {
                let r = match c {
                    'a'..='z' => (((c as u8 - b'a') + 13) % 26 + b'a') as char,
                    'A'..='Z' => (((c as u8 - b'A') + 13) % 26 + b'A') as char,
                    _ => c,
                };
                r.to_string()
            }
            _ => {
                if c.is_lowercase() {
                    c.to_uppercase().collect()
                } else {
                    c.to_lowercase().collect()
                }
            }
        })
        .collect()
}
