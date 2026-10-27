//! Main functions (Run gutter icons) and JUnit tests of a Kotlin file.
//!
//! A top-level `fun main()` / `fun main(args: Array<String>)` runs as the file's facade
//! class (`Main.kt` → `MainKt`, or `@file:JvmName`); a `@JvmStatic fun main` in an `object`
//! or a companion object runs as that object's (or the outer) class. Tests: JUnit 5
//! (`@Test`, `@ParameterizedTest`, `@Nested inner class`, `kotlin.test.Test`), JUnit 4 and
//! TestNG, with backtick names (`` fun `adds stock`() ``).

use javaforge_lang_java::discovery::{TestClass, TestMethod};
use javaforge_lang_java::Framework;
use serde::{Deserialize, Serialize};

use crate::source::{parse, ClassKind, FunDecl, KotlinFile};

/// A runnable `main` function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MainFunction {
    /// 0-based line of the function name.
    pub line: u32,
    /// The JVM class to run (`com.acme.AppKt`, `com.acme.Tool`).
    pub class: String,
    /// The configuration's name (`AppKt`).
    pub name: String,
}

fn is_main_signature(f: &FunDecl) -> bool {
    if f.name != "main" || f.receiver.is_some() || f.has_modifier("private") {
        return false;
    }
    let p = f.params.replace(' ', "");
    p.is_empty()
        || (p.ends_with(":Array<String>") && !p.contains(','))
        || (p.starts_with("vararg") && p.ends_with(":String") && !p.contains(','))
}

/// The `main` functions of a file.
pub fn main_functions(path: &str, text: &str) -> Vec<MainFunction> {
    let file = parse(text);
    mains_of(path, &file)
}

pub(crate) fn mains_of(path: &str, file: &KotlinFile) -> Vec<MainFunction> {
    let mut out = Vec::new();
    if let Some(f) = file.functions.iter().find(|f| is_main_signature(f)) {
        let class = file.facade_class(path);
        out.push(MainFunction {
            line: f.line,
            name: class.rsplit('.').next().unwrap_or(&class).to_string(),
            class,
        });
    }
    for (i, c) in file.classes.iter().enumerate() {
        if !matches!(c.kind, ClassKind::Object | ClassKind::CompanionObject) {
            continue;
        }
        let Some(f) = c.functions.iter().find(|f| {
            is_main_signature(f) && f.has_annotation("JvmStatic") && !f.params.is_empty()
        }) else {
            continue;
        };
        // A companion's static main belongs to the outer class.
        let owner = match (c.kind, c.parent) {
            (ClassKind::CompanionObject, Some(p)) => &file.classes[p],
            _ => &file.classes[i],
        };
        out.push(MainFunction {
            line: f.line,
            class: owner.binary_name.clone(),
            name: owner.name.clone(),
        });
    }
    out
}

/// The JUnit (or TestNG) tests of a Kotlin file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KotlinTestDiscovery {
    pub framework: Framework,
    pub package: Option<String>,
    pub classes: Vec<TestClass>,
}

const TEST_ANNOTATIONS: &[&str] = &[
    "Test",
    "ParameterizedTest",
    "RepeatedTest",
    "TestFactory",
    "TestTemplate",
];

fn framework_of(file: &KotlinFile) -> Option<Framework> {
    let imports = |prefix: &str| file.imports.iter().any(|i| i.starts_with(prefix));
    if imports("org.testng") {
        Some(Framework::TestNg)
    } else if imports("org.junit.jupiter") || imports("kotlin.test") {
        Some(Framework::JUnit5)
    } else if imports("org.junit.") || imports("junit.framework.") {
        Some(Framework::JUnit4)
    } else {
        None
    }
}

/// The test classes and functions of a file, or `None` without tests.
pub fn discover_tests(text: &str) -> Option<KotlinTestDiscovery> {
    tests_of(&parse(text))
}

