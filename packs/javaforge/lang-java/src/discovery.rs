//! Main methods (Run gutter icons) and tests (JUnit 5, JUnit 4/3, TestNG) of a Java file.

use serde::{Deserialize, Serialize};

use crate::source::{parse, JavaFile, MethodDecl, TypeDecl, TypeKind};

/// A runnable `main` method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MainMethod {
    /// 0-based line of the method name.
    pub line: u32,
    /// The class to run (binary name, `com.acme.App`).
    pub class: String,
    /// Simple class name (`App`), the configuration's name.
    pub name: String,
}

fn stem(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    file.strip_suffix(".java").unwrap_or(file).to_string()
}

fn string_array(params: &str) -> bool {
    if params.contains(',') {
        return false;
    }
    let p = params.replace("final ", "").replace(' ', "");
    let p = p.strip_prefix("java.lang.").unwrap_or(&p);
    p.starts_with("String[]") || p.starts_with("String...")
}

/// `public static void main(String[] args)`, or (Java 25) an instance `void main()` /
/// `void main(String[] args)` that is not private.
fn is_main(m: &MethodDecl) -> bool {
    if m.name != "main" || m.return_type != "void" || m.has_modifier("private") {
        return false;
    }
    let params = m.params.trim();
    let args_ok = params.is_empty() || string_array(params);
    if m.has_modifier("static") {
        return string_array(params);
    }
    args_ok && !m.has_modifier("abstract")
}

/// The `main` methods of a file, one per class (a static one wins over an instance one).
pub fn main_methods(path: &str, text: &str) -> Vec<MainMethod> {
    let file = parse(text);
    let mut out = Vec::new();
    for t in &file.types {
        if t.kind == TypeKind::Annotation {
            continue;
        }
        let mains: Vec<&MethodDecl> = t.methods.iter().filter(|m| is_main(m)).collect();
        let best = mains
            .iter()
            .find(|m| m.has_modifier("static"))
            .or(mains.first());
        if let Some(m) = best {
            out.push(MainMethod {
                line: m.line,
                class: t.binary_name.clone(),
                name: t.name.clone(),
            });
        }
    }
    // A compact source file: its top-level methods belong to an implicit class.
    if let Some(m) = file.top_level_methods.iter().find(|m| is_main(m)) {
        out.push(MainMethod {
            line: m.line,
            class: stem(path),
            name: stem(path),
        });
    }
    out
}

/// A test framework of a test class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Framework {
    JUnit5,
    /// JUnit 4 (and JUnit 3 `TestCase` subclasses).
    JUnit4,
    TestNg,
}

