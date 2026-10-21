//! File and code templates, like PHPStorm's Settings | Editor | File and Code Templates:
//!
//! - [`velocity`]: the template language (a Velocity subset: `${NAME}`, `#if`, `#parse`, …).
//! - [`builtin`]: the templates Forge ships (PHP Class/Interface/Trait/Enum, PHPUnit Test,
//!   HTML, CSS, JavaScript, TypeScript, Vue, React, Lua, Laravel and Symfony classes, the PHP
//!   File Header include and the PHP code templates).
//! - [`psr4`]: the namespace of a folder and the folder of a namespace from composer.json.
//! - [`catalog`] merges the built-ins with the user's templates (setting
//!   `fileTemplates.custom`), [`new_file`] renders a New ▸ … file and [`render_code`] a code
//!   template for the code generators.
//!
//! Pure logic: the desktop app and the browser-mode WebAssembly build share it.

pub mod builtin;
pub mod psr4;
pub mod velocity;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use builtin::names;
pub use velocity::{render, validate, TemplateError, Vars};

/// Setting that holds the user's templates and changed built-ins.
pub const CUSTOM_KEY: &str = "fileTemplates.custom";

/// The tabs of the settings page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TemplateKind {
    #[default]
    File,
    Include,
    Code,
}

/// A user template, or the new text of a built-in one with the same kind and name (an entry
/// of the `fileTemplates.custom` setting).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTemplate {
    pub kind: TemplateKind,
    pub name: String,
    #[serde(default)]
    pub extension: String,
    #[serde(default)]
    pub text: String,
}

/// Parse the `fileTemplates.custom` setting value, skipping malformed entries.
pub fn parse_custom(value: &serde_json::Value) -> Vec<CustomTemplate> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| serde_json::from_value::<CustomTemplate>(v.clone()).ok())
        .filter(|c| !c.name.trim().is_empty())
        .collect()
}

/// A template as the settings page and the New menus show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    pub kind: TemplateKind,
    pub name: String,
    pub extension: String,
    pub text: String,
    /// Shipped with Forge (it can be reset but not removed).
    pub builtin: bool,
    /// A built-in whose text or extension the user changed.
    pub modified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_extension: Option<String>,
    /// Offered only when the project uses this language or framework.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// Menu group (`php`, `framework`, `web`, `lua`, `user`).
    pub group: String,
    /// A PHP class-like file: New opens the New PHP Class dialog (Namespace, Directory, …).
    pub php_class: bool,
    /// Variables the template uses beyond the predefined ones: the New dialog asks for them.
    pub variables: Vec<String>,
    /// Every variable the template uses (the New PHP Class dialog shows Extends / Implements
    /// only for templates that use them).
    pub references: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The template does not parse (a message with its line).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A predefined variable and what it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableInfo {
    pub name: String,
    pub description: String,
}

/// Variables every file template can use.
pub const PREDEFINED: &[(&str, &str)] = &[
    (
        "NAME",
        "name of the new file without extension (the class name for PHP classes)",
    ),
    ("FILE_NAME", "name of the new file with extension"),
    (
        "NAMESPACE",
        "namespace of the new PHP class (from composer.json PSR-4 rules)",
    ),
    (
        "PACKAGE_NAME",
        "namespace of the new PHP class, else the folder path with dots",
    ),
    (
        "DIR_PATH",
        "folder of the new file, relative to the project root",
    ),
    ("PROJECT_NAME", "name of the current project"),
    ("USER", "current user's login name"),
    ("DATE", "current system date (2026-10-22)"),
    ("TIME", "current system time (14:05)"),
    ("YEAR", "current year"),
    ("MONTH", "current month (01-12)"),
    (
        "MONTH_NAME_SHORT",
        "first 3 letters of the month name (Oct)",
    ),
    ("MONTH_NAME_FULL", "full name of the month (October)"),
    ("DAY", "current day of the month (01-31)"),
    ("DAY_NAME_SHORT", "first 3 letters of the day name (Thu)"),
    ("DAY_NAME_FULL", "full name of the day (Thursday)"),
    ("HOUR", "current hour (00-23)"),
    ("MINUTE", "current minute"),
    ("SECOND", "current second"),
    (
        "EXTENDS",
        "parent class of a new PHP class or interface (New PHP Class dialog)",
    ),
    (
        "IMPLEMENTS",
        "interfaces of a new PHP class or enum (New PHP Class dialog)",
    ),
];

