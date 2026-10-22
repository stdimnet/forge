//! Behaviour tests of the Vim engine: `|` marks the caret in the text before and after.

use forge_vim::engine::{Effect, SelRange, Sync};
use forge_vim::{DomKey, Vim};

fn setup(text: &str) -> (Vim, u32) {
    let caret = text.chars().position(|c| c == '|').unwrap_or(0);
    let clean: String = text.chars().filter(|c| *c != '|').collect();
    let mut v = Vim::new();
    let h = v.attach(&clean);
    // UTF-16 offset of the caret.
    let off: usize = clean.chars().take(caret).map(char::len_utf16).sum();
    v.key(
        h,
        None,
        &Sync {
            selection: Some(vec![SelRange {
                anchor: off,
                head: off,
            }]),
            ..Sync::default()
        },
    );
    (v, h)
}

fn show(v: &Vim, h: u32) -> String {
    let text = v.text(h).unwrap();
    let c = v.cursor(h).unwrap();
    let mut out: String = text.chars().take(c).collect();
    out.push('|');
    out.extend(text.chars().skip(c));
    out
}

#[track_caller]
fn check(before: &str, keys: &str, after: &str) {
    let (mut v, h) = setup(before);
    v.feed(h, keys);
    assert_eq!(show(&v, h), after, "{before:?} + {keys:?}");
}

fn effects(before: &str, keys: &str) -> Vec<Effect> {
    let (mut v, h) = setup(before);
    let mut all = Vec::new();
    for k in forge_vim::keys::parse_keys(keys, "\\") {
        all.extend(v.key(h, Some(k), &Sync::default()).effects);
    }
    all.retain(|e| !matches!(e, Effect::Reveal { .. }));
    all
}

#[test]
fn deletes_lines_and_characters() {
    check("a\n|b\nc", "dd", "a\n|c");
    check("a\n|b", "dd", "|a");
    check("|a\nb\nc\nd", "2dd", "|c\nd");
    check("a\n|b\nc\nd", "dj", "a\n|d");
    check("a\nb\n|c", "dk", "|a");
    check("  foo |bar baz", "D", "  foo| ");
    check("|abc", "x", "|bc");
    check("|abc", "3x", "|");
    check("ab|c", "X", "a|c");
    check("|foo\n  bar", "J", "foo| bar");
    check("|a\nb\nc", "3J", "a b| c");
    check("|a\nb", "gJ", "|ab");
}

#[test]
fn applies_operators_with_motions() {
    check("|foo bar", "dw", "|bar");
    check("foo |bar\nbaz", "dw", "foo| \nbaz");
    check("|foo bar baz", "d2w", "|baz");
    check("|foo bar", "cwxyz<Esc>", "xy|z bar");
    check("|foo.bar", "de", "|.bar");
    check("a,b,|c,d", "dt,", "a,b,|,d");
    check("a,b,|c,d", "dF,", "a,b|c,d");
    check("|(a [b])", "d%", "|");
    check("one\n|two\nthree\n\nfour", "d}", "one\n|\nfour");
    check("x |foo bar", "gUiw", "x |FOO bar");
    check("|Hello", "g~~", "|hELLO");
    check("|abc", "~~", "AB|c");
    check("|abc", "rx", "|xbc");
    check("|abc", "Rxy<Esc>", "x|yc");
    check("|a\nb", ">j", "    |a\n    b");
    check("    |a", "<<", "|a");
    check("|x 7 y", "<C-a>", "x |8 y");
    check("|x 7 y", "5<C-x>", "x |2 y");
    check("|x -1 y", "<C-a>", "x |0 y");
}

#[test]
fn uses_text_objects() {
    check("foo b|ar baz", "ciwqux<Esc>", "foo qu|x baz");
    check("say \"h|ello\" now", "ci\"hi<Esc>", "say \"h|i\" now");
    check("say \"h|ello\" now", "da\"", "say |now");
    check("f(a, |b)", "di(", "f(|)");
    check("f(a, (|b))", "2di(", "f(|)");
    check("<a><b>|x</b></a>", "dit", "<a><b>|</b></a>");
    check("<a><b>|x</b></a>", "dat", "<a>|</a>");
    check("if (x) {\n    fo|o;\n}", "di{", "if (x) {\n|}");
    check("a\n|b\n\nc", "dap", "|c");
}

