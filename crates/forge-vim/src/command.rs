//! Parsing of Normal and Visual mode commands from the pending keys:
//! `["x][count]{operator}[count]{motion|text object}`, motions and other commands.

use crate::keys::{char_of, Key};
use crate::registers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Delete,
    Change,
    Yank,
    Indent,
    Outdent,
    AutoIndent,
    ToggleCase,
    Lower,
    Upper,
    Rot13,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordFwd(bool),
    WordBack(bool),
    WordEnd(bool),
    WordEndBack(bool),
    LineStart,
    FirstNonBlank,
    LineEnd,
    LastNonBlank,
    /// `G` (count: line, else the last line).
    LastLine,
    /// `gg` (count: line, else the first line).
    FirstLine,
    Find {
        ch: char,
        forward: bool,
        till: bool,
    },
    RepeatFind {
        reverse: bool,
    },
    Percent,
    ParaFwd,
    ParaBack,
    ScreenTop,
    ScreenMiddle,
    ScreenBottom,
    NextLine,
    PrevLine,
    CurrentLine,
    Column,
    SearchNext {
        reverse: bool,
    },
    Star {
        forward: bool,
        whole: bool,
    },
    Mark {
        ch: char,
        linewise: bool,
    },
    /// `/pattern` used as the motion of an operator.
    Search {
        pattern: String,
        forward: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Motion(Motion),
    Object { inner: bool, ch: char },
    Lines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertAt {
    Before,
    After,
    LineStart,
    LineStartCol0,
    LineEnd,
    Below,
    Above,
    LastInsert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    HalfDown,
    HalfUp,
    PageDown,
    PageUp,
    LineDown,
    LineUp,
    CursorTop,
    CursorCenter,
    CursorBottom,
    CursorTopFirst,
    CursorCenterFirst,
    CursorBottomFirst,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Insert(InsertAt),
    Visual(crate::engine::VisualKind),
    DeleteChar,
    DeleteCharBack,
    Substitute,
    SubstituteLine,
    ChangeToEnd,
    DeleteToEnd,
    YankLine,
    Put { before: bool, cursor_after: bool },
    Join { spaces: bool },
    ReplaceChar(String),
    ReplaceMode,
    Undo,
    Redo,
    Repeat,
    ToggleCaseChar,
    Record(char),
    StopRecording,
    PlayMacro(char),
    SetMark(char),
    CmdLine(char),
    Increment(i64),
    Scroll(Scroll),
    Reselect,
    IdeAction(&'static str),
    Window(String),
    Escape,
    RepeatSubstitute,
    SaveQuit,
    Quit,
    // Visual mode only.
    SwapEnds,
    VisualInsert { append: bool },
    VisualReplace(String),
    VisualPut { before: bool },
    VisualJoin { spaces: bool },
    VisualLineOp(Op),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdKind {
    Motion(Motion),
    Operator {
        op: Op,
        target: Target,
    },
    /// An operator applied to the Visual selection.
    VisualOp(Op),
    /// A text object extending the Visual selection.
    VisualObject {
        inner: bool,
        ch: char,
    },
    Action(Action),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub reg: Option<char>,
    pub count: Option<usize>,
    pub kind: CmdKind,
}

impl Command {
    pub fn count1(&self) -> usize {
        self.count.unwrap_or(1).max(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parse {
    Incomplete,
    Invalid,
    Done(Command),
}

/// Keys that start a two-key command (`gg`, `zz`, `ZZ`, `<C-w>j`, `[[`).
const PREFIXES: [&str; 6] = ["g", "z", "Z", "[", "]", "<C-w>"];

fn token(keys: &[Key], i: usize) -> Option<Result<(String, usize), ()>> {
    let k = keys.get(i)?;
    if PREFIXES.contains(&k.as_str()) {
        let n = keys.get(i + 1)?;
        if k == "<C-w>" {
            return Some(Ok((format!("<C-w>{n}"), 2)));
        }
        return Some(Ok((format!("{k}{n}"), 2)));
    }
    Some(Ok((k.clone(), 1)))
}

fn count(keys: &[Key], mut i: usize) -> (Option<usize>, usize) {
    let mut n: Option<usize> = None;
    while let Some(c) = keys.get(i).and_then(|k| char_of(k)) {
        match c.to_digit(10) {
            Some(d) if !(d == 0 && n.is_none()) => {
                n = Some(n.unwrap_or(0).saturating_mul(10).saturating_add(d as usize));
                i += 1;
            }
            _ => break,
        }
    }
    (n, i)
}

fn operator(tok: &str) -> Option<Op> {
    Some(match tok {
        "d" => Op::Delete,
        "c" => Op::Change,
        "y" => Op::Yank,
        ">" => Op::Indent,
        "<" => Op::Outdent,
        "=" => Op::AutoIndent,
        "g~" => Op::ToggleCase,
        "gu" => Op::Lower,
        "gU" => Op::Upper,
        "g?" => Op::Rot13,
        _ => return None,
    })
}

/// Whether `tok` after operator `op` means "the current line(s)" (`dd`, `gUU`, `g~g~`).
fn line_repeat(op: Op, op_tok: &str, tok: &str) -> bool {
    if tok == op_tok {
        return true;
    }
    matches!(
        (op, tok),
        (Op::ToggleCase, "~") | (Op::Lower, "u") | (Op::Upper, "U") | (Op::Rot13, "?")
    )
}

enum M {
    Incomplete,
    Invalid,
    Done(Motion, usize),
}

/// A motion starting at token `tok` (keys[i..]).
fn motion(tok: &str, len: usize, keys: &[Key], i: usize) -> M {
    let arg = |off: usize| {
        keys.get(i + off)
            .and_then(|k| char_of(k).or(if k == "<Tab>" { Some('\t') } else { None }))
    };
    let m = match tok {
        "h" | "<Left>" | "<BS>" | "<C-h>" => Motion::Left,
        "l" | "<Right>" | " " => Motion::Right,
        "j" | "<Down>" | "<C-n>" | "<C-j>" | "gj" => Motion::Down,
        "k" | "<Up>" | "<C-p>" | "gk" => Motion::Up,
        "w" | "<S-Right>" => Motion::WordFwd(false),
        "W" | "<C-Right>" => Motion::WordFwd(true),
        "b" | "<S-Left>" => Motion::WordBack(false),
        "B" | "<C-Left>" => Motion::WordBack(true),
        "e" => Motion::WordEnd(false),
        "E" => Motion::WordEnd(true),
        "ge" => Motion::WordEndBack(false),
        "gE" => Motion::WordEndBack(true),
        "0" | "<Home>" | "g0" => Motion::LineStart,
        "^" | "g^" => Motion::FirstNonBlank,
        "$" | "<End>" | "g$" => Motion::LineEnd,
        "g_" => Motion::LastNonBlank,
        "G" | "<C-End>" => Motion::LastLine,
        "gg" | "<C-Home>" => Motion::FirstLine,
        "f" | "t" | "F" | "T" => {
            let Some(ch) = arg(1) else {
                return if keys.get(i + 1).is_some() {
                    M::Invalid
                } else {
                    M::Incomplete
                };
            };
            return M::Done(
                Motion::Find {
                    ch,
                    forward: tok == "f" || tok == "t",
                    till: tok == "t" || tok == "T",
                },
                len + 1,
            );
        }
        "/" | "?" => Motion::Search {
            pattern: String::new(),
            forward: tok == "/",
        },
        ";" => Motion::RepeatFind { reverse: false },
        "," => Motion::RepeatFind { reverse: true },
        "%" => Motion::Percent,
        "}" => Motion::ParaFwd,
        "{" => Motion::ParaBack,
        "H" => Motion::ScreenTop,
        "M" => Motion::ScreenMiddle,
        "L" => Motion::ScreenBottom,
        "+" | "<CR>" | "<C-m>" => Motion::NextLine,
        "-" => Motion::PrevLine,
        "_" => Motion::CurrentLine,
        "|" => Motion::Column,
        "n" => Motion::SearchNext { reverse: false },
        "N" => Motion::SearchNext { reverse: true },
        "*" => Motion::Star {
            forward: true,
            whole: true,
        },
        "#" => Motion::Star {
            forward: false,
            whole: true,
        },
        "g*" => Motion::Star {
            forward: true,
            whole: false,
        },
        "g#" => Motion::Star {
            forward: false,
            whole: false,
        },
        "`" | "'" | "g`" | "g'" => {
            let Some(ch) = arg(1) else {
                return if keys.get(i + 1).is_some() {
                    M::Invalid
                } else {
                    M::Incomplete
                };
            };
            return M::Done(
                Motion::Mark {
                    ch,
                    linewise: tok.ends_with('\''),
                },
                len + 1,
            );
        }
        _ => return M::Invalid,
    };
    M::Done(m, len)
}

fn action(
    tok: &str,
    keys: &[Key],
    i: usize,
    len: usize,
    recording: bool,
) -> Option<Result<(Action, usize), bool>> {
    use crate::engine::VisualKind;
    let need = |off: usize| -> Result<Key, bool> { keys.get(i + off).cloned().ok_or(true) };
    let a = match tok {
        "i" | "<Insert>" => Action::Insert(InsertAt::Before),
        "a" => Action::Insert(InsertAt::After),
        "I" => Action::Insert(InsertAt::LineStart),
        "gI" => Action::Insert(InsertAt::LineStartCol0),
        "A" => Action::Insert(InsertAt::LineEnd),
        "o" => Action::Insert(InsertAt::Below),
        "O" => Action::Insert(InsertAt::Above),
        "gi" => Action::Insert(InsertAt::LastInsert),
        "v" => Action::Visual(VisualKind::Char),
        "V" => Action::Visual(VisualKind::Line),
        "<C-v>" | "<C-q>" => Action::Visual(VisualKind::Block),
        "x" | "<Del>" => Action::DeleteChar,
        "X" => Action::DeleteCharBack,
        "s" => Action::Substitute,
        "S" => Action::SubstituteLine,
        "C" => Action::ChangeToEnd,
        "D" => Action::DeleteToEnd,
        "Y" => Action::YankLine,
        "p" => Action::Put {
            before: false,
            cursor_after: false,
        },
        "P" => Action::Put {
            before: true,
            cursor_after: false,
        },
        "gp" => Action::Put {
            before: false,
            cursor_after: true,
        },
        "gP" => Action::Put {
            before: true,
            cursor_after: true,
        },
        "J" => Action::Join { spaces: true },
        "gJ" => Action::Join { spaces: false },
        "r" => {
            let k = match need(1) {
                Ok(k) => k,
                Err(e) => return Some(Err(e)),
            };
            if k == "<Esc>" {
                return Some(Err(false));
            }
            return Some(Ok((Action::ReplaceChar(k), len + 1)));
        }
        "R" => Action::ReplaceMode,
        "u" => Action::Undo,
        "<C-r>" | "U" => Action::Redo,
        "." => Action::Repeat,
        "~" => Action::ToggleCaseChar,
        "q" if recording => Action::StopRecording,
        "q" | "@" | "m" => {
            let k = match need(1) {
                Ok(k) => k,
                Err(e) => return Some(Err(e)),
            };
            let Some(c) = char_of(&k) else {
                return Some(Err(false));
            };
            let a = match tok {
                "q" if c.is_ascii_alphanumeric() || c == '"' => Action::Record(c),
                "@" if c.is_ascii_alphanumeric() || c == '@' || c == ':' || c == '"' => {
                    Action::PlayMacro(c)
                }
                "m" if c.is_ascii_alphabetic() || "'`[]<>".contains(c) => Action::SetMark(c),
                _ => return Some(Err(false)),
            };
            return Some(Ok((a, len + 1)));
        }
        ":" => Action::CmdLine(':'),
        "<C-a>" => Action::Increment(1),
        "<C-x>" => Action::Increment(-1),
        "<C-d>" => Action::Scroll(Scroll::HalfDown),
        "<C-u>" => Action::Scroll(Scroll::HalfUp),
        "<C-f>" | "<PageDown>" | "<S-Down>" => Action::Scroll(Scroll::PageDown),
        "<C-b>" | "<PageUp>" | "<S-Up>" => Action::Scroll(Scroll::PageUp),
        "<C-e>" => Action::Scroll(Scroll::LineDown),
        "<C-y>" => Action::Scroll(Scroll::LineUp),
        "zt" => Action::Scroll(Scroll::CursorTop),
        "zz" => Action::Scroll(Scroll::CursorCenter),
        "zb" => Action::Scroll(Scroll::CursorBottom),
        "z<CR>" => Action::Scroll(Scroll::CursorTopFirst),
        "z." => Action::Scroll(Scroll::CursorCenterFirst),
        "z-" => Action::Scroll(Scroll::CursorBottomFirst),
        "gv" => Action::Reselect,
        "<C-o>" => Action::IdeAction("Back"),
        "<C-i>" | "<Tab>" => Action::IdeAction("Forward"),
        "<C-]>" | "gd" | "gD" => Action::IdeAction("GotoDeclaration"),
        "<C-6>" | "<C-^>" => Action::IdeAction("Switcher"),
        "gt" => Action::IdeAction("NextTab"),
        "gT" => Action::IdeAction("PreviousTab"),
        "zo" | "zO" => Action::IdeAction("ExpandRegion"),
        "zc" | "zC" => Action::IdeAction("CollapseRegion"),
        "za" | "zA" => Action::IdeAction("ExpandCollapseToggleAction"),
        "zR" => Action::IdeAction("ExpandAllRegions"),
        "zM" => Action::IdeAction("CollapseAllRegions"),
        "K" => Action::IdeAction("QuickJavaDoc"),
        "<C-g>" => Action::IdeAction("ShowFilePath"),
        "<Esc>" | "<C-c>" => Action::Escape,
        "&" => Action::RepeatSubstitute,
        "ZZ" => Action::SaveQuit,
        "ZQ" => Action::Quit,
        t if t.starts_with("<C-w>") => Action::Window(t["<C-w>".len()..].to_string()),
        _ => return None,
    };
    Some(Ok((a, len)))
}

/// Parse the pending keys of Normal (`visual` false) or Visual mode.
pub fn parse(keys: &[Key], visual: bool, recording: bool) -> Parse {
    let mut i = 0;
    let mut reg = None;
    if keys.first().map(String::as_str) == Some("\"") {
        let Some(r) = keys.get(1) else {
            return Parse::Incomplete;
        };
        match char_of(r) {
            Some(c) if registers::is_valid(c) => reg = Some(c),
            _ => return Parse::Invalid,
        }
        i = 2;
    }
    let (c1, ni) = count(keys, i);
    i = ni;
    let Some(t) = token(keys, i) else {
        return Parse::Incomplete;
    };
    let Ok((tok, len)) = t else {
        return Parse::Invalid;
    };
    let done = |count, kind| Parse::Done(Command { reg, count, kind });

    if visual {
        let vop = |op| done(c1, CmdKind::VisualOp(op));
        let va = |a| done(c1, CmdKind::Action(a));
        match tok.as_str() {
            "i" | "a" => {
                return match keys.get(i + 1).and_then(|k| char_of(k)) {
                    Some(ch) => done(
                        c1,
                        CmdKind::VisualObject {
                            inner: tok == "i",
                            ch,
                        },
                    ),
                    None if keys.get(i + 1).is_some() => Parse::Invalid,
                    None => Parse::Incomplete,
                }
            }
            "x" | "<Del>" => return vop(Op::Delete),
            "s" => return vop(Op::Change),
            "~" => return vop(Op::ToggleCase),
            "u" => return vop(Op::Lower),
            "U" => return vop(Op::Upper),
            "X" | "D" => return va(Action::VisualLineOp(Op::Delete)),
            "Y" => return va(Action::VisualLineOp(Op::Yank)),
            "C" | "S" | "R" => return va(Action::VisualLineOp(Op::Change)),
            "o" | "O" => return va(Action::SwapEnds),
            "I" => return va(Action::VisualInsert { append: false }),
            "A" => return va(Action::VisualInsert { append: true }),
            "J" => return va(Action::VisualJoin { spaces: true }),
            "gJ" => return va(Action::VisualJoin { spaces: false }),
            "p" => return va(Action::VisualPut { before: false }),
            "P" => return va(Action::VisualPut { before: true }),
            "r" => {
                return match keys.get(i + 1) {
                    Some(k) if k != "<Esc>" => va(Action::VisualReplace(k.clone())),
                    Some(_) => Parse::Invalid,
                    None => Parse::Incomplete,
                }
            }
            _ => {}
        }
        if let Some(op) = operator(&tok) {
            return vop(op);
        }
    } else if let Some(op) = operator(&tok) {
        i += len;
        let (c2, ni) = count(keys, i);
        i = ni;
        let total = match (c1, c2) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(1) * b.unwrap_or(1)),
        };
        let Some(t2) = token(keys, i) else {
            return Parse::Incomplete;
        };
        let Ok((tok2, len2)) = t2 else {
            return Parse::Invalid;
        };
        if line_repeat(op, &tok, &tok2) {
            return done(
                total,
                CmdKind::Operator {
                    op,
                    target: Target::Lines,
                },
            );
        }
        if tok2 == "i" || tok2 == "a" {
            return match keys.get(i + 1).and_then(|k| char_of(k)) {
                Some(ch) => done(
                    total,
                    CmdKind::Operator {
                        op,
                        target: Target::Object {
                            inner: tok2 == "i",
                            ch,
                        },
                    },
                ),
                None if keys.get(i + 1).is_some() => Parse::Invalid,
                None => Parse::Incomplete,
            };
        }
        return match motion(&tok2, len2, keys, i) {
            M::Done(m, used) if i + used == keys.len() => done(
                total,
                CmdKind::Operator {
                    op,
                    target: Target::Motion(m),
                },
            ),
            M::Done(..) | M::Invalid => Parse::Invalid,
            M::Incomplete => Parse::Incomplete,
        };
    }

    match motion(&tok, len, keys, i) {
        M::Done(m, used) if i + used == keys.len() => return done(c1, CmdKind::Motion(m)),
        M::Incomplete => return Parse::Incomplete,
        _ => {}
    }
    match action(&tok, keys, i, len, recording) {
        Some(Ok((a, used))) if i + used == keys.len() => done(c1, CmdKind::Action(a)),
        Some(Err(true)) => Parse::Incomplete,
        _ => Parse::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::parse_keys;

    fn p(s: &str) -> Parse {
        parse(&parse_keys(s, "\\"), false, false)
    }

    fn cmd(s: &str) -> Command {
        match p(s) {
            Parse::Done(c) => c,
            other => panic!("{s}: {other:?}"),
        }
    }

    #[test]
    fn parses_operators_and_motions() {
        assert_eq!(p("d"), Parse::Incomplete);
        assert_eq!(p("2d"), Parse::Incomplete);
        assert_eq!(p("\""), Parse::Incomplete);
        let c = cmd("\"a2d3w");
        assert_eq!(c.reg, Some('a'));
        assert_eq!(c.count, Some(6));
        assert_eq!(
            c.kind,
            CmdKind::Operator {
                op: Op::Delete,
                target: Target::Motion(Motion::WordFwd(false))
            }
        );
        assert_eq!(
            cmd("dd").kind,
            CmdKind::Operator {
                op: Op::Delete,
                target: Target::Lines
            }
        );
        assert_eq!(
            cmd("gUU").kind,
            CmdKind::Operator {
                op: Op::Upper,
                target: Target::Lines
            }
        );
        assert_eq!(
            cmd("g~g~").kind,
            CmdKind::Operator {
                op: Op::ToggleCase,
                target: Target::Lines
            }
        );
        assert_eq!(
            cmd("ci\"").kind,
            CmdKind::Operator {
                op: Op::Change,
                target: Target::Object {
                    inner: true,
                    ch: '"'
                }
            }
        );
        assert_eq!(p("df"), Parse::Incomplete);
        assert_eq!(
            cmd("dt,").kind,
            CmdKind::Operator {
                op: Op::Delete,
                target: Target::Motion(Motion::Find {
                    ch: ',',
                    forward: true,
                    till: true
                })
            }
        );
        assert_eq!(cmd("0").kind, CmdKind::Motion(Motion::LineStart));
        assert_eq!(cmd("10j").count, Some(10));
        assert_eq!(cmd("gg").kind, CmdKind::Motion(Motion::FirstLine));
        assert_eq!(p("g"), Parse::Incomplete);
        assert_eq!(p("dQ"), Parse::Invalid);
    }

    #[test]
    fn parses_actions() {
        assert_eq!(cmd("x").kind, CmdKind::Action(Action::DeleteChar));
        assert_eq!(
            cmd("rx").kind,
            CmdKind::Action(Action::ReplaceChar("x".into()))
        );
        assert_eq!(p("r"), Parse::Incomplete);
        assert_eq!(cmd("qa").kind, CmdKind::Action(Action::Record('a')));
        assert_eq!(
            parse(&parse_keys("q", "\\"), false, true),
            Parse::Done(Command {
                reg: None,
                count: None,
                kind: CmdKind::Action(Action::StopRecording)
            })
        );
        assert_eq!(cmd("@@").kind, CmdKind::Action(Action::PlayMacro('@')));
        assert_eq!(cmd("ma").kind, CmdKind::Action(Action::SetMark('a')));
        assert_eq!(
            cmd("<C-w>v").kind,
            CmdKind::Action(Action::Window("v".into()))
        );
        assert_eq!(
            cmd("zz").kind,
            CmdKind::Action(Action::Scroll(Scroll::CursorCenter))
        );
        assert_eq!(cmd("\"+p").reg, Some('+'));
    }

    #[test]
    fn parses_visual_commands() {
        let v = |s: &str| parse(&parse_keys(s, "\\"), true, false);
        assert!(matches!(
            v("d"),
            Parse::Done(Command {
                kind: CmdKind::VisualOp(Op::Delete),
                ..
            })
        ));
        assert!(matches!(
            v("iw"),
            Parse::Done(Command {
                kind: CmdKind::VisualObject {
                    inner: true,
                    ch: 'w'
                },
                ..
            })
        ));
        assert!(matches!(
            v("3j"),
            Parse::Done(Command {
                count: Some(3),
                kind: CmdKind::Motion(Motion::Down),
                ..
            })
        ));
        assert_eq!(v("i"), Parse::Incomplete);
    }
}
