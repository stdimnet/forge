//! Symfony forms: option names of form types in `->add('field', TextType::class, [...])`
//! (a hand-written table of the core types plus the `configureOptions()` of the
//! application's types), field names from the form's `data_class`, and navigation from
//! `createForm(XType::class)` / `->add(..., XType::class)` to the type.

use phpforge_lang_php::parser::index_source;
use phpforge_lang_php::FileIndex;
use serde::{Deserialize, Serialize};

use crate::complete::{CompletionItem, CompletionKind, CompletionList, FileKind};
use crate::phparray::{array_after, call_args, PhpValue};
use crate::strings::{call_context, string_at, StringAt};
use crate::{Location, PhpFile, Snapshot};

const CORE_NS: &str = "Symfony\\Component\\Form\\Extension\\Core\\Type\\";

/// Options every type inherits from `FormType`.
const FORM_OPTIONS: &[&str] = &[
    "action",
    "allow_extra_fields",
    "attr",
    "auto_initialize",
    "block_name",
    "block_prefix",
    "by_reference",
    "compound",
    "constraints",
    "csrf_field_name",
    "csrf_message",
    "csrf_protection",
    "csrf_token_id",
    "data",
    "data_class",
    "disabled",
    "empty_data",
    "error_bubbling",
    "error_mapping",
    "extra_fields_message",
    "form_attr",
    "getter",
    "help",
    "help_attr",
    "help_html",
    "help_translation_parameters",
    "inherit_data",
    "invalid_message",
    "invalid_message_parameters",
    "is_empty_callback",
    "label",
    "label_attr",
    "label_format",
    "label_html",
    "label_translation_parameters",
    "mapped",
    "method",
    "priority",
    "property_path",
    "required",
    "row_attr",
    "setter",
    "translation_domain",
    "trim",
    "validation_groups",
];

const CHOICE_OPTIONS: &[&str] = &[
    "choices",
    "choice_attr",
    "choice_filter",
    "choice_label",
    "choice_lazy",
    "choice_loader",
    "choice_name",
    "choice_translation_domain",
    "choice_translation_parameters",
    "choice_value",
    "duplicate_preferred_choices",
    "expanded",
    "group_by",
    "multiple",
    "placeholder",
    "placeholder_attr",
    "preferred_choices",
    "separator",
    "separator_html",
];

const DATE_OPTIONS: &[&str] = &[
    "days",
    "months",
    "years",
    "format",
    "html5",
    "input",
    "input_format",
    "model_timezone",
    "view_timezone",
    "widget",
    "placeholder",
    "calendar",
];

const TIME_OPTIONS: &[&str] = &[
    "hours",
    "minutes",
    "seconds",
    "with_minutes",
    "with_seconds",
    "html5",
    "input",
    "input_format",
    "model_timezone",
    "view_timezone",
    "reference_date",
    "widget",
    "placeholder",
];

const BUTTON_OPTIONS: &[&str] = &[
    "attr",
    "block_name",
    "block_prefix",
    "disabled",
    "label",
    "label_format",
    "label_html",
    "label_translation_parameters",
    "priority",
    "row_attr",
    "translation_domain",
];