#[test]
fn yanks_and_puts() {
    check("|a\nb", "yyp", "a\n|a\nb");
    check("|a\nb", "yyP", "|a\na\nb");
    check("|foo bar", "yiw$p", "foo barfo|o");
    check("|foo bar", "veyP", "fo|ofoo bar");
    check("|a\nb", "\"ayyj\"ap", "a\nb\n|a");
    check("|ab", "xp", "b|a");
    check("|one two", "dwP", "one| two");
    check("|foo bar", "yiwwviwp", "foo fo|o");
}

#[test]
fn uses_visual_modes() {
    check("a\n|b\nc\nd", "Vjd", "a\n|d");
    check("|abc def", "vlld", "| def");
    check("|abc\nabc\nabc", "<C-v>jjd", "|bc\nbc\nbc");
    check("|abc\nabc\nabc", "<C-v>jjIx<Esc>", "|xabc\nxabc\nxabc");
    check("|abc\nabc", "<C-v>j$Ax<Esc>", "abc|x\nabcx");
    check("|abc\nabc", "<C-v>jlcZ<Esc>", "|Zc\nZc");
    check("|foo bar", "viwU", "|FOO bar");
    check("|a\nb", "Vj>", "    |a\n    b");
    check("|abc", "vlrx", "|xxc");
    check("|a\nb\nc", "VjJ", "a| b\nc");
    check("|abc d", "vlygvd", "|c d");
    check("|abcd", "vllohd", "|d");
}

#[test]
fn repeats_changes_with_dot() {
    check("|a b c", "x.", "|b c");
    check("|foo foo", "ciwbar<Esc>w.", "bar ba|r");
    check("|a\nb\nc\nd", "dd.", "|c\nd");
    check("|a\nb\nc\nd\ne", "dd2.", "|d\ne");
    check("|x\ny", "A;<Esc>j.", "x;\ny|;");
    check("|a\nb\nc", "Vd.", "|c");
    check("|one\ntwo", "oz<Esc>.", "one\nz\n|z\ntwo");
    check("|ab", "3ix<Esc>", "xx|xab");
}

#[test]
fn searches_and_marks() {
    check("|a\nfoo\nbar foo", "/foo<CR>", "a\n|foo\nbar foo");
    check("|a\nfoo\nbar foo", "/foo<CR>n", "a\nfoo\nbar |foo");
    check("|a\nfoo\nbar foo", "/foo<CR>nn", "a\n|foo\nbar foo");
    check("|a\nfoo\nbar foo", "?foo<CR>", "a\nfoo\nbar |foo");
    check("|foo bar foo", "*", "foo bar |foo");
    check("foo bar |foo", "#", "|foo bar foo");
    check("|a\nb\nc", "majj'a", "|a\nb\nc");
    check("a|bc\nd", "mbj`b", "a|bc\nd");
    check("|a foo b foo", "d/foo<CR>", "|foo b foo");
    check("|abc", "/zzz<CR>", "|abc");
}

#[test]
fn moves_the_caret() {
    check("|foo bar.baz", "w", "foo |bar.baz");
    check("|foo bar.baz", "3w", "foo bar.|baz");
    check("foo bar.ba|z", "B", "foo |bar.baz");
    check("|foo bar", "e", "fo|o bar");
    check("  |foo", "0", "|  foo");
    check("|  foo", "^", "  |foo");
    check("|foo", "$", "fo|o");
    check("|a\nb\nc", "G", "a\nb\n|c");
    check("a\nb\n|c", "gg", "|a\nb\nc");
    check("a\nb\n|c", "2G", "a\n|b\nc");
    check("|a,b,c", "f,;", "a,b|,c");
    check("|a,b,c", "f,;,", "a|,b,c");
    check("|a,b,c", "t,;", "a,|b,c");
    check("|(a [b])", "%", "(a [b]|)");
    check("a\n\n|b\nc\n\nd", "}", "a\n\nb\nc\n|\nd");
    check("abcdef\n|ab\nabcdef", "4l2j", "abcdef\nab\na|bcdef");
    check("abcd|ef\nab\nabcdef", "jj", "abcdef\nab\nabcd|ef");
    check("abcdef\nab\nabcd|ef", "kk", "abcd|ef\nab\nabcdef");
    check("|abc", "3|", "ab|c");
}

