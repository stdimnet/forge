//! Built-in templates (the dialog's Existing Templates), grouped like PHPStorm's.

use serde::{Deserialize, Serialize};

use crate::{Constraint, FileType};

/// A saved search template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedTemplate {
    pub name: String,
    /// "Expressions", "Statements", "Class-based", …
    pub category: String,
    pub file_type: FileType,
    pub template: String,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
}

fn t(
    name: &str,
    category: &str,
    file_type: FileType,
    template: &str,
    constraints: Vec<Constraint>,
) -> SavedTemplate {
    SavedTemplate {
        name: name.into(),
        category: category.into(),
        file_type,
        template: template.into(),
        constraints,
        replacement: None,
    }
}

fn any(name: &str) -> Constraint {
    Constraint::named(name).count(0, None)
}

/// The built-in templates.
pub fn builtin() -> Vec<SavedTemplate> {
    use FileType::*;
    vec![
        t(
            "method calls",
            "Expressions",
            Php,
            "$Instance$->$MethodCall$($Parameter$)",
            vec![any("Parameter")],
        ),
        t(
            "static method calls",
            "Expressions",
            Php,
            "$Class$::$MethodCall$($Parameter$)",
            vec![any("Parameter")],
        ),
        t(
            "function calls",
            "Expressions",
            Php,
            "$Function$($Parameter$)",
            vec![any("Parameter")],
        ),
        t(
            "new expressions",
            "Expressions",
            Php,
            "new $Class$($Parameter$)",
            vec![any("Parameter")],
        ),
        t(
            "assignments",
            "Expressions",
            Php,
            "$Variable$ = $Value$",
            vec![],
        ),
        t(
            "string literals",
            "Expressions",
            Php,
            "$String$",
            vec![Constraint {
                type_: "string".into(),
                ..Constraint::named("String")
            }],
        ),
        t(
            "if statements",
            "Statements",
            Php,
            "if ($Condition$) {\n    $ThenStatement$;\n}",
            vec![any("ThenStatement")],
        ),
        t(
            "foreach loops",
            "Statements",
            Php,
            "foreach ($Array$ as $Value$) {\n    $Statement$;\n}",
            vec![any("Statement")],
        ),
        t(
            "try without catch",
            "Statements",
            Php,
            "try {\n    $TryStatement$;\n} finally {\n    $FinallyStatement$;\n}",
            vec![any("TryStatement"), any("FinallyStatement")],
        ),
        t(
            "catch with empty body",
            "Statements",
            Php,
            "try {\n    $TryStatement$;\n} catch ($ExceptionType$ $Exception$) {\n}",
            vec![any("TryStatement")],
        ),
        t(
            "echo statements",
            "Statements",
            Php,
            "echo $Argument$;",
            vec![Constraint::named("Argument").count(1, None)],
        ),
        t(
            "methods of a class",
            "Class-based",
            Php,
            "public function $Method$($Parameter$) {\n    $Statement$;\n}",
            vec![any("Parameter"), any("Statement")],
        ),
        t(
            "method calls",
            "Expressions",
            JavaScript,
            "$Instance$.$MethodCall$($Parameter$)",
            vec![any("Parameter")],
        ),
        t(
            "new expressions",
            "Expressions",
            JavaScript,
            "new $Class$($Parameter$)",
            vec![any("Parameter")],
        ),
        t(
            "console.log calls",
            "Expressions",
            JavaScript,
            "console.log($Argument$)",
            vec![any("Argument")],
        ),
        t(
            "if statements",
            "Statements",
            JavaScript,
            "if ($Condition$) {\n    $ThenStatement$;\n}",
            vec![any("ThenStatement")],
        ),
        t(
            "method calls",
            "Expressions",
            TypeScript,
            "$Instance$.$MethodCall$($Parameter$)",
            vec![any("Parameter")],
        ),
        t(
            "variables with a type",
            "Statements",
            TypeScript,
            "let $Variable$: $Type$ = $Value$;",
            vec![],
        ),
    ]
}

#[cfg(all(test, feature = "engine"))]
mod tests {
    use super::*;
    use crate::{Compiled, SsrRequest};

    #[test]
    fn every_builtin_template_compiles() {
        for t in builtin() {
            let req = SsrRequest {
                template: t.template.clone(),
                file_type: t.file_type,
                constraints: t.constraints.clone(),
                ..Default::default()
            };
            if let Err(e) = Compiled::new(&req) {
                panic!("{} ({:?}): {e}", t.name, t.file_type);
            }
        }
    }
}
