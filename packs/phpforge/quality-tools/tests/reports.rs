//! Recorded reports of each tool (`tests/fixtures`), parsed as the app parses them.

use forge_runtargets::{Mapper, PathMapping};
use phpforge_quality_tools::{
    diff, parse_report, PathContext, Problem, QualityTool, Report, Severity, SeverityChoice,
};

const ROOT: &str = "/home/dev/shop";

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn parse(tool: QualityTool, name: &str, ctx: &PathContext) -> Report {
    parse_report(tool, &fixture(name), ctx, SeverityChoice::Tool).unwrap()
}

fn at(p: &Problem) -> (&str, u32, u32, Severity) {
    (p.file.as_str(), p.line, p.column, p.severity)
}

#[test]
fn phpstan_in_docker_maps_container_paths_back() {
    let ctx = PathContext {
        root: ROOT.into(),
        cwd: "/opt/project".into(),
        mapper: Mapper::new(
            ROOT,
            &[PathMapping {
                local: "".into(),
                remote: "/opt/project".into(),
            }],
        ),
        aliases: Vec::new(),
    };
    let r = parse(QualityTool::Phpstan, "phpstan.json", &ctx);
    assert_eq!(r.problems.len(), 3);
    let p = &r.problems[0];
    assert_eq!(
        at(p),
        (
            "app/Http/Controllers/PaymentController.php",
            23,
            0,
            Severity::Error
        )
    );
    assert_eq!(p.rule.as_deref(), Some("method.notFound"));
    assert!(r.problems[1]
        .message
        .ends_with("Tip: Learn more at https://phpstan.org/user-guide/return-types"));
    assert_eq!(r.problems[2].file, "app/Models/User.php");
    assert!(r.errors.is_empty());
}

#[test]
fn psalm_has_ranges_and_links() {
    let r = parse(QualityTool::Psalm, "psalm.json", &PathContext::local(ROOT));
    assert_eq!(r.problems.len(), 2);
    let p = &r.problems[0];
    assert_eq!(at(p), ("app/Support/Money.php", 11, 15, Severity::Error));
    assert_eq!((p.end_line, p.end_column), (Some(11), Some(22)));
    assert_eq!(p.rule.as_deref(), Some("UndefinedVariable"));
    assert_eq!(p.link.as_deref(), Some("https://psalm.dev/024"));
    // Psalm's "info" issues are weak warnings.
    assert_eq!(r.problems[1].severity, Severity::Weak);
}

#[test]
fn phpcs_reports_errors_warnings_and_fixable_problems() {
    let r = parse(QualityTool::Phpcs, "phpcs.json", &PathContext::local(ROOT));
    let p: Vec<_> = r.problems.iter().map(at).collect();
    assert_eq!(
        p,
        vec![
            ("app/Models/User.php", 8, 23, Severity::Error),
            ("app/Models/User.php", 39, 1, Severity::Error),
            ("app/Models/User.php", 26, 134, Severity::Warning),
        ]
    );
    assert!(r.problems[0].fixable && !r.problems[2].fixable);
    assert_eq!(
        r.problems[0].rule.as_deref(),
        Some("PSR2.Classes.ClassDeclaration.OpenBraceNewLine")
    );
}

#[test]
fn phpcs_on_stdin_reports_the_edited_file() {
    let mut ctx = PathContext::local(ROOT);
    ctx.aliases = vec![("STDIN".into(), format!("{ROOT}/app/Models/User.php"))];
    let r = parse(QualityTool::Phpcs, "phpcs-stdin.json", &ctx);
    assert_eq!(
        at(&r.problems[0]),
        ("app/Models/User.php", 2, 13, Severity::Error)
    );
}