#[test]
fn records_and_plays_macros() {
    check("|a\nb\nc", "qaA!<Esc>jq2@a", "a!\nb!\nc|!");
    check("|1\n2", "qqx<Esc>jq@q", "\n|");
    let (mut v, h) = setup("|x");
    v.feed(h, "qwiab<Esc>q");
    assert_eq!(v.register('w').unwrap().text, "iab<Esc>");
}

#[test]
fn runs_ex_commands() {
    check("|foo foo\nfoo", ":s/foo/bar/g<CR>", "|bar bar\nfoo");
    check("|foo foo\nfoo", ":%s/foo/x/<CR>", "x foo\n|x");
    check("|a1\nb2", ":%s/\\(\\w\\)\\(\\d\\)/\\2\\1/<CR>", "1a\n|2b");
    check("|ab\nab", ":%s/a/\\U&/g<CR>", "Ab\n|Ab");
    check("a\n|b\nc", ":1<CR>", "|a\nb\nc");
    check("|a\nb\nc\nd", ":2,3d<CR>", "a\n|d");
    check("|x1\ny\nx2", ":g/x/d<CR>", "|y");
    check("|x1\ny\nx2", ":v/x/d<CR>", "x1\n|x2");
    check("|a\nb", ":%normal A;<CR>", "a;\nb|;");
    check("|c\na\nb", ":sort<CR>", "|a\nb\nc");
    check("|a\nb\nc", ":m$<CR>", "b\nc\n|a");
    check("|a\nb", ":t.<CR>", "a\n|a\nb");
    check("|a\nb\nc", ":2,3><CR>", "a\n    b\n    |c");
    check("|foo\nfoo", ":s/foo/bar/<CR>j&", "bar\n|bar");
    check("|a\nb\nc", "Vj:s/$/!/<CR>", "a!\n|b!\nc");
    let (mut v, h) = setup("|x");
    let r = v.feed(h, ":bogus<CR>");
    assert!(r.message.unwrap().text.starts_with("E492"));
}

#[test]
fn emits_effects() {
    assert_eq!(effects("|x", "u"), vec![Effect::Undo { count: 1 }]);
    assert_eq!(effects("|x", "3<C-r>"), vec![Effect::Redo { count: 3 }]);
    assert_eq!(
        effects("|x", ":w<CR>"),
        vec![Effect::Action {
            id: "SaveAll".into()
        }]
    );
    assert_eq!(
        effects("|x", ":wq<CR>"),
        vec![
            Effect::Action {
                id: "SaveAll".into()
            },
            Effect::Action {
                id: "CloseContent".into()
            }
        ]
    );
    assert_eq!(
        effects("|x", ":e src/a.php<CR>"),
        vec![Effect::Open {
            path: "src/a.php".into()
        }]
    );
    assert_eq!(
        effects("|x", ":action GotoClass<CR>"),
        vec![Effect::Action {
            id: "GotoClass".into()
        }]
    );
    assert_eq!(
        effects("|x", "\"+yy"),
        vec![Effect::Clipboard { text: "x\n".into() }]
    );
    assert_eq!(
        effects("|x", "<C-w>v"),
        vec![Effect::Action {
            id: "SplitVertically".into()
        }]
    );
    assert_eq!(
        effects("|x", "gd"),
        vec![Effect::Action {
            id: "GotoDeclaration".into()
        }]
    );
}

#[test]
fn applies_mappings_and_rc() {
    let (mut v, h) = setup("|x");
    v.load_rc("let mapleader=','\ninoremap jk <Esc>\nnmap ,r :action RenameElement<CR>\nnnoremap Y y$\nnmap <leader>d dd");
    v.feed(h, "ifoojk");
    assert_eq!(show(&v, h), "fo|ox");
    assert_eq!(v.mode(h), Some("normal"));
    let r = v.key(h, Some(",".into()), &Sync::default());
    assert!(r.wait);
    let r = v.key(h, Some("r".into()), &Sync::default());
    assert!(r.effects.contains(&Effect::Action {
        id: "RenameElement".into()
    }));
    // A held key that turns out not to be a mapping is typed after the timeout.
    v.feed(h, "A");
    let r = v.key(h, Some("j".into()), &Sync::default());
    assert!(r.consumed && r.wait);
    let r = v.timeout(h, &Sync::default());
    assert_eq!(r.changes.len(), 1);
    assert_eq!(v.text(h).unwrap(), "fooxj");
}