/// Core form types: (short name, parent, own options).
const CORE_TYPES: &[(&str, Option<&str>, &[&str])] = &[
    ("FormType", None, FORM_OPTIONS),
    ("TextType", Some("FormType"), &[]),
    ("TextareaType", Some("TextType"), &[]),
    ("EmailType", Some("TextType"), &[]),
    ("SearchType", Some("TextType"), &[]),
    ("TelType", Some("TextType"), &[]),
    ("UrlType", Some("TextType"), &["default_protocol"]),
    (
        "PasswordType",
        Some("TextType"),
        &["always_empty", "hash_property_path", "toggle"],
    ),
    ("ColorType", Some("TextType"), &["html5"]),
    ("RangeType", Some("TextType"), &[]),
    ("HiddenType", Some("FormType"), &[]),
    (
        "IntegerType",
        Some("FormType"),
        &["grouping", "rounding_mode"],
    ),
    (
        "NumberType",
        Some("FormType"),
        &["grouping", "html5", "input", "rounding_mode", "scale"],
    ),
    (
        "MoneyType",
        Some("FormType"),
        &[
            "currency",
            "divisor",
            "grouping",
            "html5",
            "input",
            "rounding_mode",
            "scale",
        ],
    ),
    (
        "PercentType",
        Some("FormType"),
        &["html5", "rounding_mode", "scale", "symbol", "type"],
    ),
    ("ChoiceType", Some("FormType"), CHOICE_OPTIONS),
    ("EnumType", Some("ChoiceType"), &["class"]),
    (
        "CountryType",
        Some("ChoiceType"),
        &["alpha3", "choice_translation_locale"],
    ),
    (
        "LanguageType",
        Some("ChoiceType"),
        &[
            "alpha3",
            "choice_self_translation",
            "choice_translation_locale",
        ],
    ),
    (
        "LocaleType",
        Some("ChoiceType"),
        &["choice_translation_locale"],
    ),
    (
        "CurrencyType",
        Some("ChoiceType"),
        &["choice_translation_locale"],
    ),
    (
        "TimezoneType",
        Some("ChoiceType"),
        &["input", "intl", "choice_translation_locale"],
    ),
    ("DateType", Some("FormType"), DATE_OPTIONS),
    ("BirthdayType", Some("DateType"), &[]),
    ("TimeType", Some("FormType"), TIME_OPTIONS),
    (
        "DateTimeType",
        Some("FormType"),
        &[
            "date_format",
            "date_label",
            "date_widget",
            "time_label",
            "time_widget",
            "with_minutes",
            "with_seconds",
            "hours",
            "minutes",
            "seconds",
            "days",
            "months",
            "years",
            "format",
            "html5",
            "input",
            "input_format",
            "model_timezone",
            "view_timezone",
            "widget",
            "placeholder",
        ],
    ),
    (
        "WeekType",
        Some("FormType"),
        &["html5", "input", "widget", "weeks", "years", "placeholder"],
    ),
    (
        "DateIntervalType",
        Some("FormType"),
        &[
            "days",
            "hours",
            "minutes",
            "months",
            "seconds",
            "weeks",
            "years",
            "input",
            "labels",
            "placeholder",
            "widget",
            "with_days",
            "with_hours",
            "with_invert",
            "with_minutes",
            "with_months",
            "with_seconds",
            "with_weeks",
            "with_years",
        ],
    ),
    ("CheckboxType", Some("FormType"), &["false_values", "value"]),
    ("RadioType", Some("CheckboxType"), &[]),
    ("FileType", Some("FormType"), &["multiple"]),
    (
        "CollectionType",
        Some("FormType"),
        &[
            "allow_add",
            "allow_delete",
            "delete_empty",
            "entry_options",
            "entry_type",
            "keep_as_list",
            "prototype",
            "prototype_data",
            "prototype_name",
            "prototype_options",
        ],
    ),
    (
        "RepeatedType",
        Some("FormType"),
        &[
            "first_name",
            "first_options",
            "options",
            "second_name",
            "second_options",
            "type",
        ],
    ),
    ("ButtonType", None, BUTTON_OPTIONS),
    ("SubmitType", Some("ButtonType"), &["validate"]),
    ("ResetType", Some("ButtonType"), &[]),
    // Doctrine bridge
    (
        "EntityType",
        Some("ChoiceType"),
        &["class", "em", "query_builder"],
    ),
];

/// An application form type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormType {
    pub class: String,
    /// `getParent()`, fully qualified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Options from `configureOptions()` (`setDefaults`, `setDefined`, `setRequired`, `define`).
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_class: Option<String>,
    pub location: Location,
}

fn core_entry(
    short: &str,
) -> Option<&'static (&'static str, Option<&'static str>, &'static [&'static str])> {
    CORE_TYPES.iter().find(|t| t.0 == short)
}

fn is_core(fqn: &str) -> Option<&'static str> {
    let short = fqn.rsplit('\\').next().unwrap_or(fqn);
    let ns_ok = fqn.starts_with(CORE_NS)
        || fqn.starts_with("Symfony\\Bridge\\Doctrine\\Form\\Type\\")
        || !fqn.contains('\\');
    if ns_ok {
        core_entry(short).map(|t| t.0)
    } else {
        None
    }
}