pub(crate) fn tests_of(file: &KotlinFile) -> Option<KotlinTestDiscovery> {
    let framework = framework_of(file)?;
    let mut classes = Vec::new();
    for c in &file.classes {
        if c.kind != ClassKind::Class || c.has_modifier("abstract") || c.has_modifier("sealed") {
            continue;
        }
        let nested = c.parent.is_some();
        if nested && framework == Framework::JUnit5 && !c.has_annotation("Nested") {
            continue;
        }
        let methods: Vec<TestMethod> = c
            .functions
            .iter()
            .filter(|f| TEST_ANNOTATIONS.iter().any(|a| f.has_annotation(a)))
            .map(|f| TestMethod {
                name: f.name.clone(),
                line: f.line,
            })
            .collect();
        if methods.is_empty() {
            continue;
        }
        classes.push(TestClass {
            name: c.name.clone(),
            class: c.binary_name.clone(),
            line: c.line,
            nested,
            methods,
        });
    }
    (!classes.is_empty()).then(|| KotlinTestDiscovery {
        framework,
        package: file.package.clone(),
        classes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_top_level_and_static_mains() {
        let app = "package com.acme\n\nfun main(args: Array<String>) {\n  println(args)\n}\n";
        assert_eq!(
            main_functions("app/src/main/kotlin/com/acme/App.kt", app),
            [MainFunction {
                line: 2,
                class: "com.acme.AppKt".into(),
                name: "AppKt".into()
            }]
        );
        assert_eq!(
            main_functions("Hello.kt", "suspend fun main() = coroutineScope { }")[0].class,
            "HelloKt"
        );
        let named = "@file:JvmName(\"Launcher\")\npackage a\nfun main() {}\n";
        assert_eq!(main_functions("x.kt", named)[0].class, "a.Launcher");
        let objects = "package a\nobject Tool {\n  @JvmStatic fun main(args: Array<String>) {}\n}\nclass Server {\n  companion object {\n    @JvmStatic\n    fun main(args: Array<String>) {}\n  }\n}\n";
        let mains = main_functions("Tool.kt", objects);
        assert_eq!(
            mains
                .iter()
                .map(|m| (m.class.as_str(), m.line))
                .collect::<Vec<_>>(),
            [("a.Tool", 2), ("a.Server", 7)]
        );
        assert!(main_functions("a.kt", "fun main(x: Int) {}").is_empty());
        assert!(main_functions("a.kt", "private fun main() {}").is_empty());
        assert!(main_functions("a.kt", "fun String.main() {}").is_empty());
        assert!(main_functions("a.kt", "object O { fun main(args: Array<String>) {} }").is_empty());
    }

    const JUNIT5: &str = r#"package com.acme.cart

import org.junit.jupiter.api.Nested
import org.junit.jupiter.api.Test
import org.junit.jupiter.params.ParameterizedTest

class CartTest {
    @Test
    fun `adds items to the cart`() {
        assertEquals(1, Cart().add("a").size)
    }

    @ParameterizedTest
    @ValueSource(ints = [1, 2])
    fun rejectsNegative(qty: Int) {}

    fun helper() {}

    @Nested
    inner class Totals {
        @Test fun `sums prices`() {}
    }

    class NotNested { @Test fun ignored() {} }
}

abstract class BaseTest { @Test fun inherited() {} }
"#;

    #[test]
    fn discovers_junit5_with_backtick_names_and_nested_classes() {
        let d = discover_tests(JUNIT5).unwrap();
        assert_eq!(d.framework, Framework::JUnit5);
        assert_eq!(d.package.as_deref(), Some("com.acme.cart"));
        assert_eq!(d.classes.len(), 2);
        let c = &d.classes[0];
        assert_eq!((c.class.as_str(), c.line), ("com.acme.cart.CartTest", 6));
        let names: Vec<&str> = c.methods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["adds items to the cart", "rejectsNegative"]);
        assert_eq!(c.methods[0].line, 8);
        assert_eq!(d.classes[1].class, "com.acme.cart.CartTest$Totals");
        assert!(d.classes[1].nested);
        let kt = "import kotlin.test.Test\nclass A { @Test fun a() {} }";
        assert_eq!(discover_tests(kt).unwrap().framework, Framework::JUnit5);
        let j4 = "import org.junit.Test\nclass B { @Test fun b() {} }";
        assert_eq!(discover_tests(j4).unwrap().framework, Framework::JUnit4);
        assert!(discover_tests("class Plain { fun a() {} }").is_none());
    }
}
