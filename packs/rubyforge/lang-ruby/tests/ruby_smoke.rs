//! Smoke test with a real Ruby and Minitest: run a project's tests with the command line
//! the provider builds and read the output with forge-output's Minitest parser. Ignored by
//! default; run with `cargo test -p rubyforge-lang-ruby -- --ignored` (`ruby` on PATH).

use forge_output::formats::{line_parser, MinitestParser};
use forge_output::testevents::{LineParser, TestEvent};
use forge_runtools::Configuration;
use forge_testing::{CommandContext, TestFrameworkProvider};
use rubyforge_lang_ruby::{RubyContext, RubyTests, CONTEXT_KEY, MINITEST};

const TEST: &str = "require \"minitest/autorun\"\n\nclass CartTest < Minitest::Test\n  def test_adds\n    assert_equal 2, [1, 2].size\n  end\n\n  def test_total\n    assert_equal 1050, 1049\n  end\nend\n";

#[test]
#[ignore]
fn minitest_runs_through_the_provider() {
    if std::process::Command::new("ruby")
        .arg("-v")
        .output()
        .is_err()
    {
        eprintln!("no ruby: skipped");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("test")).unwrap();
    std::fs::write(root.path().join("test/cart_test.rb"), TEST).unwrap();
    let mut ctx = CommandContext::default();
    ctx.settings.insert(
        CONTEXT_KEY.into(),
        serde_json::to_string(&RubyContext::default()).unwrap(),
    );
    let p = RubyTests::minitest();
    for cfg in [
        Configuration::new("All", MINITEST),
        Configuration::new("File", MINITEST)
            .with_option("scope", "file")
            .with_option("file", "test/cart_test.rb"),
    ] {
        let cmd = p.command(&cfg, &ctx).unwrap().unwrap();
        assert!(line_parser(cmd.report).is_some());
        let out = std::process::Command::new(&cmd.program)
            .args(&cmd.args)
            .current_dir(root.path())
            .output()
            .unwrap();
        let mut parser = MinitestParser::new();
        let mut events = Vec::new();
        for l in String::from_utf8_lossy(&out.stdout).lines() {
            events.extend(parser.line(l).events);
        }
        events.extend(parser.finish());
        let failed: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                TestEvent::TestFailed { name, location, .. } => {
                    Some((name.clone(), location.clone()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(failed.len(), 1, "{events:?}");
        assert_eq!(failed[0].0, "test_total");
        assert_eq!(failed[0].1.as_ref().and_then(|l| l.line), Some(9));
        let finished = events
            .iter()
            .filter(|e| matches!(e, TestEvent::TestFinished { .. }))
            .count();
        assert_eq!(finished, 2);
    }
}