pub fn predefined_variables() -> Vec<VariableInfo> {
    PREDEFINED
        .iter()
        .map(|(n, d)| VariableInfo {
            name: n.to_string(),
            description: d.to_string(),
        })
        .collect()
}

fn info(
    kind: TemplateKind,
    name: &str,
    extension: &str,
    text: &str,
    shipped: Option<&builtin::Builtin>,
) -> TemplateInfo {
    let refs = velocity::referenced_variables(text);
    let php_class = kind == TemplateKind::File
        && extension.eq_ignore_ascii_case("php")
        && refs.iter().any(|v| v == "NAMESPACE");
    let variables = if kind == TemplateKind::File {
        refs.iter()
            .filter(|v| !PREDEFINED.iter().any(|(p, _)| p == *v))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    TemplateInfo {
        kind,
        name: name.to_string(),
        extension: extension.to_string(),
        text: text.to_string(),
        builtin: shipped.is_some(),
        modified: shipped.is_some_and(|b| b.text != text || b.extension != extension),
        default_text: shipped.map(|b| b.text.to_string()),
        default_extension: shipped.map(|b| b.extension.to_string()),
        when: shipped.and_then(|b| b.when).map(str::to_string),
        group: shipped.map_or("user", |b| b.group).to_string(),
        php_class,
        variables,
        references: refs,
        description: shipped.map_or("", |b| b.description).to_string(),
        error: velocity::validate(text).err().map(|e| e.to_string()),
    }
}

/// The built-in templates with the user's changes, followed by the user's own templates.
pub fn catalog(custom: &[CustomTemplate]) -> Vec<TemplateInfo> {
    let find =
        |kind: TemplateKind, name: &str| custom.iter().find(|c| c.kind == kind && c.name == name);
    let mut out: Vec<TemplateInfo> = builtin::BUILTINS
        .iter()
        .map(|b| match find(b.kind, b.name) {
            Some(c) => {
                let ext = if c.extension.trim().is_empty() {
                    b.extension
                } else {
                    c.extension.trim()
                };
                info(b.kind, b.name, ext, &c.text, Some(b))
            }
            None => info(b.kind, b.name, b.extension, b.text, Some(b)),
        })
        .collect();
    for c in custom {
        if builtin::builtin(c.kind, &c.name).is_none()
            && !out.iter().any(|t| t.kind == c.kind && t.name == c.name)
        {
            out.push(info(
                c.kind,
                c.name.trim(),
                c.extension.trim(),
                &c.text,
                None,
            ));
        }
    }
    out
}

/// Text of a template (the user's version when there is one).
pub fn template_text(custom: &[CustomTemplate], kind: TemplateKind, name: &str) -> Option<String> {
    custom
        .iter()
        .find(|c| c.kind == kind && c.name == name)
        .map(|c| c.text.clone())
        .or_else(|| builtin::builtin(kind, name).map(|b| b.text.to_string()))
}

/// Looks up includes for `#parse("PHP File Header.php")` (the name with or without extension).
fn include_lookup(custom: &[CustomTemplate]) -> impl Fn(&str) -> Option<String> + '_ {
    let includes: Vec<TemplateInfo> = catalog(custom)
        .into_iter()
        .filter(|t| t.kind == TemplateKind::Include)
        .collect();
    move |wanted: &str| {
        includes
            .iter()
            .find(|t| {
                t.name == wanted
                    || (!t.extension.is_empty() && format!("{}.{}", t.name, t.extension) == wanted)
            })
            .map(|t| t.text.clone())
    }
}

// ---- New file

/// Local date and time, supplied by the caller (WebAssembly has no clock or time zone).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateParts {
    pub year: i32,
    /// 1-12.
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    /// 0 = Sunday.
    pub weekday: u32,
}

/// Values of the context variables.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TemplateContext {
    pub project_name: String,
    pub user: String,
    pub now: Option<DateParts>,
}

/// File | New ▸ <template>.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewFileRequest {
    /// Name of a file template.
    pub template: String,
    /// Project-relative folder.
    pub dir: String,
    /// `${NAME}`: the class name, or a file name (with or without the extension, may start
    /// with sub folders: `components/Button`).
    pub name: String,
    /// The file name when it differs from `${NAME}.<extension>` (New PHP Class dialog).
    pub file_name: Option<String>,
    pub namespace: Option<String>,
    /// Parent class (interfaces: parent interfaces, comma separated).
    pub extends: Option<String>,
    /// Comma separated interfaces.
    pub implements: Option<String>,
    /// Values of the template's own variables.
    pub variables: Vars,
    pub context: TemplateContext,
}

