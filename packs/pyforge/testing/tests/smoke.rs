//! Smoke tests with a real Python 3 on this machine (`python3` on the PATH, standard
//! library only): Forge's unittest runner reports TeamCity messages the test tree reads,
//! and cProfile's `.pstats` files parse. Ignored by default; run with
//! `cargo test -p pyforge-testing --test smoke -- --ignored`.

use std::path::PathBuf;
use std::process::Command;

use forge_output::links::LinkContext;
use forge_output::{LineParser, TestTree};
use forge_runtools::Configuration;
use pyforge_testing::command::{self, Context};
use pyforge_testing::pstats;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pyforge-smoke-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    dir
}

#[test]
#[ignore = "needs python3 on the PATH"]
fn unittest_runner_reports_a_tree() {
    let dir = scratch("unittest");
    std::fs::write(dir.join("tests/__init__.py"), "").unwrap();
    std::fs::write(
        dir.join("tests/test_cart.py"),
        "import unittest\n\nclass TestCart(unittest.TestCase):\n    def test_total(self):\n        self.assertEqual(2, 2)\n\n    def test_name(self):\n        self.assertEqual('Grace', 'Amazing Grace')\n\n    @unittest.skip('later')\n    def test_skip(self):\n        pass\n",
    )
    .unwrap();
    let cfg = Configuration::new("All", "unittest");
    let cmd = command::unittest(
        &cfg,
        &Context {
            cwd: dir.to_string_lossy().into_owned(),
            ..Context::default()
        },
    )
    .unwrap();
    let out = Command::new("python3")
        .args(&cmd.args)
        .current_dir(&dir)
        .output()
        .expect("python3");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut parser = forge_output::testevents::TeamCityParser;
    let mut tree = TestTree::new("Test Results", LinkContext::new(dir.to_str(), None));
    for line in stdout.lines() {
        for e in parser.line(line).events {
            tree.apply_event(&e);
        }
    }
    tree.finish(out.status.code());
    let counts = tree.counts();
    assert_eq!(
        (counts.total, counts.passed, counts.failed, counts.ignored),
        (3, 1, 1, 1),
        "{stdout}"
    );
    let failed = tree
        .snapshot()
        .into_iter()
        .find(|n| n.name == "test_name")
        .unwrap();
    assert_eq!(failed.expected.as_deref(), Some("Amazing Grace"));
    assert_eq!(failed.actual.as_deref(), Some("Grace"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs python3 on the PATH"]
fn cprofile_snapshots_parse() {
    let dir = scratch("profile");
    std::fs::write(
        dir.join("work.py"),
        "def fib(n):\n    return n if n < 2 else fib(n - 1) + fib(n - 2)\n\nprint(fib(15))\n",
    )
    .unwrap();
    let file = dir.join("work.pstats");
    let mut args = command::profile_args(&file.to_string_lossy(), "");
    args.push("work.py".into());
    let status = Command::new("python3")
        .args(&args)
        .current_dir(&dir)
        .status()
        .expect("python3");
    assert!(status.success());
    let p = pstats::parse(&std::fs::read(&file).unwrap()).unwrap();
    let fib = p.functions.iter().find(|f| f.name == "fib").expect("fib");
    assert_eq!(fib.calls, 1973);
    assert_eq!(fib.primitive_calls, 1);
    let tree = p.call_tree(pstats::TreeLimits::default());
    assert!(!tree.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