#[test]
fn phpmd_priorities_become_severities() {
    let r = parse(QualityTool::Phpmd, "phpmd.json", &PathContext::local(ROOT));
    assert_eq!(r.problems.len(), 2);
    assert_eq!(
        at(&r.problems[0]),
        ("app/Services/PaymentGateway.php", 20, 0, Severity::Weak)
    );
    assert_eq!(r.problems[1].severity, Severity::Warning);
    assert_eq!(
        r.problems[1].link.as_deref(),
        Some("https://phpmd.org/rules/codesize.html#cyclomaticcomplexity")
    );
    assert_eq!(
        r.errors,
        vec!["app/Broken.php: Unexpected token: }, line: 4, col: 1, file: /home/dev/shop/app/Broken.php."]
    );
}

#[test]
fn php_cs_fixer_problems_sit_on_the_changed_lines_and_the_diff_applies() {
    let r = parse(
        QualityTool::PhpCsFixer,
        "php-cs-fixer.json",
        &PathContext::local(ROOT),
    );
    assert_eq!(r.fixes.len(), 1);
    assert_eq!(r.fixes[0].file, "app/Support/Money.php");
    assert_eq!(
        r.fixes[0].rules,
        vec!["array_syntax", "binary_operator_spaces"]
    );
    let lines: Vec<u32> = r.problems.iter().map(|p| p.line).collect();
    assert_eq!(lines, vec![6, 12]);
    assert!(r
        .problems
        .iter()
        .all(|p| p.fixable && p.severity == Severity::Weak));
    assert_eq!(
        r.problems[0].message,
        "PHP CS Fixer: array_syntax, binary_operator_spaces"
    );
    let text = "<?php\n\nnamespace App\\Support;\n\nfinal class Money\n{\n    private $parts = array(1,2);\n\n    public function __construct(private int $cents)\n    {\n    }\n\n    {\n        return $this->cents/100;\n    }\n}\n";
    let fixed = diff::apply(text, &r.fixes[0].diff).unwrap();
    assert!(fixed.contains("private $parts = [1,2];"));
    assert!(fixed.contains("return $this->cents / 100;"));
}

#[test]
fn pint_without_a_diff_marks_the_file() {
    let r = parse(QualityTool::Pint, "pint.json", &PathContext::local(ROOT));
    assert_eq!(r.problems.len(), 1);
    let p = &r.problems[0];
    assert_eq!(
        at(p),
        (
            "app/Http/Controllers/PaymentController.php",
            0,
            0,
            Severity::Weak
        )
    );
    assert_eq!(
        p.message,
        "Laravel Pint: ordered_imports, no_unused_imports"
    );
}

#[test]
fn rector_reports_its_diff_rules_and_errors() {
    let r = parse(
        QualityTool::Rector,
        "rector.json",
        &PathContext::local(ROOT),
    );
    assert_eq!(r.fixes.len(), 1);
    let fix = &r.fixes[0];
    assert_eq!(fix.file, "app/Services/PaymentGateway.php");
    assert_eq!(
        r.problems[0].message,
        "Rector: ClassPropertyAssignToConstructorPromotionRector"
    );
    assert_eq!((r.problems[0].line, r.problems[0].end_line), (10, Some(17)));
    // A file Rector could not parse is an error in that file.
    assert_eq!(
        at(&r.problems[1]),
        ("app/Broken.php", 3, 0, Severity::Error)
    );
    let text = "<?php\n\nnamespace App\\Services;\n\nuse RuntimeException;\n\n/** Gateway. */\n\nfinal class PaymentGateway\n{\n    private string $apiKey;\n\n    private int $retries;\n\n    public function __construct(string $apiKey, int $retries = 3)\n    {\n        $this->apiKey = $apiKey;\n        $this->retries = $retries;\n    }\n\n    public function charge(int $cents, string $currency): bool\n    {\n        return true;\n    }\n}\n";
    let fixed = diff::apply(text, &fix.diff).unwrap();
    assert!(fixed.contains(
        "    public function __construct(private string $apiKey, private int $retries = 3)\n    {\n    }\n"
    ));
    assert!(!fixed.contains("$this->apiKey = $apiKey;"));
}