/// The rendered file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewFile {
    /// Project-relative path.
    pub path: String,
    pub text: String,
}

fn fail<T>(message: impl Into<String>) -> Result<T, TemplateError> {
    Err(TemplateError {
        message: message.into(),
        line: 0,
    })
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

fn date_vars(vars: &mut Vars, d: &DateParts) {
    let month = MONTHS[(d.month.clamp(1, 12) - 1) as usize];
    let day = DAYS[(d.weekday % 7) as usize];
    let pairs = [
        ("YEAR", d.year.to_string()),
        ("MONTH", format!("{:02}", d.month)),
        ("MONTH_NAME_SHORT", month[..3].to_string()),
        ("MONTH_NAME_FULL", month.to_string()),
        ("DAY", format!("{:02}", d.day)),
        ("DAY_NAME_SHORT", day[..3].to_string()),
        ("DAY_NAME_FULL", day.to_string()),
        ("HOUR", format!("{:02}", d.hour)),
        ("MINUTE", format!("{:02}", d.minute)),
        ("SECOND", format!("{:02}", d.second)),
        ("DATE", format!("{}-{:02}-{:02}", d.year, d.month, d.day)),
        ("TIME", format!("{:02}:{:02}", d.hour, d.minute)),
    ];
    for (k, v) in pairs {
        vars.insert(k.to_string(), v);
    }
}

/// A PHP name like `Model` or `\Illuminate\Database\Eloquent\Model`.
pub fn is_class_reference(name: &str) -> bool {
    let n = name.strip_prefix('\\').unwrap_or(name);
    !n.is_empty() && n.split('\\').all(is_php_identifier)
}

/// A PHP identifier (class, namespace segment).
pub fn is_php_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Check a namespace typed into the New PHP Class dialog (empty is fine).
pub fn validate_namespace(namespace: &str) -> Option<String> {
    let ns = namespace.trim().trim_matches('\\');
    if ns.is_empty() || ns.split('\\').all(is_php_identifier) {
        None
    } else {
        Some(format!("'{}' is not a valid namespace", namespace.trim()))
    }
}

fn split_list(list: Option<&str>) -> Result<Vec<String>, TemplateError> {
    let mut out = Vec::new();
    for n in list
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        if !is_class_reference(n) {
            return fail(format!("'{n}' is not a valid class name"));
        }
        out.push(n.trim_start_matches('\\').to_string());
    }
    Ok(out)
}

/// Short names for `names` in `namespace` and the `use` imports they need; `qualified` keeps
/// the full names (the file declares no namespace).
fn class_refs(
    namespace: &str,
    own: &str,
    names: &[String],
    qualified: bool,
    imports: &mut Vec<String>,
) -> String {
    let mut out = Vec::new();
    for n in names {
        let Some((ns, short)) = n.rsplit_once('\\') else {
            out.push(n.clone());
            continue;
        };
        if qualified {
            out.push(n.clone());
        } else if ns.eq_ignore_ascii_case(namespace) {
            out.push(short.to_string());
        } else {
            let clash = short.eq_ignore_ascii_case(own)
                || imports.iter().any(|i| {
                    i != n
                        && i.rsplit('\\')
                            .next()
                            .is_some_and(|s| s.eq_ignore_ascii_case(short))
                });
            if clash {
                out.push(format!("\\{n}"));
            } else {
                if !imports.contains(n) {
                    imports.push(n.clone());
                }
                out.push(short.to_string());
            }
        }
    }
    out.join(", ")
}

/// Add `use` lines after the namespace statement (after the file's own imports, if any);
/// `None` when the text declares no namespace.
fn add_imports(text: &str, imports: &[String]) -> Option<String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let ns = lines.iter().position(|l| {
        let t = l.trim();
        t.starts_with("namespace ") && t.ends_with(';')
    })?;
    let missing: Vec<&String> = imports
        .iter()
        .filter(|i| !lines.iter().any(|l| l.trim() == format!("use {i};")))
        .collect();
    if missing.is_empty() {
        return Some(text.to_string());
    }
    let mut at = ns + 1;
    let mut last_use = None;
    while at < lines.len() {
        let t = lines[at].trim();
        if t.starts_with("use ") {
            last_use = Some(at);
        } else if !t.is_empty() {
            break;
        }
        at += 1;
    }
    let block: String = missing.iter().map(|i| format!("use {i};\n")).collect();
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        out.push_str(l);
        if !l.ends_with('\n') && (Some(i) == last_use || (last_use.is_none() && i == ns)) {
            out.push('\n');
        }
        match last_use {
            Some(u) if u == i => out.push_str(&block),
            None if i == ns => {
                out.push('\n');
                out.push_str(&block);
            }
            _ => {}
        }
    }
    Some(out)
}

