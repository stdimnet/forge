//! Recorded JSON reports of the real tools (ESLint 9, Stylelint 16 with stylelint-scss,
//! Biome 1.9) parsed into problems.

use webforge_quality_tools::{parse_report, PathContext, Severity, WebTool};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn eslint_messages_become_problems() {
    let ctx = PathContext::new("/home/me/storefront", "/home/me/storefront");
    let r = parse_report(WebTool::Eslint, &fixture("eslint.json"), "", &ctx, &|_| None).unwrap();
    assert_eq!(r.problems.len(), 4);
    assert!(r.outputs.is_empty());
    let unused = &r.problems[0];
    assert_eq!(unused.file, "src/components/Header.tsx");
    assert_eq!(
        (unused.line, unused.column, unused.end_line, unused.end_column),
        (7, 8, Some(7), Some(17))
    );
    assert_eq!(unused.severity, Severity::Error);
    assert_eq!(unused.rule.as_deref(), Some("no-unused-vars"));
    assert_eq!(
        unused.link.as_deref(),
        Some("https://eslint.org/docs/latest/rules/no-unused-vars")
    );
    assert!(!unused.fixable);
    // Suggestions are not fixes of `--fix`.
    let eqeqeq = &r.problems[1];
    assert_eq!(eqeqeq.severity, Severity::Warning);
    assert!(!eqeqeq.fixable);
    assert!(r.problems[2].fixable);
    assert_eq!(
        r.problems[3].link.as_deref(),
        Some("https://typescript-eslint.io/rules/no-explicit-any")
    );
}

#[test]
fn eslint_fix_dry_run_carries_the_fixed_text() {
    let ctx = PathContext::new("/home/me/storefront", "/home/me/storefront")
        .with_stdin("src/hooks/useCart.ts");
    let r = parse_report(
        WebTool::Eslint,
        &format!("(node:1) ExperimentalWarning: …\n{}", fixture("eslint-fix-dry-run.json")),
        "",
        &ctx,
        &|_| None,
    )
    .unwrap();
    assert_eq!(r.outputs.len(), 1);
    assert_eq!(r.outputs[0].file, "src/hooks/useCart.ts");
    assert!(r.outputs[0].text.contains("const total = items.length;"));
    assert_eq!(r.problems.len(), 1);
}

#[test]
fn stylelint_warnings_in_scss() {
    let ctx = PathContext::new("/home/me/admin-portal", "/home/me/admin-portal");
    // Stylelint 16 prints the report on stderr.
    let r = parse_report(WebTool::Stylelint, "", &fixture("stylelint.json"), &ctx, &|_| None)
        .unwrap();
    assert_eq!(r.problems.len(), 4);
    let hex = &r.problems[0];
    assert_eq!(hex.file, "src/app/users/user-card.component.scss");
    assert_eq!((hex.line, hex.column, hex.end_column), (2, 9, Some(13)));
    assert_eq!(hex.message, "Unexpected invalid hex color \"#ffg\"");
    assert_eq!(
        hex.link.as_deref(),
        Some("https://stylelint.io/user-guide/rules/color-no-invalid-hex")
    );
    assert!(hex.fixable);
    let scss = &r.problems[2];
    assert_eq!(scss.severity, Severity::Warning);
    assert_eq!(scss.link, None);
    let syntax = &r.problems[3];
    assert_eq!(syntax.file, "src/styles.scss");
    assert_eq!(syntax.message, "Unclosed block");
    assert!(!syntax.fixable);
    assert_eq!(r.errors.len(), 1);
    assert!(r.errors[0].starts_with("Invalid option value"));
}

#[test]
fn biome_byte_spans_map_to_lines_and_columns() {
    let ctx = PathContext::new("/home/me/orders-api", "/home/me/orders-api");
    // 15 lines of 30 bytes: offset 262 is line 8, column 22.
    let line = format!("{}\n", "x".repeat(29));
    let text = line.repeat(15);
    let source = |f: &str| (f == "src/orders/pricing.ts").then(|| text.clone());
    let r = parse_report(WebTool::Biome, &fixture("biome-lint.json"), "", &ctx, &source).unwrap();
    assert_eq!(r.problems.len(), 2);
    let eq = &r.problems[0];
    assert_eq!(eq.file, "src/orders/pricing.ts");
    assert_eq!(
        (eq.line, eq.column, eq.end_line, eq.end_column),
        (8, 22, Some(8), Some(24))
    );
    assert_eq!(eq.message, "Use === instead of ==");
    assert_eq!(eq.rule.as_deref(), Some("lint/suspicious/noDoubleEquals"));
    assert_eq!(
        eq.link.as_deref(),
        Some("https://biomejs.dev/linter/rules/no-double-equals/")
    );
    assert!(eq.fixable);
    let bang = &r.problems[1];
    assert_eq!(bang.severity, Severity::Warning);
    assert_eq!((bang.line, bang.column), (11, 11));
    assert!(!bang.fixable);
}

#[test]
fn output_without_a_report_is_an_error() {
    let ctx = PathContext::new("/p", "/p");
    let err = parse_report(
        WebTool::Eslint,
        "Oops! Something went wrong! :(\n\nESLint: 9.25.1\n",
        "",
        &ctx,
        &|_| None,
    )
    .unwrap_err();
    assert_eq!(err, "ESLint printed no JSON report");
}
