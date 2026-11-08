//! Bindings, conflict detection and the resolver state machine that turns key events into
//! action ids.
//!
//! Handles single keystrokes, two-stroke chords ("Ctrl+K Ctrl+C"), modifier double-taps
//! ("Shift Shift"), keys pressed while a double-tapped modifier is held ("Double Ctrl+Up")
//! mouse shortcuts ("Ctrl+Button1 Click", "Alt+Wheel Up") and focus contexts (editor / text
//! input / terminal / everything else).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::catalog::ActionContext;
use crate::shortcut::{
    modifier_of_event, mouse_stroke, stroke_from_event, try_parse_shortcut, KeyEvent, KeyStroke,
    Modifier, MouseInput, Platform, Shortcut,
};

/// Where keyboard focus currently is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FocusContext {
    Editor,
    Input,
    Terminal,
    Toolwindow,
    /// A text field of a tool window that also takes the tool window's actions, like
    /// PHPStorm's commit message editor (`Ctrl+M`, `Ctrl+D`). Keys a text field needs stay
    /// with it, as in [`FocusContext::Input`].
    #[serde(rename = "toolwindow-input")]
    ToolwindowInput,
    /// A diff or merge window (its editors included).
    Diff,
    /// A Jupyter notebook in command mode (a cell is selected, no cell editor has focus).
    Notebook,
    /// The code editor of a notebook cell (edit mode); editor actions apply too.
    #[serde(rename = "notebook-editor")]
    NotebookEditor,
    #[default]
    Global,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub action_id: String,
    pub shortcut: Shortcut,
    pub context: ActionContext,
}

/// Keystrokes a text field needs for itself. In inputs these never trigger actions, so
/// Ctrl+A/C/V or word navigation keep working; plain and Shift+ keys are always left alone.
const INPUT_RESERVED: [&str; 39] = [
    "Ctrl+A",
    "Ctrl+C",
    "Ctrl+V",
    "Ctrl+X",
    "Ctrl+Z",
    "Ctrl+Y",
    "Ctrl+Shift+Z",
    "Ctrl+Insert",
    "Shift+Insert",
    "Shift+Delete",
    "Ctrl+Left",
    "Ctrl+Right",
    "Ctrl+Shift+Left",
    "Ctrl+Shift+Right",
    "Ctrl+Backspace",
    "Ctrl+Delete",
    "Ctrl+Home",
    "Ctrl+End",
    "Ctrl+Shift+Home",
    "Ctrl+Shift+End",
    "Meta+A",
    "Meta+C",
    "Meta+V",
    "Meta+X",
    "Meta+Z",
    "Meta+Shift+Z",
    "Meta+Left",
    "Meta+Right",
    "Meta+Shift+Left",
    "Meta+Shift+Right",
    "Meta+Backspace",
    "Alt+Left",
    "Alt+Right",
    "Alt+Shift+Left",
    "Alt+Shift+Right",
    "Alt+Backspace",
    "Alt+Delete",
    "Meta+Up",
    "Meta+Down",
];

/// Whether a keystroke may trigger an action while a plain text field has focus.
pub fn allowed_in_input(s: &KeyStroke) -> bool {
    if INPUT_RESERVED.contains(&s.id().as_str()) {
        return false;
    }
    if s.is_function_key() {
        return true;
    }
    if matches!(s.key.as_str(), "Escape" | "Enter" | "Tab") {
        return false;
    }
    s.ctrl || s.alt || s.meta
}

/// Whether a keystroke may trigger an action while the terminal has focus. Like PHPStorm's
/// "Override IDE shortcuts", plain Ctrl+letter, Alt+letter, Escape and editing keys go to the
/// shell; function keys, Alt+digit (tool windows) and Ctrl+Shift / Ctrl+Alt combinations
/// stay with the IDE (except Ctrl+Shift+C/V, the terminal's copy and paste).
pub fn allowed_in_terminal(s: &KeyStroke) -> bool {
    if s.is_function_key() {
        return true;
    }
    let digit = s.key.len() == 1 && s.key.as_bytes()[0].is_ascii_digit();
    if s.alt && !s.ctrl && digit {
        return true;
    }
    let copy_paste = s.key == "C" || s.key == "V";
    if s.meta {
        return !copy_paste;
    }
    if s.ctrl && s.shift && copy_paste {
        return false;
    }
    s.ctrl && (s.shift || s.alt)
}

/// Whether an action with `context` may fire under `focus`.
pub fn context_matches(context: ActionContext, focus: FocusContext) -> bool {
    match context {
        ActionContext::Any => true,
        ActionContext::Editor => {
            matches!(focus, FocusContext::Editor | FocusContext::NotebookEditor)
        }
        ActionContext::Toolwindow => matches!(
            focus,
            FocusContext::Toolwindow | FocusContext::Terminal | FocusContext::ToolwindowInput
        ),
        ActionContext::Diff => focus == FocusContext::Diff,
        ActionContext::Notebook => {
            matches!(focus, FocusContext::Notebook | FocusContext::NotebookEditor)
        }
        ActionContext::NotebookCommand => focus == FocusContext::Notebook,
    }
}