fn join_path(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{}/{name}", dir.trim_end_matches('/'))
    }
}

fn check_segment(seg: &str) -> Result<(), TemplateError> {
    if seg.is_empty() || seg == "." || seg == ".." || seg.contains(['\\', '\0', ':']) {
        return fail(format!("'{seg}' is not a valid file name"));
    }
    Ok(())
}

/// Render File | New ▸ <template> into the new file's path and text.
pub fn new_file(req: &NewFileRequest, custom: &[CustomTemplate]) -> Result<NewFile, TemplateError> {
    let templates = catalog(custom);
    let Some(template) = templates
        .iter()
        .find(|t| t.kind == TemplateKind::File && t.name == req.template)
    else {
        return fail(format!("Unknown file template '{}'", req.template));
    };
    let ext = template.extension.trim_start_matches('.');
    let typed = req.name.trim().trim_matches('/');
    if typed.is_empty() {
        return fail("Enter a name");
    }
    let mut dir = req.dir.trim_matches('/').to_string();
    let (subdirs, base) = typed.rsplit_once('/').unwrap_or(("", typed));
    for seg in subdirs.split('/').filter(|s| !s.is_empty()) {
        check_segment(seg)?;
        dir = join_path(&dir, seg);
    }
    check_segment(base)?;
    let name = match base.len().checked_sub(ext.len() + 1) {
        Some(n)
            if !ext.is_empty() && base[n..].eq_ignore_ascii_case(&format!(".{ext}")) && n > 0 =>
        {
            &base[..n]
        }
        _ => base,
    };
    if template.php_class && !is_php_identifier(name) {
        return fail(format!("'{name}' is not a valid class name"));
    }
    let file_name = match req
        .file_name
        .as_deref()
        .map(str::trim)
        .filter(|f| !f.is_empty())
    {
        Some(f) => {
            check_segment(f)?;
            f.to_string()
        }
        None if ext.is_empty() => name.to_string(),
        None => format!("{name}.{ext}"),
    };
    let namespace = req
        .namespace
        .as_deref()
        .unwrap_or("")
        .trim()
        .trim_matches('\\')
        .to_string();
    if let Some(problem) = validate_namespace(&namespace) {
        return fail(problem);
    }
    let extends = split_list(req.extends.as_deref())?;
    let implements = split_list(req.implements.as_deref())?;

    let mut vars = Vars::new();
    for (k, v) in &req.variables {
        if !PREDEFINED.iter().any(|(p, _)| p == k) {
            vars.insert(k.clone(), v.clone());
        }
    }
    vars.insert("NAME".into(), name.to_string());
    vars.insert("FILE_NAME".into(), file_name.clone());
    vars.insert("NAMESPACE".into(), namespace.clone());
    vars.insert(
        "PACKAGE_NAME".into(),
        if namespace.is_empty() {
            dir.replace('/', ".")
        } else {
            namespace.clone()
        },
    );
    vars.insert("DIR_PATH".into(), dir.clone());
    vars.insert("PROJECT_NAME".into(), req.context.project_name.clone());
    vars.insert("USER".into(), req.context.user.clone());
    if let Some(now) = &req.context.now {
        date_vars(&mut vars, now);
    }
    let includes = include_lookup(custom);
    let render_with = |qualified: bool, imports: &mut Vec<String>| {
        let mut v = vars.clone();
        v.insert(
            "EXTENDS".into(),
            class_refs(&namespace, name, &extends, qualified, imports),
        );
        v.insert(
            "IMPLEMENTS".into(),
            class_refs(&namespace, name, &implements, qualified, imports),
        );
        velocity::render(&template.text, &v, &includes)
    };
    let mut imports = Vec::new();
    let mut text = render_with(false, &mut imports)?;
    if !imports.is_empty() {
        text = match add_imports(&text, &imports) {
            Some(t) => t,
            None => render_with(true, &mut Vec::new())?,
        };
    }
    Ok(NewFile {
        path: join_path(&dir, &file_name),
        text,
    })
}