fn method_slice<'a>(
    src: &'a str,
    idx: &FileIndex,
    class_fqn: &str,
    method: &str,
) -> Option<(usize, usize, &'a str)> {
    let c = idx.classes.iter().find(|c| c.fqn == class_fqn)?;
    let m = c.method(method)?;
    let (s, e) = (m.range.start, m.range.end.min(src.len()));
    Some((s, e, &src[s..e]))
}

/// `Product::class` written in `idx` -> fully qualified name.
fn class_const(idx: &FileIndex, expr: &str, offset: usize) -> Option<String> {
    let name = expr.trim().strip_suffix("::class")?.trim();
    Some(
        idx.resolve_class(name, offset)
            .trim_start_matches('\\')
            .to_string(),
    )
}

/// Form types of the application (classes with `buildForm` or `configureOptions` that
/// extend a form type).
pub fn project_types(php: &[PhpFile]) -> Vec<FormType> {
    let mut out = Vec::new();
    for f in php {
        for c in &f.index.classes {
            let is_type = c.extends.iter().any(|e| e.ends_with("AbstractType"))
                || c.implements
                    .iter()
                    .any(|i| i.ends_with("FormTypeInterface"))
                || (c.name.ends_with("Type") && c.method("buildForm").is_some());
            if !is_type {
                continue;
            }
            let mut options = Vec::new();
            let mut data_class = None;
            if let Some((s, e, _)) = method_slice(&f.source, &f.index, &c.fqn, "configureOptions") {
                if let Some((at, defaults)) = array_after(&f.source, s, e, "setDefaults(") {
                    for item in defaults.items() {
                        let Some(k) = item.key.as_ref().and_then(|k| k.as_str()) else {
                            continue;
                        };
                        options.push(k.to_string());
                        if k == "data_class" {
                            data_class = match &item.value {
                                PhpValue::Expr(x) => class_const(&f.index, x, at),
                                PhpValue::Str(x) => Some(x.trim_start_matches('\\').to_string()),
                                _ => None,
                            };
                        }
                    }
                }
                for needle in ["setDefined(", "setRequired(", "define("] {
                    for args in call_args(&f.source, s, e, needle) {
                        for a in args {
                            match a {
                                PhpValue::Str(x) => options.push(x),
                                PhpValue::Array(items) => options.extend(
                                    items
                                        .iter()
                                        .filter_map(|i| i.value.as_str().map(String::from)),
                                ),
                                _ => {}
                            }
                        }
                    }
                }
            }
            let parent =
                method_slice(&f.source, &f.index, &c.fqn, "getParent").and_then(|(s, _, body)| {
                    let r = body.find("return")? + 6;
                    let expr = body[r..].split(';').next()?;
                    class_const(&f.index, expr, s + r)
                });
            options.sort();
            options.dedup();
            out.push(FormType {
                class: c.fqn.clone(),
                parent,
                options,
                data_class,
                location: Location::new(&f.path, c.name_range.line, c.name_range.column),
            });
        }
    }
    out
}

/// Options of a type with the type that defines each, own options first.
pub fn options_of(snap: &Snapshot, fqn: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut current = Some(fqn.trim_start_matches('\\').to_string());
    let mut guard = 0;
    while let Some(t) = current.take() {
        guard += 1;
        if guard > 10 {
            break;
        }
        let short = t.rsplit('\\').next().unwrap_or(&t).to_string();
        if let Some(p) = snap.form_types.iter().find(|p| p.class == t) {
            for o in &p.options {
                out.push((o.clone(), short.clone()));
            }
            current = Some(
                p.parent
                    .clone()
                    .unwrap_or_else(|| format!("{CORE_NS}FormType")),
            );
            continue;
        }
        if let Some(core) = is_core(&t).and_then(core_entry) {
            for o in core.2 {
                out.push((o.to_string(), core.0.to_string()));
            }
            current = core.1.map(|p| format!("{CORE_NS}{p}"));
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|(o, _)| seen.insert(o.clone()));
    out
}

/// What a string in a form builder is.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FormRef {
    /// Option name in the options array of `->add()`; the type's FQN.
    Option(String),
    /// Field name (first argument of `->add()`); the form's data class.
    Field(Option<String>),
}