#[test]
fn mapping_queues_keys_behind_async_effects() {
    let (mut v, h) = setup("|a\nb\nc");
    v.load_rc("nmap Q ujj");
    let r = v.key(h, Some("Q".into()), &Sync::default());
    assert!(r.resume);
    let r = v.key(h, None, &Sync::default());
    assert!(!r.resume);
    assert_eq!(show(&v, h), "a\nb\n|c");
}

#[test]
fn claims_keys_from_the_ide_keymap() {
    let (mut v, h) = setup("|x");
    let ev = |key: &str, code: &str, ctrl: bool| DomKey {
        key: key.into(),
        code: code.into(),
        ctrl_key: ctrl,
        ..DomKey::default()
    };
    assert!(!v.claims(&ev("j", "KeyJ", false)), "no focused editor");
    v.focus(Some(h));
    assert!(v.claims(&ev("j", "KeyJ", false)));
    assert!(v.claims(&ev("r", "KeyR", true)));
    assert!(!v.claims(&ev("s", "KeyS", true)));
    assert!(!v.claims(&ev("F5", "F5", false)));
    let altgr = DomKey {
        alt_key: true,
        ..ev("{", "Digit7", true)
    };
    assert!(v.claims(&altgr), "AltGr characters are typed text");
    let ctrl_alt_v = DomKey {
        alt_key: true,
        ..ev("v", "KeyV", true)
    };
    assert!(!v.claims(&ctrl_alt_v));
    v.feed(h, "i");
    assert!(!v.claims(&ev("j", "KeyJ", false)));
    assert!(v.claims(&ev("Escape", "Escape", false)));
    assert!(v.claims(&ev("w", "KeyW", true)));
    assert!(!v.claims(&ev("v", "KeyV", true)));
    v.handlers.sethandler("<C-W> i:ide").unwrap();
    assert!(!v.claims(&ev("w", "KeyW", true)));
    v.enabled = false;
    assert!(!v.claims(&ev("Escape", "Escape", false)));
}

#[test]
fn follows_editor_changes_and_selection() {
    use forge_vim::Change;
    let (mut v, h) = setup("|hello");
    // The editor typed " world" at the end and put the caret at 0.
    let sync = Sync {
        changes: vec![vec![Change {
            from: 5,
            to: 5,
            insert: " world".into(),
        }]],
        selection: Some(vec![SelRange { anchor: 6, head: 6 }]),
        ..Sync::default()
    };
    let r = v.key(h, Some("x".into()), &sync);
    assert_eq!(v.text(h).unwrap(), "hello orld");
    assert_eq!(
        r.changes,
        vec![Change {
            from: 6,
            to: 7,
            insert: String::new()
        }]
    );
    // A mouse selection enters Visual mode.
    let sync = Sync {
        selection: Some(vec![SelRange { anchor: 0, head: 5 }]),
        ..Sync::default()
    };
    let r = v.key(h, Some("d".into()), &sync);
    assert_eq!(v.text(h).unwrap(), " orld");
    assert_eq!(r.mode, "normal");
}

#[test]
fn reports_utf16_offsets() {
    let (mut v, h) = setup("😀|ab");
    let r = v.key(h, Some("x".into()), &Sync::default());
    assert_eq!(r.changes[0].from, 2);
    assert_eq!(r.cursor, 2);
    assert_eq!(v.text(h).unwrap(), "😀b");
}

#[test]
fn highlights_searches_with_hlsearch() {
    let (mut v, h) = setup("|foo");
    v.load_rc("set hlsearch");
    let r = v.feed(h, "/o<CR>");
    assert_eq!(r.highlight.unwrap().source, "o");
    let r = v.feed(h, ":noh<CR>");
    assert!(r.highlight.is_none());
}