// ---- Code templates

/// The user's versions of the code templates by name (for the code generators).
pub fn code_overrides(custom: &[CustomTemplate]) -> BTreeMap<String, String> {
    custom
        .iter()
        .filter(|c| c.kind == TemplateKind::Code)
        .map(|c| (c.name.clone(), c.text.clone()))
        .collect()
}

/// Render the code template `name` (the user's version from `overrides` when there is one).
pub fn render_code(
    overrides: &BTreeMap<String, String>,
    name: &str,
    vars: &Vars,
) -> Result<String, TemplateError> {
    let text = overrides
        .get(name)
        .cloned()
        .or_else(|| builtin::builtin(TemplateKind::Code, name).map(|b| b.text.to_string()))
        .unwrap_or_default();
    velocity::render(&text, vars, &|_| None).map_err(|e| TemplateError {
        message: format!("{name}: {}", e.message),
        line: e.line,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(template: &str, dir: &str, name: &str) -> NewFileRequest {
        NewFileRequest {
            template: template.into(),
            dir: dir.into(),
            name: name.into(),
            context: TemplateContext {
                project_name: "shop".into(),
                user: "ada".into(),
                now: Some(DateParts {
                    year: 2026,
                    month: 10,
                    day: 22,
                    hour: 9,
                    minute: 5,
                    second: 7,
                    weekday: 4,
                }),
            },
            ..Default::default()
        }
    }

    #[test]
    fn php_class_with_namespace() {
        let mut r = req("PHP Class", "app/Models", "Order");
        r.namespace = Some("App\\Models".into());
        let f = new_file(&r, &[]).unwrap();
        assert_eq!(f.path, "app/Models/Order.php");
        assert_eq!(
            f.text,
            "<?php\n\nnamespace App\\Models;\n\nclass Order\n{\n\n}\n"
        );
        // Without a namespace.
        let f = new_file(&req("PHP Class", "", "Order.php"), &[]).unwrap();
        assert_eq!(f.path, "Order.php");
        assert_eq!(f.text, "<?php\n\nclass Order\n{\n\n}\n");
    }

    #[test]
    fn extends_and_implements_are_imported() {
        let mut r = req("PHP Class", "app/Models", "Order");
        r.namespace = Some("App\\Models".into());
        r.extends = Some("\\Illuminate\\Database\\Eloquent\\Model".into());
        r.implements = Some(
            "App\\Models\\Contracts\\HasTotal, \\JsonSerializable, App\\Models\\Billable".into(),
        );
        let f = new_file(&r, &[]).unwrap();
        assert_eq!(
            f.text,
            "<?php\n\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\nuse App\\Models\\Contracts\\HasTotal;\n\nclass Order extends Model implements HasTotal, JsonSerializable, Billable\n{\n\n}\n"
        );
        // Without a namespace the names stay qualified; a clash with the class name too.
        let mut r = req("PHP Class", "", "Order");
        r.extends = Some("Illuminate\\Database\\Eloquent\\Model".into());
        let f = new_file(&r, &[]).unwrap();
        assert!(
            f.text
                .contains("class Order extends Illuminate\\Database\\Eloquent\\Model\n"),
            "{}",
            f.text
        );
        r.name = "Model".into();
        r.namespace = Some("App".into());
        let f = new_file(&r, &[]).unwrap();
        assert!(
            f.text
                .contains("class Model extends \\Illuminate\\Database\\Eloquent\\Model\n"),
            "{}",
            f.text
        );
        assert!(!f.text.contains("use "));
        r.extends = Some("Not Valid".into());
        assert!(new_file(&r, &[]).is_err());
    }

    #[test]
    fn imports_go_after_the_templates_own_imports() {
        let mut r = req(
            "Laravel Controller",
            "app/Http/Controllers",
            "OrderController",
        );
        r.namespace = Some("App\\Http\\Controllers".into());
        let f = new_file(&r, &[]).unwrap();
        assert_eq!(
            f.text,
            "<?php\n\nnamespace App\\Http\\Controllers;\n\nuse Illuminate\\Http\\Request;\n\nclass OrderController extends Controller\n{\n    //\n}\n"
        );
        let custom = [CustomTemplate {
            kind: TemplateKind::File,
            name: "Service".into(),
            extension: "php".into(),
            text: "<?php\nnamespace ${NAMESPACE};\nuse A\\B;\nclass ${NAME} extends ${EXTENDS} {}"
                .into(),
        }];
        let mut r = req("Service", "src", "Mailer");
        r.namespace = Some("Acme".into());
        r.extends = Some("Base\\Service".into());
        assert_eq!(
            new_file(&r, &custom).unwrap().text,
            "<?php\nnamespace Acme;\nuse A\\B;\nuse Base\\Service;\nclass Mailer extends Service {}"
        );
    }

    #[test]
    fn interfaces_traits_enums_and_tests() {
        let mut r = req("PHP Interface", "src", "Repository");
        r.namespace = Some("Acme".into());
        r.extends = Some("Countable, IteratorAggregate".into());
        assert!(new_file(&r, &[])
            .unwrap()
            .text
            .contains("interface Repository extends Countable, IteratorAggregate\n{"));
        let f = new_file(&req("PHP Trait", "", "Loggable"), &[]).unwrap();
        assert!(f.text.contains("trait Loggable\n{"));
        let mut r = req("PHP Enum", "", "Status");
        r.variables.insert("BACKED_TYPE".into(), "string".into());
        assert!(new_file(&r, &[])
            .unwrap()
            .text
            .contains("enum Status: string\n{"));
        let mut r = req("PHPUnit Test", "tests/Unit", "OrderTest");
        r.namespace = Some("Tests\\Unit".into());
        let f = new_file(&r, &[]).unwrap();
        assert!(f.text.contains("namespace Tests\\Unit;\n\nuse PHPUnit\\Framework\\TestCase;\n\nclass OrderTest extends TestCase\n"));
        assert!(new_file(&req("PHP Class", "", "1Bad"), &[])
            .unwrap_err()
            .message
            .contains("valid class name"));
    }

    #[test]
    fn symfony_controller_routes() {
        let mut r = req("Symfony Controller", "src/Controller", "OrderController");
        r.namespace = Some("App\\Controller".into());
        let f = new_file(&r, &[]).unwrap();
        assert!(
            f.text.contains("#[Route('/order', name: 'app_order')]"),
            "{}",
            f.text
        );
        assert!(f.text.contains("return $this->render('order/index.html.twig', [\n            'controller_name' => 'OrderController',"));
        assert!(f
            .text
            .contains("namespace App\\Controller;\n\nuse Symfony\\Bundle"));
    }

    #[test]
    fn web_and_lua_files() {
        let f = new_file(&req("HTML File", "public", "about"), &[]).unwrap();
        assert_eq!(f.path, "public/about.html");
        assert!(f.text.contains("<title>about</title>"));
        let f = new_file(
            &req(
                "Vue Single File Component",
                "src",
                "components/OrderList.vue",
            ),
            &[],
        )
        .unwrap();
        assert_eq!(f.path, "src/components/OrderList.vue");
        assert!(f.text.starts_with("<script setup lang=\"ts\">"));
        let f = new_file(&req("React TypeScript Component", "src", "Cart"), &[]).unwrap();
        assert_eq!(f.path, "src/Cart.tsx");
        assert!(f
            .text
            .contains("export default function Cart(props: CartProps) {"));
        let f = new_file(&req("Lua Module", "", "player"), &[]).unwrap();
        assert_eq!(f.text, "local player = {}\n\nreturn player\n");
        assert_eq!(
            new_file(&req("CSS File", "", "app.css"), &[]).unwrap().path,
            "app.css"
        );
        // A name that is only the extension stays the name.
        assert_eq!(
            new_file(&req("CSS File", "", ".css"), &[]).unwrap().path,
            ".css.css"
        );
        assert!(new_file(&req("CSS File", "", "../x"), &[]).is_err());
        assert!(new_file(&req("CSS File", "", "  "), &[]).is_err());
        assert!(new_file(&req("Nope", "", "x"), &[]).is_err());
    }

    #[test]
    fn context_variables_and_header_include() {
        let custom = [
            CustomTemplate {
                kind: TemplateKind::Include,
                name: "PHP File Header".into(),
                extension: "php".into(),
                text: "/**\n * ${PROJECT_NAME}: ${FILE_NAME} by ${USER}, ${DATE} ${TIME} (${DAY_NAME_SHORT}, ${MONTH_NAME_FULL} ${YEAR})\n */".into(),
            },
            CustomTemplate {
                kind: TemplateKind::File,
                name: "Note".into(),
                extension: "md".into(),
                text: "# ${TITLE}\n${PACKAGE_NAME} ${DIR_PATH} ${SECOND}".into(),
            },
        ];
        let f = new_file(&req("PHP File", "", "index"), &custom).unwrap();
        assert_eq!(
            f.text,
            "<?php\n/**\n * shop: index.php by ada, 2026-10-22 09:05 (Thu, October 2026)\n */\n\n"
        );
        let mut r = req("Note", "docs/guide", "intro");
        r.variables.insert("TITLE".into(), "Intro".into());
        r.variables.insert("NAME".into(), "ignored".into());
        let f = new_file(&r, &custom).unwrap();
        assert_eq!(f.path, "docs/guide/intro.md");
        assert_eq!(f.text, "# Intro\ndocs.guide docs/guide 07");
    }

    #[test]
    fn catalog_merges_user_changes() {
        let custom = [
            CustomTemplate {
                kind: TemplateKind::File,
                name: "PHP Class".into(),
                extension: String::new(),
                text: "<?php\nnamespace ${NAMESPACE};\nfinal class ${NAME} {}\n".into(),
            },
            CustomTemplate {
                kind: TemplateKind::File,
                name: "Action".into(),
                extension: "php".into(),
                text: "<?php\nnamespace ${NAMESPACE};\n#if ($A)\n".into(),
            },
        ];
        let all = catalog(&custom);
        let class = all.iter().find(|t| t.name == "PHP Class").unwrap();
        assert!(class.builtin && class.modified && class.php_class);
        assert_eq!(class.extension, "php");
        assert!(class
            .default_text
            .as_deref()
            .unwrap()
            .contains("class ${NAME}"));
        let action = all.last().unwrap();
        assert_eq!(
            (action.name.as_str(), action.group.as_str(), action.builtin),
            ("Action", "user", false)
        );
        assert!(action.php_class);
        assert_eq!(action.variables, vec!["A"]);
        assert_eq!(
            action.error.as_deref(),
            Some("Line 3: Missing #end for #if")
        );
        let enum_ = all.iter().find(|t| t.name == "PHP Enum").unwrap();
        assert_eq!(enum_.variables, vec!["BACKED_TYPE"]);
        assert!(
            !all.iter()
                .find(|t| t.name == "HTML File")
                .unwrap()
                .php_class
        );
        assert_eq!(
            all.iter()
                .find(|t| t.name == "Laravel Model")
                .unwrap()
                .when
                .as_deref(),
            Some("laravel")
        );
        assert!(
            catalog(&[])
                .iter()
                .all(|t| t.error.is_none() && !t.modified),
            "built-ins parse"
        );
    }

    #[test]
    fn parses_the_setting() {
        let v = serde_json::json!([
            { "kind": "code", "name": "PHP Implemented Method Body", "text": "throw new \\LogicException();" },
            { "kind": "file", "name": "" },
            { "kind": "bogus", "name": "x" },
            "nope"
        ]);
        let custom = parse_custom(&v);
        assert_eq!(custom.len(), 1);
        let overrides = code_overrides(&custom);
        let mut vars = Vars::new();
        vars.insert("NAME".into(), "run".into());
        assert_eq!(
            render_code(&overrides, names::IMPLEMENTED_METHOD_BODY, &vars).unwrap(),
            "throw new \\LogicException();"
        );
        assert_eq!(
            render_code(&BTreeMap::new(), names::IMPLEMENTED_METHOD_BODY, &vars).unwrap(),
            "// TODO: Implement run() method."
        );
        let bad: BTreeMap<String, String> =
            [(names::FIELD_DOC_COMMENT.to_string(), "#end".to_string())].into();
        assert!(render_code(&bad, names::FIELD_DOC_COMMENT, &vars)
            .unwrap_err()
            .message
            .starts_with("PHP Field Doc Comment:"));
    }

    #[test]
    fn namespaces_are_checked() {
        assert_eq!(validate_namespace("App\\Models"), None);
        assert_eq!(validate_namespace("\\App\\"), None);
        assert_eq!(validate_namespace(""), None);
        assert!(validate_namespace("App\\1x").is_some());
        assert!(validate_namespace("App Models").is_some());
        assert!(
            is_class_reference("\\Foo\\Bar")
                && !is_class_reference("Foo\\")
                && !is_class_reference("")
        );
    }
}
