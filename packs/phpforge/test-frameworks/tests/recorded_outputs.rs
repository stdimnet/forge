//! Recorded runner output (Pest `--teamcity`, a Codeception `--xml` report) through the
//! test tree, checked against the names the gutter discovery gives the same tests.

use forge_output::links::{LinkContext, OutputPathMapping};
use forge_output::{
    junit_messages, parse_service_message, JUnitFlavor, TestNodeKind, TestStatus, TestTree,
};
use phpforge_test_frameworks::discover;

const PEST_FILE: &str = include_str!("fixtures/CartTest.php");
const PEST_OUTPUT: &str = include_str!("fixtures/pest-teamcity.txt");
const CODECEPTION_REPORT: &str = include_str!("fixtures/codeception-report.xml");

/// `<path>::<member>` of every test, like the UI's result keys.
fn keys(tree: &TestTree) -> Vec<(String, TestStatus)> {
    tree.snapshot()
        .into_iter()
        .filter(|n| n.kind == TestNodeKind::Test)
        .map(|n| {
            let loc = n.location.unwrap();
            let member = loc.member.unwrap_or(n.name);
            (
                format!("{}::{member}", loc.workspace_path.unwrap()),
                n.status,
            )
        })
        .collect()
}

#[test]
fn pest_teamcity_names_match_the_discovered_tests() {
    let mut tree = TestTree::new("Test Results", LinkContext::new(Some("/p"), None));
    for line in PEST_OUTPUT.lines() {
        if let Some(m) = parse_service_message(line) {
            tree.apply(&m);
        }
    }
    tree.finish(Some(1));
    assert_eq!(
        tree.summary().text,
        "Tests failed: 1, passed: 2 of 3 tests – 10 ms"
    );
    let discovered: Vec<String> = discover("tests/Feature/CartTest.php", PEST_FILE)
        .targets
        .into_iter()
        .map(|t| t.key)
        .filter(|k| !k.is_empty())
        .collect();
    for (key, _) in keys(&tree) {
        assert!(discovered.contains(&key), "{key} not in {discovered:?}");
    }
    let failed = tree
        .snapshot()
        .into_iter()
        .find(|n| n.status == TestStatus::Failed && n.kind == TestNodeKind::Test)
        .unwrap();
    assert_eq!(failed.expected.as_deref(), Some("1.18"));
}

#[test]
fn codeception_report_through_a_container_mapping() {
    let links =
        LinkContext::new(Some("/home/me/shop"), None).with_mappings(vec![OutputPathMapping {
            remote: "/srv/app".into(),
            local: "/home/me/shop".into(),
        }]);
    let mut tree = TestTree::new("Test Results", links);
    for m in junit_messages(CODECEPTION_REPORT, JUnitFlavor::Generic).unwrap() {
        tree.apply(&m);
    }
    tree.finish(Some(1));
    let nodes = tree.snapshot();
    let bad = nodes
        .iter()
        .find(|n| n.name == "loginWithWrongPassword")
        .unwrap();
    assert_eq!(bad.status, TestStatus::Failed);
    assert_eq!(
        bad.message.as_deref(),
        Some("Failed asserting that  on page /login")
    );
    let loc = bad.location.as_ref().unwrap();
    assert_eq!(
        (loc.workspace_path.as_deref(), loc.line),
        (Some("tests/Acceptance/LoginCest.php"), Some(22))
    );
    let discovered = discover(
        "tests/Acceptance/LoginCest.php",
        "<?php\nclass LoginCest\n{\n    public function loginWithWrongPassword($I) {}\n}\n",
    );
    assert_eq!(
        discovered.targets[1].key,
        format!("tests/Acceptance/LoginCest.php::{}", bad.name)
    );
}