/// Unmatched `[` before `offset` on the same statement.
fn open_bracket(text: &str, offset: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = offset;
    let floor = offset.saturating_sub(3000);
    while i > floor {
        i -= 1;
        match bytes[i] {
            b']' | b')' => depth += 1,
            b'(' if depth > 0 => depth -= 1,
            b'[' if depth > 0 => depth -= 1,
            b'[' => return Some(i),
            b'(' | b';' | b'{' => return None,
            b'\'' | b'"' => {
                let q = bytes[i];
                while i > floor {
                    i -= 1;
                    if bytes[i] == q && bytes[i - 1] != b'\\' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Top-level arguments of the call whose `(` is the last one before `bracket`.
fn args_before(text: &str, bracket: usize) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = bracket;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b')' | b']' => depth += 1,
            b'(' | b'[' if depth > 0 => depth -= 1,
            b'(' => break,
            _ => {}
        }
    }
    let inner = &text[i + 1..bracket];
    let mut args = Vec::new();
    let mut d = 0i32;
    let mut cur = String::new();
    for c in inner.chars() {
        match c {
            '(' | '[' => d += 1,
            ')' | ']' => d -= 1,
            ',' if d == 0 => {
                args.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    args.push(cur.trim().to_string());
    args
}

/// The data class of the form type the buffer defines.
fn buffer_data_class(
    snap: &Snapshot,
    path: &str,
    text: &str,
    idx: &FileIndex,
    offset: usize,
) -> Option<String> {
    let class = idx.class_at(offset)?;
    snap.form_types
        .iter()
        .find(|t| t.class == class.fqn && t.location.path == path)
        .and_then(|t| t.data_class.clone())
        .or_else(|| {
            // Unsaved buffers: read `data_class` from the text itself.
            let types = project_types(&[PhpFile {
                path: path.to_string(),
                source: text.to_string(),
                index: idx.clone(),
            }]);
            types.into_iter().find_map(|t| t.data_class)
        })
}

fn form_ref(
    snap: &Snapshot,
    path: &str,
    text: &str,
    s: &StringAt,
    idx: &FileIndex,
) -> Option<FormRef> {
    let before = text[..s.open].trim_end();
    if let Some(bracket) = open_bracket(text, s.open) {
        let key_position = before.ends_with('[') || before.ends_with(',');
        if !key_position {
            return None;
        }
        let call = call_context(text, bracket)?;
        if !(call.is_method && matches!(call.function.as_str(), "add" | "create") && call.arg == 2)
        {
            return None;
        }
        let args = args_before(text, bracket);
        let ty = args
            .get(1)
            .and_then(|a| class_const(idx, a, s.open))
            .unwrap_or_else(|| format!("{CORE_NS}TextType"));
        return Some(FormRef::Option(ty));
    }
    let call = call_context(text, s.open)?;
    if call.is_method && call.function == "add" && call.arg == 0 {
        let head = tail(text, s.open, 200);
        if head.contains("builder") || head.contains("->add(") {
            return Some(FormRef::Field(buffer_data_class(
                snap, path, text, idx, s.open,
            )));
        }
    }
    None
}

/// Entity properties for field names: (name, type).
fn fields_of(snap: &Snapshot, class: &str) -> Vec<(String, String, Location)> {
    for f in &snap.php {
        if let Some(c) = f
            .index
            .classes
            .iter()
            .find(|c| c.fqn.eq_ignore_ascii_case(class))
        {
            return c
                .properties()
                .filter(|p| !p.modifiers.is_static)
                .map(|p| {
                    (
                        p.name.trim_start_matches('$').to_string(),
                        p.effective_type().unwrap_or_default(),
                        Location::new(&f.path, p.name_range.line, p.name_range.column),
                    )
                })
                .collect();
        }
    }
    Vec::new()
}

/// Up to `n` bytes of `text` before `end`, on a char boundary.
fn tail(text: &str, end: usize, n: usize) -> &str {
    let end = end.min(text.len());
    let mut from = end.saturating_sub(n);
    while !text.is_char_boundary(from) {
        from += 1;
    }
    &text[from..end]
}

/// Cheap check before parsing the buffer: a form call shortly before `offset`.
fn near_form_call(text: &str, offset: usize) -> bool {
    let head = tail(text, offset, 3000);
    [
        "->add(",
        "->create(",
        "createForm(",
        "createFormBuilder(",
        "createNamed(",
    ]
    .iter()
    .any(|n| head.contains(n))
}

/// Completion of form options and field names.
pub fn complete(snap: &Snapshot, path: &str, text: &str, offset: usize) -> Option<CompletionList> {
    if FileKind::of(path) != FileKind::Php {
        return None;
    }
    let s = string_at(text, offset)?;
    if !near_form_call(text, s.open) {
        return None;
    }
    let idx = index_source(path, text);
    let r = form_ref(snap, path, text, &s, &idx)?;
    let p = s.prefix.to_ascii_lowercase();
    let items: Vec<CompletionItem> = match r {
        FormRef::Option(ty) => options_of(snap, &ty)
            .into_iter()
            .filter(|(o, _)| o.contains(&p))
            .map(|(o, from)| CompletionItem {
                label: o,
                kind: CompletionKind::FormOption,
                detail: from,
                info: String::new(),
                apply: None,
            })
            .collect(),
        FormRef::Field(class) => fields_of(snap, class.as_deref()?)
            .into_iter()
            .filter(|(n, _, _)| n.to_ascii_lowercase().contains(&p))
            .map(|(n, ty, _)| CompletionItem {
                label: n,
                kind: CompletionKind::FormField,
                detail: ty,
                info: class.clone().unwrap_or_default(),
                apply: None,
            })
            .collect(),
    };
    Some(CompletionList {
        from: s.start,
        items,
    })
}

/// Go to the form type of `XType::class` in `createForm()`/`add()`, or to the property of a
/// field name.
pub fn goto(snap: &Snapshot, path: &str, text: &str, offset: usize) -> Option<Vec<Location>> {
    if FileKind::of(path) != FileKind::Php || !near_form_call(text, offset.min(text.len())) {
        return None;
    }
    let idx = index_source(path, text);
    if let Some(s) = string_at(text, offset) {
        if let Some(FormRef::Field(Some(class))) = form_ref(snap, path, text, &s, &idx) {
            return fields_of(snap, &class)
                .into_iter()
                .find(|(n, _, _)| *n == s.value)
                .map(|(_, _, l)| vec![l]);
        }
        return None;
    }
    let offset = offset.min(text.len());
    let bytes = text.as_bytes();
    let is_name = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'\\';
    let mut start = offset;
    while start > 0 && is_name(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && is_name(bytes[end]) {
        end += 1;
    }
    let name = &text[start..end];
    if name.is_empty() || !text[end..].starts_with("::class") {
        return None;
    }
    let before = text[..start].trim_end();
    let in_form_call = [
        "createForm(",
        "createFormBuilder(",
        "createNamed(",
        "->add(",
        "->create(",
    ]
    .iter()
    .any(|n| {
        let head = tail(text, start, 200);
        head.rfind(n.trim_start_matches("->"))
            .is_some_and(|p| !head[p..].contains(';'))
    }) && (before.ends_with('(') || before.ends_with(','));
    if !in_form_call {
        return None;
    }
    let fqn = idx
        .resolve_class(name, start)
        .trim_start_matches('\\')
        .to_string();
    snap.class_location(&fqn).map(|l| vec![l])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fixture, scan, Config};

    pub const PRODUCT_TYPE: &str = r#"<?php

namespace App\Form;

use App\Entity\Product;
use Symfony\Component\Form\AbstractType;
use Symfony\Component\Form\Extension\Core\Type\ChoiceType;
use Symfony\Component\Form\Extension\Core\Type\MoneyType;
use Symfony\Component\Form\FormBuilderInterface;
use Symfony\Component\OptionsResolver\OptionsResolver;

class ProductType extends AbstractType
{
    public function buildForm(FormBuilderInterface $builder, array $options): void
    {
        $builder
            ->add('name')
            ->add('price', MoneyType::class, ['currency' => 'EUR'])
        ;
    }

    public function configureOptions(OptionsResolver $resolver): void
    {
        $resolver->setDefaults([
            'data_class' => Product::class,
            'with_stock' => false,
        ]);
        $resolver->setDefined(['discount']);
    }
}
"#;

    const PRODUCT: &str = "<?php\nnamespace App\\Entity;\nclass Product\n{\n    private ?int $id = null;\n    private string $name = '';\n    private int $price = 0;\n}\n";

    const STATUS_TYPE: &str = "<?php\nnamespace App\\Form;\nuse Symfony\\Component\\Form\\AbstractType;\nuse Symfony\\Component\\Form\\Extension\\Core\\Type\\ChoiceType;\nclass StatusType extends AbstractType\n{\n    public function getParent(): string\n    {\n        return ChoiceType::class;\n    }\n}\n";

    fn project() -> tempfile::TempDir {
        let dir = fixture::project();
        fixture::write(dir.path(), "src/Form/ProductType.php", PRODUCT_TYPE);
        fixture::write(dir.path(), "src/Form/StatusType.php", STATUS_TYPE);
        fixture::write(dir.path(), "src/Entity/Product.php", PRODUCT);
        dir
    }

    fn at(src: &str) -> (String, usize) {
        let pos = src.find('¦').unwrap();
        (src.replacen('¦', "", 1), pos)
    }

    #[test]
    fn project_types_and_options() {
        let dir = project();
        let snap = scan(dir.path(), &Config::default());
        let t = snap
            .form_types
            .iter()
            .find(|t| t.class == "App\\Form\\ProductType")
            .unwrap();
        assert_eq!(t.options, vec!["data_class", "discount", "with_stock"]);
        assert_eq!(t.data_class.as_deref(), Some("App\\Entity\\Product"));
        let s = snap
            .form_types
            .iter()
            .find(|t| t.class == "App\\Form\\StatusType")
            .unwrap();
        assert_eq!(
            s.parent.as_deref(),
            Some("Symfony\\Component\\Form\\Extension\\Core\\Type\\ChoiceType")
        );
        let opts = options_of(&snap, "App\\Form\\StatusType");
        assert!(opts
            .iter()
            .any(|(o, from)| o == "choices" && from == "ChoiceType"));
        assert!(opts
            .iter()
            .any(|(o, from)| o == "label" && from == "FormType"));
    }

    #[test]
    fn completes_options_fields_and_navigates() {
        let dir = project();
        let snap = scan(dir.path(), &Config::default());
        let src = PRODUCT_TYPE.replace("['currency' => 'EUR']", "['currency' => 'EUR', 'gro¦']");
        let (src, pos) = at(&src);
        let list = complete(&snap, "src/Form/ProductType.php", &src, pos).unwrap();
        assert_eq!(list.items[0].label, "grouping");
        assert_eq!(list.items[0].detail, "MoneyType");

        let src = PRODUCT_TYPE.replace("['currency' => 'EUR']", "['currency' => '¦']");
        let (src, pos) = at(&src);
        assert!(complete(&snap, "src/Form/ProductType.php", &src, pos).is_none());

        let src = PRODUCT_TYPE.replace("->add('name')", "->add('pr¦')");
        let (src, pos) = at(&src);
        let list = complete(&snap, "src/Form/ProductType.php", &src, pos).unwrap();
        let labels: Vec<&str> = list.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["price"]);
        assert_eq!(list.items[0].detail, "int");

        let (src, pos) = at("<?php\nuse App\\Form\\ProductType;\n$form = $this->createForm(Product¦Type::class, $product);");
        let locs = goto(&snap, "src/Controller/X.php", &src, pos).unwrap();
        assert_eq!(locs[0].path, "src/Form/ProductType.php");

        let src = PRODUCT_TYPE.replace("->add('name')", "->add('na¦me')");
        let (src, pos) = at(&src);
        let locs = goto(&snap, "src/Form/ProductType.php", &src, pos).unwrap();
        assert_eq!(
            (locs[0].path.as_str(), locs[0].line),
            ("src/Entity/Product.php", 5)
        );
    }
}