impl Framework {
    /// The run configuration type (`junit`, `testng`).
    pub fn type_id(self) -> &'static str {
        match self {
            Framework::JUnit5 | Framework::JUnit4 => "junit",
            Framework::TestNg => "testng",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestMethod {
    pub name: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestClass {
    pub name: String,
    /// Binary name (`com.acme.CartTest$Totals`).
    pub class: String,
    pub line: u32,
    /// A nested class (JUnit 5 `@Nested`).
    pub nested: bool,
    pub methods: Vec<TestMethod>,
}

/// The tests of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestDiscovery {
    pub framework: Framework,
    pub package: Option<String>,
    pub classes: Vec<TestClass>,
}

const JUNIT5_TESTS: &[&str] = &[
    "Test",
    "ParameterizedTest",
    "RepeatedTest",
    "TestFactory",
    "TestTemplate",
];

const TESTNG_CONFIG: &[&str] = &[
    "BeforeMethod",
    "AfterMethod",
    "BeforeClass",
    "AfterClass",
    "BeforeSuite",
    "AfterSuite",
    "BeforeTest",
    "AfterTest",
    "BeforeGroups",
    "AfterGroups",
    "DataProvider",
    "Factory",
];

fn framework_of(file: &JavaFile) -> Option<Framework> {
    let imports = |prefix: &str| {
        file.imports
            .iter()
            .any(|i| i.trim_start_matches("static ").starts_with(prefix))
    };
    let annotated = |prefix: &str| {
        file.types.iter().any(|t| {
            t.annotations.iter().any(|a| a.starts_with(prefix))
                || t.methods
                    .iter()
                    .any(|m| m.annotations.iter().any(|a| a.starts_with(prefix)))
        })
    };
    if imports("org.testng") || annotated("org.testng.") {
        Some(Framework::TestNg)
    } else if imports("org.junit.jupiter") || annotated("org.junit.jupiter.") {
        Some(Framework::JUnit5)
    } else if imports("org.junit.") || imports("junit.framework.") || annotated("org.junit.") {
        Some(Framework::JUnit4)
    } else {
        None
    }
}

fn test_methods(framework: Framework, t: &TypeDecl) -> Vec<TestMethod> {
    let class_level_testng = framework == Framework::TestNg && t.has_annotation("Test");
    let junit3 = framework == Framework::JUnit4
        && t.extends
            .as_deref()
            .is_some_and(|e| e == "TestCase" || e.ends_with(".TestCase"));
    t.methods
        .iter()
        .filter(|m| {
            let annotated = match framework {
                Framework::JUnit5 => JUNIT5_TESTS.iter().any(|a| m.has_annotation(a)),
                Framework::JUnit4 | Framework::TestNg => m.has_annotation("Test"),
            };
            annotated
                || (class_level_testng
                    && m.has_modifier("public")
                    && !m.has_modifier("static")
                    && !m.return_type.is_empty()
                    && !TESTNG_CONFIG.iter().any(|a| m.has_annotation(a)))
                || (junit3
                    && m.name.starts_with("test")
                    && m.has_modifier("public")
                    && m.return_type == "void"
                    && m.params.trim().is_empty())
        })
        .map(|m| TestMethod {
            name: m.name.clone(),
            line: m.line,
        })
        .collect()
}

/// The test classes and methods of a file, or `None` without tests.
pub fn discover_tests(text: &str) -> Option<TestDiscovery> {
    let file = parse(text);
    let framework = framework_of(&file)?;
    let mut classes = Vec::new();
    for t in &file.types {
        if t.kind != TypeKind::Class || t.has_modifier("abstract") {
            continue;
        }
        // Nested classes run with their outer class unless they are static or @Nested.
        let nested = t.parent.is_some();
        if nested && framework == Framework::JUnit5 && !t.has_annotation("Nested") {
            continue;
        }
        let methods = test_methods(framework, t);
        if methods.is_empty() {
            continue;
        }
        classes.push(TestClass {
            name: t.name.clone(),
            class: t.binary_name.clone(),
            line: t.line,
            nested,
            methods,
        });
    }
    (!classes.is_empty()).then(|| TestDiscovery {
        framework,
        package: file.package.clone(),
        classes,
    })
}

/// The source root of a file in a package: `app/src/test/java/com/acme/CartTest.java` in
/// `com.acme` → `app/src/test/java/`.
pub fn source_root(path: &str, package: Option<&str>) -> String {
    let path = path.replace('\\', "/");
    let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let pkg_dir = package.unwrap_or_default().replace('.', "/");
    let root = if pkg_dir.is_empty() {
        dir
    } else {
        dir.strip_suffix(&pkg_dir).unwrap_or(dir)
    };
    let root = root.trim_end_matches('/');
    if root.is_empty() {
        String::new()
    } else {
        format!("{root}/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_static_and_instance_mains() {
        let app = "package com.acme;\npublic class App {\n  public static void main(String[] args) {}\n  static class Tool { static void main(String... a) {} }\n  class NotMain { int main(String[] a) { return 0; } }\n}\n";
        let mains = main_methods("src/main/java/com/acme/App.java", app);
        assert_eq!(
            mains,
            [
                MainMethod {
                    line: 2,
                    class: "com.acme.App".into(),
                    name: "App".into()
                },
                MainMethod {
                    line: 3,
                    class: "com.acme.App$Tool".into(),
                    name: "Tool".into()
                },
            ]
        );
        let modern = "class Hello {\n  void main() { System.out.println(1); }\n}\n";
        assert_eq!(main_methods("Hello.java", modern)[0].class, "Hello");
        let compact = "void main() {\n  IO.println(\"Hi\");\n}\n";
        assert_eq!(main_methods("scripts/Hi.java", compact)[0].class, "Hi");
        assert!(main_methods(
            "A.java",
            "class A { private static void main(String[] a) {} }"
        )
        .is_empty());
        assert!(main_methods("A.java", "class A { static void main() {} }").is_empty());
    }

    const JUNIT5: &str = r#"package com.acme.inventory;

import org.junit.jupiter.api.*;
import org.junit.jupiter.params.ParameterizedTest;

class InventoryTest {
    @BeforeEach void setUp() {}

    @Test
    void addsStock() {}

    @ParameterizedTest
    @ValueSource(ints = {1, 2})
    void rejectsNegative(int qty) {}

    void helper() {}

    @Nested
    class WhenEmpty {
        @Test void isEmpty() {}
    }

    static class Fixtures { @Test void notNested() {} }
}

abstract class BaseTest { @Test void inherited() {} }
"#;

    #[test]
    fn discovers_junit5_classes_methods_and_nested() {
        let d = discover_tests(JUNIT5).unwrap();
        assert_eq!(d.framework, Framework::JUnit5);
        assert_eq!(d.classes.len(), 2);
        let c = &d.classes[0];
        assert_eq!(c.class, "com.acme.inventory.InventoryTest");
        assert_eq!(c.line, 5);
        let names: Vec<&str> = c.methods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["addsStock", "rejectsNegative"]);
        assert_eq!(c.methods[0].line, 9);
        assert_eq!(
            d.classes[1].class,
            "com.acme.inventory.InventoryTest$WhenEmpty"
        );
        assert!(d.classes[1].nested);
    }

    #[test]
    fn discovers_junit4_junit3_and_testng() {
        let j4 = "import org.junit.Test;\npublic class A { @Test public void a() {} public void b() {} }";
        let d = discover_tests(j4).unwrap();
        assert_eq!(d.framework, Framework::JUnit4);
        assert_eq!(d.classes[0].methods.len(), 1);
        let j3 = "import junit.framework.TestCase;\npublic class B extends TestCase { public void testX() {} public void other() {} private void testY() {} }";
        let d = discover_tests(j3).unwrap();
        assert_eq!(d.classes[0].methods[0].name, "testX");
        assert_eq!(d.classes[0].methods.len(), 1);
        let ng = "import org.testng.annotations.*;\n@Test\npublic class C {\n  @BeforeMethod public void init() {}\n  public void one() {}\n  @Test(groups = \"slow\") public void two() {}\n  void hidden() {}\n}";
        let d = discover_tests(ng).unwrap();
        assert_eq!(d.framework, Framework::TestNg);
        let names: Vec<&str> = d.classes[0]
            .methods
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(names, ["one", "two"]);
        assert_eq!(Framework::TestNg.type_id(), "testng");
        assert!(discover_tests("class Plain { void a() {} }").is_none());
        assert!(discover_tests(
            "import org.junit.jupiter.api.Test;\ninterface I { @Test default void a() {} }"
        )
        .is_none());
    }

    #[test]
    fn source_roots() {
        assert_eq!(
            source_root("app/src/test/java/com/acme/CartTest.java", Some("com.acme")),
            "app/src/test/java/"
        );
        assert_eq!(source_root("CartTest.java", None), "");
        assert_eq!(source_root("test/CartTest.java", None), "test/");
    }
}