/// Build bindings from action → shortcut strings. Unparsable strings are skipped.
pub fn build_bindings(
    shortcuts: &[(String, Vec<String>)],
    context_of: impl Fn(&str) -> ActionContext,
    platform: Platform,
) -> Vec<Binding> {
    let mut out = Vec::new();
    for (action_id, list) in shortcuts {
        for text in list {
            if let Some(shortcut) = try_parse_shortcut(text, platform) {
                out.push(Binding {
                    action_id: action_id.clone(),
                    shortcut,
                    context: context_of(action_id),
                });
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictKind {
    /// Identical shortcut.
    Same,
    /// One is the first keystroke of the other's chord.
    Prefix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub action_id: String,
    pub shortcut: Shortcut,
    pub kind: ConflictKind,
}

/// Bindings that would clash with assigning `shortcut` to `action_id`.
pub fn find_conflicts(
    bindings: &[Binding],
    shortcut: &Shortcut,
    action_id: Option<&str>,
) -> Vec<Conflict> {
    let id = shortcut.id();
    let strokes = shortcut.strokes();
    let first = strokes.first().map(KeyStroke::id);
    let mut out = Vec::new();
    for b in bindings {
        if Some(b.action_id.as_str()) == action_id {
            continue;
        }
        let kind = if b.shortcut.id() == id {
            Some(ConflictKind::Same)
        } else {
            match (&first, &b.shortcut) {
                (Some(first), Shortcut::Keys(other))
                    if strokes.len() != other.len() && other[0].id() == *first =>
                {
                    Some(ConflictKind::Prefix)
                }
                _ => None,
            }
        };
        if let Some(kind) = kind {
            out.push(Conflict {
                action_id: b.action_id.clone(),
                shortcut: b.shortcut.clone(),
                kind,
            });
        }
    }
    out
}

/// All shortcuts bound to more than one action with overlapping contexts, in binding order:
/// shortcut id → action ids.
pub fn all_conflicts(bindings: &[Binding]) -> Vec<(String, Vec<String>)> {
    let mut order: Vec<String> = Vec::new();
    let mut by_id: HashMap<String, Vec<&Binding>> = HashMap::new();
    for b in bindings {
        let id = b.shortcut.id();
        by_id
            .entry(id.clone())
            .or_insert_with(|| {
                order.push(id);
                Vec::new()
            })
            .push(b);
    }
    let mut out = Vec::new();
    for id in order {
        let list = &by_id[&id];
        let mut ids: Vec<String> = Vec::new();
        for b in list {
            if !ids.contains(&b.action_id) {
                ids.push(b.action_id.clone());
            }
        }
        let overlapping = list.iter().enumerate().any(|(i, a)| {
            list[i + 1..].iter().any(|b| {
                a.action_id != b.action_id
                    && (a.context == ActionContext::Any
                        || b.context == ActionContext::Any
                        || a.context == b.context)
            })
        });
        if ids.len() > 1 && overlapping {
            out.push((id, ids));
        }
    }
    out
}

/// What the resolver is waiting for after a consumed or ignored event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Pending {
    /// The first keystroke of a chord was pressed.
    Chord,
    /// A modifier was tapped once; a second tap fires a double-tap action.
    DoubleTap,
}

/// Outcome of one key event.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolution {
    /// Whether the event belongs to the keymap (the UI calls `preventDefault`).
    pub consumed: bool,
    /// Action to run, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<Pending>,
}

impl Resolution {
    fn ignored() -> Self {
        Self::default()
    }

    fn action(id: String) -> Self {
        Self {
            consumed: true,
            action: Some(id),
            pending: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyInputKind {
    #[default]
    Keydown,
    Keyup,
    /// Mouse clicks and focus changes break a double-tap sequence.
    Interrupt,
    /// Window blur: forget chords and taps.
    Reset,
    /// A click or wheel turn (`button`, `clickCount`, `deltaY` plus the modifier keys).
    Mouse,
}

/// A key event as the UI forwards it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KeyInput {
    #[serde(rename = "type")]
    pub kind: KeyInputKind,
    #[serde(flatten)]
    pub event: KeyEvent,
    pub focus: FocusContext,
    /// Event time in milliseconds (`KeyboardEvent.timeStamp`).
    pub time: f64,
    /// Mouse events: `MouseEvent.button`.
    pub button: u8,
    /// Mouse events: `MouseEvent.detail` (2 for the second click of a double click).
    pub click_count: u32,
    /// Wheel events: `WheelEvent.deltaY`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_y: Option<f64>,
}

impl KeyInput {
    /// The mouse part of a `mouse` input.
    pub fn mouse(&self) -> MouseInput {
        MouseInput {
            button: self.button,
            click_count: self.click_count,
            delta_y: self.delta_y,
            ctrl_key: self.event.ctrl_key,
            alt_key: self.event.alt_key,
            shift_key: self.event.shift_key,
            meta_key: self.event.meta_key,
        }
    }
}

#[derive(Debug, Default)]
struct StrokeEntry {
    single: Vec<Binding>,
    /// Second stroke id → bindings.
    chords: HashMap<String, Vec<Binding>>,
}

#[derive(Debug, Clone, Copy)]
struct Tap {
    modifier: Modifier,
    at: f64,
}

/// The resolver state machine: bindings plus pending chord / double-tap state.
#[derive(Debug)]
pub struct Resolver {
    index: HashMap<String, StrokeEntry>,
    doubles: HashMap<Modifier, Vec<Binding>>,
    /// "Double Ctrl+Up" bindings by stroke id ("Ctrl+Up").
    double_keys: HashMap<String, Vec<Binding>>,
    /// Modifier that was tapped and is held down again within the double-tap time.
    double_held: Option<Modifier>,
    pending_chord: Option<(String, FocusContext)>,
    /// Modifier currently held as a potential tap.
    tap: Option<Modifier>,
    last_tap: Option<Tap>,
    double_tap_ms: f64,
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Resolver {
    pub const DOUBLE_TAP_MS: f64 = 400.0;

    pub fn new(bindings: Vec<Binding>) -> Self {
        let mut r = Self {
            index: HashMap::new(),
            doubles: HashMap::new(),
            double_keys: HashMap::new(),
            double_held: None,
            pending_chord: None,
            tap: None,
            last_tap: None,
            double_tap_ms: Self::DOUBLE_TAP_MS,
        };
        r.set_bindings(bindings);
        r
    }

    pub fn set_bindings(&mut self, bindings: Vec<Binding>) {
        self.index.clear();
        self.doubles.clear();
        self.double_keys.clear();
        self.reset();
        for b in bindings {
            let (first, second) = match &b.shortcut {
                Shortcut::Double(m) => {
                    self.doubles.entry(*m).or_default().push(b);
                    continue;
                }
                Shortcut::DoubleKey(_, stroke) => {
                    self.double_keys.entry(stroke.id()).or_default().push(b);
                    continue;
                }
                Shortcut::Keys(strokes) => (strokes[0].id(), strokes.get(1).map(KeyStroke::id)),
            };
            let entry = self.index.entry(first).or_default();
            match second {
                Some(sk) => entry.chords.entry(sk).or_default().push(b),
                None => entry.single.push(b),
            }
        }
    }

    /// Whether the resolver is waiting for the second keystroke of a chord.
    pub fn chord_pending(&self) -> bool {
        self.pending_chord.is_some()
    }

    pub fn reset(&mut self) {
        self.pending_chord = None;
        self.tap = None;
        self.last_tap = None;
        self.double_held = None;
    }

    /// Mouse clicks and focus changes break a double-tap sequence.
    pub fn interrupt(&mut self) {
        self.tap = None;
        self.last_tap = None;
        self.double_held = None;
    }

    pub fn handle(&mut self, input: &KeyInput) -> Resolution {
        match input.kind {
            KeyInputKind::Keydown => {
                // The second press of a double tap: keys pressed while it is held may fire
                // "Double Ctrl+Up" bindings.
                if let Some(m) = modifier_of_event(&input.event) {
                    let again = self
                        .last_tap
                        .filter(|t| t.modifier == m && input.time - t.at <= self.double_tap_ms);
                    if again.is_some() && !input.event.repeat {
                        self.double_held = Some(m);
                    }
                }
                self.key_down(&input.event, input.focus)
            }
            KeyInputKind::Keyup => self.key_up(&input.event, input.focus, input.time),
            KeyInputKind::Interrupt => {
                self.interrupt();
                Resolution::ignored()
            }
            KeyInputKind::Reset => {
                self.reset();
                Resolution::ignored()
            }
            KeyInputKind::Mouse => self.mouse(&input.mouse(), input.focus),
        }
    }

    /// A click or wheel turn: mouse shortcuts are single strokes. Text fields and the
    /// terminal keep their clicks, except the side buttons (Button4/5: Back/Forward), which
    /// mean nothing there and work everywhere like in PHPStorm. A pending chord or double tap
    /// is cancelled.
    pub fn mouse(&mut self, e: &MouseInput, focus: FocusContext) -> Resolution {
        self.interrupt();
        self.pending_chord = None;
        let Some(stroke) = mouse_stroke(e) else {
            return Resolution::ignored();
        };
        if !is_side_button(e)
            && matches!(
                focus,
                FocusContext::Input | FocusContext::ToolwindowInput | FocusContext::Terminal
            )
        {
            return Resolution::ignored();
        }
        match Self::pick(self.index.get(&stroke.id()).map(|e| &e.single), focus) {
            Some(action) => Resolution::action(action),
            None => Resolution::ignored(),
        }
    }

    /// The action a mouse stroke is bound to under `focus`, without changing any state
    /// (editor extensions that handle a click themselves ask this first).
    pub fn mouse_action(&self, e: &MouseInput, focus: FocusContext) -> Option<String> {
        let stroke = mouse_stroke(e)?;
        Self::pick(self.index.get(&stroke.id()).map(|e| &e.single), focus)
    }

    fn pick(list: Option<&Vec<Binding>>, focus: FocusContext) -> Option<String> {
        // Context-specific bindings win over global ones (e.g. Generate vs. New… on Alt+Insert),
        // the most specific first (Run Cell vs. Start New Line on Shift+Enter in a cell).
        let mut best: Option<&Binding> = None;
        for b in list?.iter().filter(|b| context_matches(b.context, focus)) {
            if best.is_none_or(|x| x.context.specificity() < b.context.specificity()) {
                best = Some(b);
            }
        }
        best.map(|b| b.action_id.clone())
    }

    pub fn key_down(&mut self, e: &KeyEvent, focus: FocusContext) -> Resolution {
        if let Some(m) = modifier_of_event(e) {
            // A bare modifier press starts a potential tap unless other modifiers are held.
            let held = [e.ctrl_key, e.alt_key, e.shift_key, e.meta_key]
                .iter()
                .filter(|x| **x)
                .count();
            self.tap = (!e.repeat && held == 1).then_some(m);
            let pending = if self.pending_chord.is_some() {
                Some(Pending::Chord)
            } else {
                self.last_tap
                    .filter(|t| t.modifier == m)
                    .map(|_| Pending::DoubleTap)
            };
            return Resolution {
                consumed: false,
                action: None,
                pending,
            };
        }
        self.tap = None;
        self.last_tap = None;
        let stroke = stroke_from_event(e);
        let sid = stroke.id();

        if let Some(m) = self.double_held {
            if held(e, m) {
                let list = self.double_keys.get(&sid).map(|l| {
                    l.iter()
                        .filter(|b| matches!(b.shortcut, Shortcut::DoubleKey(bm, _) if bm == m))
                        .cloned()
                        .collect::<Vec<_>>()
                });
                // Holding the modifier, the key can be pressed again (more carets).
                if let Some(action) = Self::pick(list.as_ref(), focus) {
                    return Resolution::action(action);
                }
            } else {
                self.double_held = None;
            }
        }

        if let Some((first, chord_focus)) = self.pending_chord.take() {
            let entry = self.index.get(&first);
            return match Self::pick(entry.and_then(|en| en.chords.get(&sid)), chord_focus) {
                Some(action) => Resolution::action(action),
                // Unknown second stroke: swallow it like PHPStorm does.
                None => Resolution {
                    consumed: true,
                    ..Resolution::default()
                },
            };
        }

        let text_field = matches!(focus, FocusContext::Input | FocusContext::ToolwindowInput);
        if text_field && !allowed_in_input(&stroke) {
            return Resolution::ignored();
        }
        if focus == FocusContext::Terminal && !allowed_in_terminal(&stroke) {
            return Resolution::ignored();
        }
        let Some(entry) = self.index.get(&sid) else {
            return Resolution::ignored();
        };
        let has_chord = entry
            .chords
            .values()
            .any(|l| l.iter().any(|b| context_matches(b.context, focus)));
        if has_chord {
            self.pending_chord = Some((sid, focus));
            return Resolution {
                consumed: true,
                action: None,
                pending: Some(Pending::Chord),
            };
        }
        match Self::pick(Some(&entry.single), focus) {
            Some(action) => Resolution::action(action),
            None => Resolution::ignored(),
        }
    }

    pub fn key_up(&mut self, e: &KeyEvent, focus: FocusContext, time: f64) -> Resolution {
        let m = modifier_of_event(e);
        if m.is_some() && m == self.double_held {
            self.double_held = None;
        }
        let tapped = self.tap.take();
        let Some(m) = m.filter(|m| tapped == Some(*m)) else {
            return Resolution::ignored();
        };
        if let Some(last) = self.last_tap {
            if last.modifier == m && time - last.at <= self.double_tap_ms {
                self.last_tap = None;
                return match Self::pick(self.doubles.get(&m), focus) {
                    Some(action) => Resolution::action(action),
                    None => Resolution::ignored(),
                };
            }
        }
        self.last_tap = Some(Tap {
            modifier: m,
            at: time,
        });
        let pending = self.doubles.contains_key(&m).then_some(Pending::DoubleTap);
        Resolution {
            consumed: false,
            action: None,
            pending,
        }
    }
}

/// Whether modifier `m` is down during `e`.
fn held(e: &KeyEvent, m: Modifier) -> bool {
    match m {
        Modifier::Ctrl => e.ctrl_key,
        Modifier::Alt => e.alt_key,
        Modifier::Shift => e.shift_key,
        Modifier::Meta => e.meta_key,
    }
}

/// A click of a side button (DOM buttons 3 and 4, PHPStorm's Button4/Button5: back and forward).
pub fn is_side_button(e: &MouseInput) -> bool {
    e.delta_y.is_none() && (3..=4).contains(&e.button)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcut::{parse_shortcut, parse_stroke};

    const O: Platform = Platform::Other;

    fn context_of(id: &str) -> ActionContext {
        match id {
            "Generate" | "EditorDuplicate" => ActionContext::Editor,
            "Rerun" => ActionContext::Toolwindow,
            "NextDiff" => ActionContext::Diff,
            "EditorStartNewLine" => ActionContext::Editor,
            "Jupyter.RunCellSelectBelow" => ActionContext::Notebook,
            "Jupyter.DeleteCell" | "Jupyter.InsertCellAbove" => ActionContext::NotebookCommand,
            _ => ActionContext::Any,
        }
    }

    fn map(entries: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        entries
            .iter()
            .map(|(id, list)| (id.to_string(), list.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    fn resolver(entries: &[(&str, &[&str])]) -> Resolver {
        Resolver::new(build_bindings(&map(entries), context_of, O))
    }

    /// A keydown-like event from "Ctrl+Shift+A".
    fn key(text: &str) -> KeyEvent {
        let s = parse_stroke(text, O).unwrap();
        let k = s.key.as_str();
        let code = if k.len() == 1 && k.as_bytes()[0].is_ascii_uppercase() {
            format!("Key{k}")
        } else if k.len() == 1 && k.as_bytes()[0].is_ascii_digit() {
            format!("Digit{k}")
        } else {
            match k {
                "/" => "Slash".into(),
                "Up" => "ArrowUp".into(),
                other => other.to_string(),
            }
        };
        KeyEvent {
            code,
            key: s.key.clone(),
            ctrl_key: s.ctrl,
            alt_key: s.alt,
            shift_key: s.shift,
            meta_key: s.meta,
            repeat: false,
        }
    }

    fn modifier(name: &str, down: bool) -> KeyEvent {
        KeyEvent {
            code: format!("{name}Left"),
            key: name.into(),
            ctrl_key: name == "Control" && down,
            alt_key: name == "Alt" && down,
            shift_key: name == "Shift" && down,
            meta_key: name == "Meta" && down,
            repeat: false,
        }
    }

    use FocusContext::{Editor, Global, Input, Terminal, Toolwindow, ToolwindowInput};

    fn action(r: &mut Resolver, text: &str, focus: FocusContext) -> Option<String> {
        r.key_down(&key(text), focus).action
    }

    #[test]
    fn notebook_bindings_win_in_notebooks_only() {
        let mut r = resolver(&[
            ("EditorStartNewLine", &["Shift+Enter"]),
            ("Jupyter.RunCellSelectBelow", &["Shift+Enter"]),
            ("Jupyter.DeleteCell", &["D D"]),
            ("Jupyter.InsertCellAbove", &["A"]),
            ("EditorDuplicate", &["Ctrl+D"]),
        ]);
        use FocusContext::{Notebook, NotebookEditor};
        assert_eq!(
            action(&mut r, "Shift+Enter", Editor).as_deref(),
            Some("EditorStartNewLine")
        );
        assert_eq!(
            action(&mut r, "Shift+Enter", NotebookEditor).as_deref(),
            Some("Jupyter.RunCellSelectBelow")
        );
        assert_eq!(
            action(&mut r, "Shift+Enter", Notebook).as_deref(),
            Some("Jupyter.RunCellSelectBelow")
        );
        // Editor actions still work in cell editors; command keys do not type there.
        assert_eq!(
            action(&mut r, "Ctrl+D", NotebookEditor).as_deref(),
            Some("EditorDuplicate")
        );
        assert_eq!(action(&mut r, "A", NotebookEditor), None);
        assert_eq!(
            action(&mut r, "A", Notebook).as_deref(),
            Some("Jupyter.InsertCellAbove")
        );
        assert_eq!(action(&mut r, "D", Notebook), None);
        assert_eq!(
            action(&mut r, "D", Notebook).as_deref(),
            Some("Jupyter.DeleteCell")
        );
        assert_eq!(action(&mut r, "D", Global), None);
    }

    fn single_stroke_resolver() -> Resolver {
        resolver(&[
            ("GotoAction", &["Ctrl+Shift+A"]),
            ("Generate", &["Alt+Insert"]),
            ("NewElement", &["Alt+Insert"]),
            ("RenameElement", &["Shift+F6"]),
            ("EditorDuplicate", &["Ctrl+D"]),
            ("Rerun", &["Ctrl+F5"]),
            ("JumpToLastWindow", &["F12"]),
        ])
    }

    #[test]
    fn resolves_bound_keystrokes() {
        let mut r = single_stroke_resolver();
        assert_eq!(
            r.key_down(&key("Ctrl+Shift+A"), Global),
            Resolution::action("GotoAction".into())
        );
    }

    #[test]
    fn ignores_unbound_keystrokes() {
        let mut r = single_stroke_resolver();
        assert_eq!(
            r.key_down(&key("Ctrl+Shift+Q"), Global),
            Resolution::ignored()
        );
    }

    #[test]
    fn prefers_context_specific_bindings_in_their_context() {
        let mut r = single_stroke_resolver();
        assert_eq!(
            action(&mut r, "Alt+Insert", Editor).as_deref(),
            Some("Generate")
        );
        assert_eq!(
            action(&mut r, "Alt+Insert", Global).as_deref(),
            Some("NewElement")
        );
    }

    #[test]
    fn diff_windows_take_their_own_bindings() {
        let mut r = resolver(&[("StepInto", &["F7"]), ("NextDiff", &["F7"])]);
        assert_eq!(
            action(&mut r, "F7", FocusContext::Diff).as_deref(),
            Some("NextDiff")
        );
        assert_eq!(action(&mut r, "F7", Editor).as_deref(), Some("StepInto"));
        assert_eq!(action(&mut r, "F7", Global).as_deref(), Some("StepInto"));
    }

    #[test]
    fn does_not_fire_editor_actions_outside_the_editor() {
        let mut r = single_stroke_resolver();
        assert_eq!(r.key_down(&key("Ctrl+D"), Global), Resolution::ignored());
        assert_eq!(
            action(&mut r, "Ctrl+D", Editor).as_deref(),
            Some("EditorDuplicate")
        );
    }

    #[test]
    fn fires_tool_window_actions_only_in_tool_windows() {
        let mut r = single_stroke_resolver();
        assert!(!r.key_down(&key("Ctrl+F5"), Editor).consumed);
        assert_eq!(
            action(&mut r, "Ctrl+F5", Toolwindow).as_deref(),
            Some("Rerun")
        );
        // A tool window's text field (the commit message) takes them too …
        assert_eq!(
            action(&mut r, "Ctrl+F5", FocusContext::ToolwindowInput).as_deref(),
            Some("Rerun")
        );
        // … but keeps its own editing keys.
        let mut plain = resolver(&[("Z", &["Ctrl+A"]), ("Y", &["F"])]);
        assert!(
            !plain
                .key_down(&key("Ctrl+A"), FocusContext::ToolwindowInput)
                .consumed
        );
        assert!(
            !plain
                .key_down(&key("F"), FocusContext::ToolwindowInput)
                .consumed
        );
    }

    #[test]
    fn lets_text_inputs_keep_plain_keys_but_allows_real_shortcuts() {
        let mut plain = resolver(&[("X", &["F"]), ("Y", &["Shift+F"]), ("Z", &["Ctrl+A"])]);
        assert!(!plain.key_down(&key("F"), Input).consumed);
        assert!(!plain.key_down(&key("Shift+F"), Input).consumed);
        assert!(!plain.key_down(&key("Ctrl+A"), Input).consumed);
        assert_eq!(action(&mut plain, "F", Global).as_deref(), Some("X"));
        let mut r = single_stroke_resolver();
        assert_eq!(
            action(&mut r, "Ctrl+Shift+A", Input).as_deref(),
            Some("GotoAction")
        );
        assert_eq!(
            action(&mut r, "Shift+F6", Input).as_deref(),
            Some("RenameElement")
        );
    }

    #[test]
    fn chords_wait_for_the_second_keystroke() {
        let mut r = resolver(&[
            ("CommentLine", &["Ctrl+K Ctrl+C"]),
            ("Uncomment", &["Ctrl+K Ctrl+U"]),
        ]);
        let first = r.key_down(&key("Ctrl+K"), Editor);
        assert!(first.consumed && first.action.is_none());
        assert_eq!(first.pending, Some(Pending::Chord));
        assert!(r.chord_pending());
        assert_eq!(
            r.key_down(&key("Ctrl+U"), Editor),
            Resolution::action("Uncomment".into())
        );
        assert!(!r.chord_pending());
    }

    #[test]
    fn chords_swallow_an_unknown_second_keystroke_and_recover() {
        let mut r = resolver(&[("CommentLine", &["Ctrl+K Ctrl+C"]), ("Find", &["Ctrl+F"])]);
        r.key_down(&key("Ctrl+K"), Global);
        let swallowed = r.key_down(&key("Ctrl+F"), Global);
        assert!(swallowed.consumed && swallowed.action.is_none() && swallowed.pending.is_none());
        assert_eq!(action(&mut r, "Ctrl+F", Global).as_deref(), Some("Find"));
    }

    #[test]
    fn chords_ignore_pure_modifier_presses_between_strokes() {
        let mut r = resolver(&[("X", &["Ctrl+K Shift+C"])]);
        r.key_down(&key("Ctrl+K"), Global);
        let m = r.key_down(&modifier("Shift", true), Global);
        assert_eq!(m.pending, Some(Pending::Chord));
        assert_eq!(action(&mut r, "Shift+C", Global).as_deref(), Some("X"));
    }

    fn tap(r: &mut Resolver, at: f64, focus: FocusContext) -> Resolution {
        r.key_down(&modifier("Shift", true), focus);
        r.key_up(&modifier("Shift", false), focus, at)
    }

    fn tap_resolver() -> Resolver {
        resolver(&[("SearchEverywhere", &["Shift Shift"])])
    }

    #[test]
    fn double_tap_fires_on_two_quick_taps() {
        let mut r = tap_resolver();
        let first = tap(&mut r, 0.0, Global);
        assert_eq!(first.action, None);
        assert_eq!(first.pending, Some(Pending::DoubleTap));
        assert_eq!(
            tap(&mut r, 200.0, Global),
            Resolution::action("SearchEverywhere".into())
        );
    }

    #[test]
    fn double_tap_works_while_typing_in_an_input() {
        let mut r = tap_resolver();
        tap(&mut r, 0.0, Input);
        assert_eq!(
            tap(&mut r, 150.0, Input).action.as_deref(),
            Some("SearchEverywhere")
        );
    }

    #[test]
    fn double_tap_does_not_fire_when_taps_are_too_slow() {
        let mut r = tap_resolver();
        tap(&mut r, 0.0, Global);
        assert_eq!(tap(&mut r, 900.0, Global).action, None);
        assert_eq!(
            tap(&mut r, 1000.0, Global).action.as_deref(),
            Some("SearchEverywhere")
        );
    }

    #[test]
    fn double_tap_does_not_fire_when_shift_was_used_as_a_modifier() {
        let mut r = tap_resolver();
        tap(&mut r, 0.0, Global);
        r.key_down(&modifier("Shift", true), Global);
        r.key_down(&key("Shift+A"), Global);
        assert_eq!(
            r.key_up(&modifier("Shift", false), Global, 100.0).action,
            None
        );
    }

    #[test]
    fn double_tap_is_broken_by_another_key_between_the_taps() {
        let mut r = tap_resolver();
        tap(&mut r, 0.0, Global);
        r.key_down(&key("A"), Global);
        assert_eq!(tap(&mut r, 100.0, Global).action, None);
    }

    #[test]
    fn double_tap_is_broken_by_interrupt() {
        let mut r = tap_resolver();
        tap(&mut r, 0.0, Global);
        r.handle(&KeyInput {
            kind: KeyInputKind::Interrupt,
            ..KeyInput::default()
        });
        assert_eq!(tap(&mut r, 100.0, Global).action, None);
    }

    fn input(kind: KeyInputKind, event: KeyEvent, time: f64) -> KeyInput {
        KeyInput {
            kind,
            event,
            focus: Editor,
            time,
            ..Default::default()
        }
    }

    fn clone_caret_resolver() -> Resolver {
        let mut r = resolver(&[
            ("EditorCloneCaretAbove", &["Double Ctrl+Up"]),
            ("EditorCloneCaretBelow", &["Double Ctrl+Down"]),
            ("EditorScrollUp", &["Ctrl+Up"]),
            ("RunAnything", &["Double Ctrl"]),
        ]);
        r.handle(&input(
            KeyInputKind::Keydown,
            modifier("Control", true),
            0.0,
        ));
        r.handle(&input(
            KeyInputKind::Keyup,
            modifier("Control", false),
            50.0,
        ));
        r
    }

    #[test]
    fn keys_fire_while_a_double_tapped_modifier_is_held() {
        let mut r = clone_caret_resolver();
        r.handle(&input(
            KeyInputKind::Keydown,
            modifier("Control", true),
            150.0,
        ));
        let up = |r: &mut Resolver, t| r.handle(&input(KeyInputKind::Keydown, key("Ctrl+Up"), t));
        assert_eq!(
            up(&mut r, 200.0).action.as_deref(),
            Some("EditorCloneCaretAbove")
        );
        // Still held: every press clones again, also after the double-tap time.
        assert_eq!(
            up(&mut r, 2000.0).action.as_deref(),
            Some("EditorCloneCaretAbove")
        );
        let down = r.handle(&input(KeyInputKind::Keydown, key("Ctrl+Down"), 2100.0));
        assert_eq!(down.action.as_deref(), Some("EditorCloneCaretBelow"));
        // Releasing the modifier neither fires "Double Ctrl" nor keeps the state.
        let release = r.handle(&input(
            KeyInputKind::Keyup,
            modifier("Control", false),
            2200.0,
        ));
        assert_eq!(release.action, None);
        r.handle(&input(
            KeyInputKind::Keydown,
            modifier("Control", true),
            5000.0,
        ));
        assert_eq!(up(&mut r, 5050.0).action.as_deref(), Some("EditorScrollUp"));
    }

    #[test]
    fn a_plain_or_slow_modifier_press_keeps_the_normal_binding() {
        let mut r = clone_caret_resolver();
        r.handle(&input(
            KeyInputKind::Keydown,
            modifier("Control", true),
            900.0,
        ));
        let res = r.handle(&input(KeyInputKind::Keydown, key("Ctrl+Up"), 950.0));
        assert_eq!(res.action.as_deref(), Some("EditorScrollUp"));
        // The double tap alone still fires "Double Ctrl".
        let mut r = clone_caret_resolver();
        r.handle(&input(
            KeyInputKind::Keydown,
            modifier("Control", true),
            100.0,
        ));
        let res = r.handle(&input(
            KeyInputKind::Keyup,
            modifier("Control", false),
            150.0,
        ));
        assert_eq!(res.action.as_deref(), Some("RunAnything"));
    }

    #[test]
    fn double_tap_ignores_auto_repeat_of_a_held_modifier() {
        let mut r = tap_resolver();
        tap(&mut r, 0.0, Global);
        let mut held = modifier("Shift", true);
        held.repeat = true;
        r.key_down(&held, Global);
        assert_eq!(
            r.key_up(&modifier("Shift", false), Global, 100.0).action,
            None
        );
    }

    #[test]
    fn handles_key_inputs_from_json() {
        let mut r = tap_resolver();
        let down: KeyInput = serde_json::from_str(
            r#"{"type":"keydown","key":"Shift","code":"ShiftLeft","shiftKey":true,"focus":"global","time":0}"#,
        )
        .unwrap();
        let up: KeyInput = serde_json::from_str(
            r#"{"type":"keyup","key":"Shift","code":"ShiftLeft","focus":"global","time":10}"#,
        )
        .unwrap();
        r.handle(&down);
        r.handle(&up);
        r.handle(&down);
        let res = r.handle(&KeyInput { time: 120.0, ..up });
        assert_eq!(
            serde_json::to_string(&res).unwrap(),
            r#"{"consumed":true,"action":"SearchEverywhere"}"#
        );
    }

    fn conflict_bindings() -> Vec<Binding> {
        build_bindings(
            &map(&[
                ("GotoClass", &["Ctrl+N"]),
                ("CheckinProject", &["Ctrl+K"]),
                ("Generate", &["Alt+Insert"]),
                ("NewElement", &["Alt+Insert"]),
                ("Chord", &["Ctrl+K Ctrl+C"]),
            ]),
            context_of,
            O,
        )
    }

    fn sc(text: &str) -> Shortcut {
        parse_shortcut(text, O).unwrap()
    }

    #[test]
    fn finds_identical_shortcuts() {
        let c = find_conflicts(&conflict_bindings(), &sc("Ctrl+N"), Some("Other"));
        let got: Vec<_> = c.iter().map(|x| (x.action_id.as_str(), x.kind)).collect();
        assert_eq!(got, vec![("GotoClass", ConflictKind::Same)]);
    }

    #[test]
    fn does_not_report_the_action_itself() {
        assert!(find_conflicts(&conflict_bindings(), &sc("Ctrl+N"), Some("GotoClass")).is_empty());
    }

    #[test]
    fn finds_chord_prefix_clashes_both_ways() {
        let ids = |text: &str| -> Vec<String> {
            find_conflicts(&conflict_bindings(), &sc(text), Some("New"))
                .into_iter()
                .map(|c| c.action_id)
                .collect()
        };
        assert_eq!(ids("Ctrl+K Ctrl+X"), vec!["CheckinProject"]);
        assert_eq!(ids("Ctrl+K"), vec!["CheckinProject", "Chord"]);
    }

    #[test]
    fn lists_all_conflicting_shortcuts() {
        let all = all_conflicts(&conflict_bindings());
        assert_eq!(
            all,
            vec![(
                "Alt+Insert".to_string(),
                vec!["Generate".to_string(), "NewElement".to_string()]
            )]
        );
    }

    #[test]
    fn context_matching() {
        assert!(context_matches(ActionContext::Any, Input));
        assert!(context_matches(ActionContext::Editor, Editor));
        assert!(!context_matches(ActionContext::Editor, Input));
        assert!(!context_matches(ActionContext::Toolwindow, Global));
        assert!(context_matches(ActionContext::Toolwindow, Terminal));
        assert!(context_matches(ActionContext::Diff, FocusContext::Diff));
        assert!(context_matches(
            ActionContext::Editor,
            FocusContext::NotebookEditor
        ));
        assert!(context_matches(
            ActionContext::Notebook,
            FocusContext::NotebookEditor
        ));
        assert!(context_matches(
            ActionContext::Notebook,
            FocusContext::Notebook
        ));
        assert!(context_matches(
            ActionContext::NotebookCommand,
            FocusContext::Notebook
        ));
        assert!(!context_matches(
            ActionContext::NotebookCommand,
            FocusContext::NotebookEditor
        ));
        assert!(!context_matches(
            ActionContext::Notebook,
            FocusContext::Editor
        ));
        assert!(context_matches(ActionContext::Any, FocusContext::Diff));
        assert!(!context_matches(ActionContext::Diff, Editor));
        assert!(!context_matches(ActionContext::Editor, FocusContext::Diff));
        // The commit message takes tool window actions, but not editor ones.
        assert!(context_matches(
            ActionContext::Toolwindow,
            FocusContext::ToolwindowInput
        ));
        assert!(!context_matches(ActionContext::Toolwindow, Input));
        assert!(!context_matches(
            ActionContext::Editor,
            FocusContext::ToolwindowInput
        ));
        assert_eq!(
            serde_json::to_string(&FocusContext::ToolwindowInput).unwrap(),
            "\"toolwindow-input\""
        );
    }

    #[test]
    fn input_rules() {
        let allowed = |s: &str| allowed_in_input(&parse_stroke(s, O).unwrap());
        assert!(allowed("Ctrl+Shift+A"));
        assert!(allowed("F2"));
        assert!(!allowed("Ctrl+V"));
        assert!(!allowed("Meta+Left"));
        assert!(!allowed("Shift+A"));
        assert!(!allowed("Escape"));
    }

    #[test]
    fn leaves_shell_keys_to_the_terminal_but_keeps_ide_function_keys_and_tool_windows() {
        let allowed = |s: &str| allowed_in_terminal(&parse_stroke(s, O).unwrap());
        for s in [
            "Ctrl+C",
            "Ctrl+D",
            "Ctrl+R",
            "Ctrl+E",
            "Alt+B",
            "Escape",
            "Tab",
            "Ctrl+Shift+C",
            "Ctrl+Shift+V",
        ] {
            assert!(!allowed(s), "{s}");
        }
        for s in [
            "Alt+F12",
            "Shift+F10",
            "Ctrl+F2",
            "Alt+4",
            "Ctrl+Shift+A",
            "Ctrl+Alt+S",
            "Ctrl+Shift+T",
        ] {
            assert!(allowed(s), "{s}");
        }
        let mut r = Resolver::default();
        r.set_bindings(build_bindings(
            &map(&[("RecentFiles", &["Ctrl+E"]), ("Run", &["Shift+F10"])]),
            |_| ActionContext::Any,
            O,
        ));
        assert_eq!(r.key_down(&key("Ctrl+E"), Terminal), Resolution::ignored());
        assert_eq!(
            action(&mut r, "Shift+F10", Terminal).as_deref(),
            Some("Run")
        );
    }

    #[test]
    fn resolves_mouse_shortcuts_by_focus() {
        let mut r = resolver(&[
            (
                "GotoDeclaration",
                &["Ctrl+B", "Ctrl+Button1 Click", "Button2 Click"],
            ),
            ("Generate", &["Alt+Wheel Down"]),
            ("NextTab", &["Ctrl+Wheel Down"]),
        ]);
        let click = |button: u8, ctrl: bool| KeyInput {
            kind: KeyInputKind::Mouse,
            event: KeyEvent {
                ctrl_key: ctrl,
                ..Default::default()
            },
            button,
            click_count: 1,
            focus: Editor,
            ..Default::default()
        };
        assert_eq!(
            r.handle(&click(0, true)).action.as_deref(),
            Some("GotoDeclaration")
        );
        assert_eq!(
            r.handle(&click(1, false)).action.as_deref(),
            Some("GotoDeclaration")
        );
        assert_eq!(r.handle(&click(0, false)), Resolution::ignored());
        // Generate is an editor action: not in a tool window.
        let wheel = MouseInput {
            delta_y: Some(40.0),
            alt_key: true,
            ..Default::default()
        };
        assert_eq!(r.mouse_action(&wheel, Editor).as_deref(), Some("Generate"));
        assert_eq!(r.mouse_action(&wheel, Toolwindow), None);
        let ctrl_wheel = MouseInput {
            delta_y: Some(40.0),
            ctrl_key: true,
            ..Default::default()
        };
        assert_eq!(
            r.mouse(&ctrl_wheel, Global).action.as_deref(),
            Some("NextTab")
        );
        // Clicks in text fields stay with them.
        let mut input = click(0, true);
        input.focus = Input;
        assert_eq!(r.handle(&input), Resolution::ignored());
    }

    #[test]
    fn side_buttons_navigate_in_every_focus() {
        let mut r = resolver(&[
            ("Back", &["Ctrl+Alt+Left", "Button4 Click"]),
            ("Forward", &["Ctrl+Alt+Right", "Button5 Click"]),
            ("GotoDeclaration", &["Ctrl+Button1 Click"]),
        ]);
        let side = |button: u8| MouseInput {
            button,
            click_count: 1,
            ..Default::default()
        };
        for focus in [Global, Editor, Toolwindow, Input, ToolwindowInput, Terminal] {
            assert_eq!(
                r.mouse(&side(3), focus).action.as_deref(),
                Some("Back"),
                "{focus:?}"
            );
            assert_eq!(
                r.mouse(&side(4), focus).action.as_deref(),
                Some("Forward"),
                "{focus:?}"
            );
        }
        // Other clicks in text fields still stay with them.
        let ctrl_click = MouseInput {
            button: 0,
            click_count: 1,
            ctrl_key: true,
            ..Default::default()
        };
        assert_eq!(r.mouse(&ctrl_click, Input), Resolution::ignored());
        assert!(is_side_button(&side(3)) && is_side_button(&side(4)));
        assert!(!is_side_button(&side(1)) && !is_side_button(&side(5)));
    }

    #[test]
    fn a_click_cancels_a_pending_chord() {
        let mut r = resolver(&[("CommentByLineComment", &["Ctrl+K Ctrl+C"])]);
        assert!(r.key_down(&key("Ctrl+K"), Editor).consumed);
        assert!(r.chord_pending());
        r.mouse(
            &MouseInput {
                click_count: 1,
                ..Default::default()
            },
            Editor,
        );
        assert!(!r.chord_pending());
    }

    #[test]
    fn build_bindings_skips_unparsable_strings() {
        let b = build_bindings(
            &map(&[("A", &["Ctrl+Nope", "Ctrl+A"])]),
            |_| ActionContext::Any,
            O,
        );
        assert_eq!(b.len(), 1);
    }
}
