#![cfg(feature = "engine")]
//! Structural search and replace over fixture files.

use forge_ssr::{check, search_files, Compiled, Constraint, FileType, SsrRequest};

const PHP: &str = include_str!("fixtures/OrderService.php");
const TS: &str = include_str!("fixtures/cart.ts");
const PHP_PATH: &str = "app/Services/OrderService.php";

fn req(template: &str, constraints: Vec<Constraint>) -> SsrRequest {
    SsrRequest {
        template: template.into(),
        constraints,
        ..Default::default()
    }
}

fn matched(r: &SsrRequest) -> Vec<String> {
    let c = Compiled::new(r).unwrap_or_else(|e| panic!("{e}"));
    c.search(PHP_PATH, PHP)
        .into_iter()
        .map(|m| m.matched)
        .collect()
}

fn any(name: &str) -> Constraint {
    Constraint::named(name).count(0, None)
}

#[test]
fn method_calls_with_any_arguments() {
    let found = matched(&req("$Instance$->$Method$($Args$)", vec![any("Args")]));
    assert_eq!(
        found,
        [
            "$order->addItem($item)",
            "$order->total()",
            "$order->flag('review')",
            "$order->save()",
            "$mail->send()",
            "$this->log('placed')",
            "$this->flush()",
            "logger()->info($message)",
        ]
    );
    // Without a count the argument list must hold exactly one argument.
    let found = matched(&req("$Instance$->$Method$($Arg$)", vec![]));
    assert_eq!(found.len(), 4, "{found:?}");
    // A literal method name and a text modifier.
    assert_eq!(matched(&req("$o$->save()", vec![])), ["$order->save()"]);
    let mut c = Constraint::named("Method");
    c.text = "(log|flush)".into();
    let found = matched(&req("$this->$Method$($Args$)", vec![c, any("Args")]));
    assert_eq!(found, ["$this->log('placed')", "$this->flush()"]);
    let mut c = Constraint::named("Method");
    c.text = "(log|flush)".into();
    c.invert_text = true;
    let found = matched(&req("$this->$Method$($Args$)", vec![c, any("Args")]));
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn new_expressions_and_types() {
    let found = matched(&req("new $Class$($Args$)", vec![any("Args")]));
    assert_eq!(
        found,
        [
            "new Order(['customer_id' => $customer->id])",
            "new Invoice($order, true)"
        ]
    );
    // The type of `$order` comes from `$order = new Order(…)`, of `$customer` from the
    // parameter.
    let mut c = Constraint::named("Instance");
    c.type_ = "Order".into();
    let found = matched(&req("$Instance$->$Method$($Args$)", vec![c, any("Args")]));
    assert_eq!(found.len(), 4, "{found:?}");
    let mut c = Constraint::named("Value");
    c.type_ = "Customer".into();
    let found = matched(&req("$Value$->id", vec![c]));
    assert_eq!(found, ["$customer->id"]);
    let mut c = Constraint::named("String");
    c.type_ = "string".into();
    let found = matched(&req("$String$", vec![c]));
    // `$message` is declared `string`.
    assert_eq!(
        found,
        [
            "'customer_id'",
            "'review'",
            "'placed'",
            "'done'",
            "$message",
            "$message"
        ]
    );
}

#[test]
fn statements() {
    let found = matched(&req(
        "if ($Condition$) { $Statement$; }",
        vec![any("Statement")],
    ));
    assert_eq!(found.len(), 1);
    assert!(found[0].starts_with("if ($order->total() > 1000)"));
    let found = matched(&req(
        "try { $Try$; } finally { $Finally$; }",
        vec![any("Try"), any("Finally")],
    ));
    assert_eq!(found.len(), 1, "try without catch");
    assert!(found[0].contains("finally"));
    let found = matched(&req(
        "try { $Try$; } catch ($Type$ $e$) { }",
        vec![any("Try")],
    ));
    assert_eq!(found.len(), 1, "catch with empty body: {found:?}");
    assert!(found[0].contains("RuntimeException"));
    let found = matched(&req(
        "echo $Args$;",
        vec![Constraint::named("Args").count(1, None)],
    ));
    assert_eq!(found, ["echo 'done', PHP_EOL;"]);
    // A sequence of statements.
    let found = matched(&req("$o$->save(); $m$ = new Invoice($a$);", vec![any("a")]));
    assert_eq!(found.len(), 1, "{found:?}");
    // Class members.
    let found = matched(&req(
        "private function $m$($p$) { $s$; }",
        vec![any("p"), any("s")],
    ));
    assert_eq!(
        found.len(),
        1,
        "a template without a return type matches any"
    );
    assert!(found[0].starts_with("private function log"));
    let found = matched(&req(
        "private function $m$($p$): int { $s$; }",
        vec![any("p"), any("s")],
    ));
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_variable_used_twice_matches_the_same_text() {
    let found = matched(&req("$a$->$m$($a$)", vec![]));
    assert!(found.is_empty(), "{found:?}");
    // `$$v$` only matches PHP variables.
    let found = matched(&req("$$v$->save()", vec![]));
    assert_eq!(found, ["$order->save()"]);
    let found = matched(&req("$$v$->info($x$)", vec![]));
    assert!(found.is_empty(), "logger() is not a variable");
}

#[test]
fn positions_and_bindings() {
    let c = Compiled::new(&req("$o$->flag($r$)", vec![])).unwrap();
    let m = &c.search(PHP_PATH, PHP)[0];
    assert_eq!((m.line, m.column), (16, 12));
    assert_eq!(m.text, "$order->flag('review');");
    assert_eq!(m.ranges, [(0, 22)]);
    assert_eq!(m.bindings[0].name, "o");
    assert_eq!(m.bindings[0].text, "$order");
    assert_eq!(m.bindings[1].text, "'review'");
}

#[test]
fn replace_fills_variables_and_keeps_indentation() {
    let mut r = req("$o$->flag($r$)", vec![]);
    r.replacement = Some("$o$->markForReview($r$, true)".into());
    let c = Compiled::new(&r).unwrap();
    let (out, n) = c.replace(PHP_PATH, PHP).unwrap();
    assert_eq!(n, 1);
    assert!(out.contains("            $order->markForReview('review', true);\n"));
    assert_eq!(
        c.search(PHP_PATH, PHP)[0].replacement.as_deref(),
        Some("$order->markForReview('review', true)")
    );

    let mut r = req(
        "try { $Try$; } finally { $Finally$; }",
        vec![any("Try"), any("Finally")],
    );
    r.replacement = Some("try {\n    $Try$;\n} catch (\\Throwable $e) {\n    report($e);\n} finally {\n    $Finally$;\n}".into());
    let (out, n) = Compiled::new(&r).unwrap().replace(PHP_PATH, PHP).unwrap();
    assert_eq!(n, 1);
    assert!(
        out.contains(
            "        } catch (\\Throwable $e) {\n            report($e);\n        } finally {"
        ),
        "{out}"
    );
}

#[test]
fn typescript_and_javascript() {
    let r = SsrRequest {
        template: "console.log($Args$)".into(),
        file_type: FileType::TypeScript,
        constraints: vec![any("Args")],
        ..Default::default()
    };
    let c = Compiled::new(&r).unwrap();
    let found: Vec<String> = c
        .search("src/cart.ts", TS)
        .into_iter()
        .map(|m| m.matched)
        .collect();
    assert_eq!(
        found,
        ["console.log(\"added\", item.id)", "console.log(sum)"]
    );
    assert!(
        c.search("src/cart.php", TS).is_empty(),
        "other file types are skipped"
    );
    let found = c.search("src/Cart.tsx", TS);
    assert_eq!(found.len(), 2, "TSX files use the TSX grammar");

    let r = SsrRequest {
        template: "$Instance$.$Method$($Args$)".into(),
        file_type: FileType::JavaScript,
        constraints: vec![any("Args")],
        ..Default::default()
    };
    let js = "items.push(a);\nconsole.log(a, b);\nfoo();\n";
    let c = Compiled::new(&r).unwrap();
    let found: Vec<String> = c
        .search("a.js", js)
        .into_iter()
        .map(|m| m.matched)
        .collect();
    assert_eq!(found, ["items.push(a)", "console.log(a, b)"]);
}

#[test]
fn search_files_honors_scope_buffers_and_limits() {
    let mut r = req("$o$->save()", vec![]);
    let files = [
        (PHP_PATH, PHP),
        ("tests/OrderTest.php", "<?php $order->save();"),
        ("resources/app.js", "order.save()"),
    ];
    let result = search_files(&r, files);
    assert_eq!(result.match_count, 2);
    assert_eq!(result.searched_files, 2);
    r.scope = Some(forge_scopes::ScopeFilter {
        scope: forge_scopes::ScopeSpec::Pattern("file:app//*".into()),
        ..Default::default()
    });
    let result = search_files(&r, files);
    assert_eq!(result.files.len(), 1);
    assert_eq!(result.files[0].path, PHP_PATH);
    r.buffers
        .insert(PHP_PATH.into(), "<?php $a->save(); $b->save();".into());
    assert_eq!(search_files(&r, files).match_count, 2);
    r.limit = 1;
    let result = search_files(&r, files);
    assert!(result.truncated && result.match_count == 1);
    r.template = "if (".into();
    assert!(search_files(&r, files).error.is_some());
}

#[test]
fn check_reports_variables_and_problems() {
    let mut r = req("$a$->$m$($p$)", vec![]);
    r.replacement = Some("$a$->$other$()".into());
    let c = check(&r);
    assert_eq!(c.variables, ["a", "m", "p"]);
    assert_eq!(c.unknown_in_replacement, ["other"]);
    assert!(c.error.is_none());
    let mut bad = Constraint::named("a");
    bad.text = "(".into();
    let c = check(&req("$a$->x()", vec![bad]));
    assert!(c
        .error
        .unwrap()
        .contains("Invalid text regular expression for $a$"));
    let c = check(&req("$a$ +", vec![]));
    assert_eq!(
        c.error.as_deref(),
        Some("The search template is not a valid PHP code fragment")
    );
}
