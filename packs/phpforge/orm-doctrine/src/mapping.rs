//! Entity mapping metadata from attributes, annotations, XML and YAML.

use std::path::Path;
use std::sync::OnceLock;

use phpforge_lang_php::docblock::AnnotationArg;
use phpforge_lang_php::{parser, ClassLike, FileIndex, Member, SymbolKind};
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use regex::Regex;
use serde::{Deserialize, Serialize};
use yaml_rust2::{Yaml, YamlLoader};

use crate::{line_col, Location};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MappingSource {
    Attribute,
    Annotation,
    Xml,
    Yaml,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssociationKind {
    ManyToOne,
    OneToMany,
    ManyToMany,
    OneToOne,
}

impl AssociationKind {
    fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().replace('-', "").as_str() {
            "manytoone" => Self::ManyToOne,
            "onetomany" => Self::OneToMany,
            "manytomany" => Self::ManyToMany,
            "onetoone" => Self::OneToOne,
            _ => return None,
        })
    }

    /// To-many associations hold collections.
    pub fn is_collection(self) -> bool {
        matches!(self, Self::OneToMany | Self::ManyToMany)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    /// Doctrine type (`string`, `integer`, `datetime_immutable`, ...), or the embeddable
    /// class for embedded fields.
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nullable: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unique: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub id: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub generated: bool,
    /// Embedded value object (`#[ORM\Embedded]`); `type_name` is its class.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub embedded: bool,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Association {
    pub name: String,
    pub kind: AssociationKind,
    /// Target entity FQN.
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mapped_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inversed_by: Option<String>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    /// Fully qualified class name.
    pub class: String,
    /// Short class name.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_class: Option<String>,
    /// File holding the mapping (PHP class or mapping file).
    pub path: String,
    pub line: u32,
    pub source: MappingSource,
    /// `#[ORM\Embeddable]` value object rather than an entity.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub embeddable: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mapped_superclass: bool,
    pub fields: Vec<Field>,
    pub associations: Vec<Association>,
}

impl Entity {
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name == name)
    }

    pub fn association(&self, name: &str) -> Option<&Association> {
        self.associations.iter().find(|a| a.name == name)
    }

    pub fn location(&self) -> Location {
        Location::new(&self.path, self.line, 0)
    }
}

/// A repository class and the entity it manages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    pub path: String,
    pub line: u32,
}

/// Mapping element name (`Column`) of an attribute or annotation, or `None` when it is not
/// a Doctrine mapping.
fn mapping_name<'a>(name: &'a str, fqn: Option<&'a str>) -> Option<&'a str> {
    if let Some(f) = fqn {
        if let Some(rest) = f.strip_prefix("Doctrine\\ORM\\Mapping\\") {
            return Some(rest);
        }
    }
    let n = name.trim_start_matches('\\');
    n.strip_prefix("ORM\\")
        .or_else(|| n.strip_prefix("Doctrine\\ORM\\Mapping\\"))
        .or_else(|| n.strip_prefix("Mapping\\"))
}

/// One mapping attribute/annotation, normalized.
struct Mapping<'a> {
    name: &'a str,
    args: &'a [AnnotationArg],
}

impl Mapping<'_> {
    fn arg(&self, name: &str) -> Option<&str> {
        self.args
            .iter()
            .find(|a| a.name.as_deref() == Some(name))
            .map(|a| a.value.as_str())
    }

    fn positional(&self, i: usize) -> Option<&str> {
        self.args
            .iter()
            .filter(|a| a.name.is_none())
            .nth(i)
            .map(|a| a.value.as_str())
    }
}

fn mappings_of<'a>(
    attrs: &'a [phpforge_lang_php::Attribute],
    doc: Option<&'a phpforge_lang_php::docblock::DocBlock>,
    annotations: bool,
) -> (Vec<Mapping<'a>>, MappingSource) {
    let mut out: Vec<Mapping> = attrs
        .iter()
        .filter_map(|a| {
            mapping_name(&a.name, Some(&a.fqn)).map(|name| Mapping {
                name,
                args: &a.args,
            })
        })
        .collect();
    if !out.is_empty() || !annotations {
        return (out, MappingSource::Attribute);
    }
    if let Some(doc) = doc {
        out.extend(doc.annotations.iter().filter_map(|a| {
            mapping_name(&a.name, None).map(|name| Mapping {
                name,
                args: &a.args,
            })
        }));
    }
    (out, MappingSource::Annotation)
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    for q in ['"', '\''] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return v[1..v.len() - 1].replace("\\\\", "\\");
        }
    }
    v.to_string()
}

/// `Order::class`, `'App\Entity\Order'` or `"Order"` -> fully qualified name.
fn class_value(v: &str, file: &FileIndex, offset: usize) -> String {
    let v = v.trim();
    let name = match v.strip_suffix("::class") {
        Some(n) => n.to_string(),
        None => unquote(v),
    };
    if name.is_empty() {
        return name;
    }
    if !name.contains('\\') || v.ends_with("::class") {
        // Relative names in annotations are relative to the entity's namespace.
        return file.resolve_class(&name, offset);
    }
    name.trim_start_matches('\\').to_string()
}

/// Doctrine type of a `Types::DECIMAL` constant or a quoted name.
fn type_value(v: &str) -> String {
    let v = v.trim();
    if let Some(c) = v.strip_prefix("Types::") {
        return c.to_ascii_lowercase();
    }
    unquote(v)
}

/// Default Doctrine type for a PHP property type (as DoctrineBundle infers it).
fn infer_type(php_type: Option<&str>) -> String {
    let Some(t) = php_type else {
        return "string".into();
    };
    let t = t.trim_start_matches('?').trim_start_matches('\\');
    let t = t.split('|').find(|p| *p != "null").unwrap_or(t);
    match t.to_ascii_lowercase().as_str() {
        "int" => "integer",
        "float" => "float",
        "bool" => "boolean",
        "array" => "json",
        "string" => "string",
        "datetimeimmutable" => "datetime_immutable",
        "datetime" | "datetimeinterface" => "datetime",
        "dateinterval" => "dateinterval",
        "symfony\\component\\uid\\uuid" | "uuid" => "uuid",
        "symfony\\component\\uid\\ulid" | "ulid" => "ulid",
        _ => "string",
    }
    .into()
}

fn is_true(v: Option<&str>) -> bool {
    v.is_some_and(|v| v.trim() == "true")
}

/// Element type of `Collection<int, OrderItem>` / `OrderItem[]` in a docblock.
fn collection_element(t: &str) -> Option<String> {
    let t = t.trim();
    if let Some(inner) = t.split_once('<').map(|(_, r)| r.trim_end_matches('>')) {
        return inner.rsplit(',').next().map(|s| s.trim().to_string());
    }
    t.strip_suffix("[]").map(str::to_string)
}

fn property_field_or_association(
    entity: &mut Entity,
    file: &FileIndex,
    class: &ClassLike,
    m: &Member,
    annotations: bool,
) {
    let (maps, _) = mappings_of(&m.attributes, m.doc.as_ref(), annotations);
    if maps.is_empty() {
        return;
    }
    let offset = m.range.start;
    let php_type = m.effective_type();
    let mut field: Option<Field> = None;
    let mut id = false;
    let mut generated = false;
    for map in &maps {
        match map.name {
            "Id" => id = true,
            "GeneratedValue" => generated = true,
            "Column" => {
                let ty = map
                    .arg("type")
                    .or_else(|| map.positional(0).filter(|_| false))
                    .map(type_value)
                    .unwrap_or_else(|| infer_type(m.type_hint.as_deref()));
                field = Some(Field {
                    name: m.name.clone(),
                    type_name: ty,
                    column: map.arg("name").map(unquote),
                    length: map.arg("length").and_then(|l| l.trim().parse().ok()),
                    nullable: is_true(map.arg("nullable")),
                    unique: is_true(map.arg("unique")),
                    id: false,
                    generated: false,
                    embedded: false,
                    line: m.name_range.line,
                });
            }
            "Embedded" => {
                let class_name = map
                    .arg("class")
                    .or_else(|| map.positional(0))
                    .map(|v| class_value(v, file, offset))
                    .or_else(|| {
                        php_type
                            .as_deref()
                            .map(|t| file.resolve_class(t.trim_start_matches('?'), offset))
                    })
                    .unwrap_or_default();
                field = Some(Field {
                    name: m.name.clone(),
                    type_name: class_name,
                    line: m.name_range.line,
                    embedded: true,
                    ..Default::default()
                });
            }
            kind => {
                let Some(kind) = AssociationKind::parse(kind) else {
                    continue;
                };
                let target = map
                    .arg("targetEntity")
                    .or_else(|| map.positional(0))
                    .map(|v| class_value(v, file, offset))
                    .or_else(|| {
                        let t = php_type.as_deref()?;
                        let element = if kind.is_collection() {
                            collection_element(t)?
                        } else {
                            t.trim_start_matches('?').to_string()
                        };
                        Some(file.resolve_class(&element, offset))
                    })
                    .unwrap_or_default();
                entity.associations.push(Association {
                    name: m.name.clone(),
                    kind,
                    target,
                    mapped_by: map.arg("mappedBy").map(unquote),
                    inversed_by: map.arg("inversedBy").map(unquote),
                    line: m.name_range.line,
                });
            }
        }
    }
    let _ = class;
    if let Some(mut f) = field {
        f.id = id;
        f.generated = generated;
        entity.fields.push(f);
    } else if id {
        entity.fields.push(Field {
            name: m.name.clone(),
            type_name: infer_type(m.type_hint.as_deref()),
            id: true,
            generated,
            line: m.name_range.line,
            ..Default::default()
        });
    }
}

/// Entity metadata of one class, if it is mapped with attributes or annotations.
pub fn entity_from_class(file: &FileIndex, class: &ClassLike, annotations: bool) -> Option<Entity> {
    if class.kind != SymbolKind::Class {
        return None;
    }
    let (maps, source) = mappings_of(&class.attributes, class.doc.as_ref(), annotations);
    let head = maps
        .iter()
        .find(|m| matches!(m.name, "Entity" | "Embeddable" | "MappedSuperclass"))?;
    let mut entity = Entity {
        class: class.fqn.clone(),
        name: class.name.clone(),
        table: maps
            .iter()
            .find(|m| m.name == "Table")
            .and_then(|t| t.arg("name").or_else(|| t.positional(0)))
            .map(unquote),
        repository_class: head
            .arg("repositoryClass")
            .map(|v| class_value(v, file, class.range.start)),
        path: file.path.clone(),
        line: class.name_range.line,
        source,
        embeddable: head.name == "Embeddable",
        mapped_superclass: head.name == "MappedSuperclass",
        fields: Vec::new(),
        associations: Vec::new(),
    };
    for m in class.properties() {
        property_field_or_association(&mut entity, file, class, m, annotations);
    }
    Some(entity)
}

fn parent_entity_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"parent::__construct\(\s*\$\w+\s*,\s*([\w\\]+)::class").expect("valid regex")
    })
}

/// Repository metadata of a class extending `ServiceEntityRepository`/`EntityRepository`.
pub fn repository_from_class(
    file: &FileIndex,
    source: &str,
    class: &ClassLike,
) -> Option<Repository> {
    if !class
        .extends
        .iter()
        .any(|e| e.ends_with("ServiceEntityRepository") || e.ends_with("EntityRepository"))
    {
        return None;
    }
    let body = &source[class.range.start..class.range.end.min(source.len())];
    let entity = parent_entity_re()
        .captures(body)
        .map(|c| file.resolve_class(&c[1], class.range.start))
        .or_else(|| {
            let doc = class.doc.as_ref()?;
            let g = doc.generics.iter().find(|g| g.tag == "extends")?;
            let inner = g.type_name.split_once('<')?.1.trim_end_matches('>');
            Some(file.resolve_class(inner.trim(), class.range.start))
        });
    Some(Repository {
        class: class.fqn.clone(),
        entity,
        path: file.path.clone(),
        line: class.name_range.line,
    })
}

/// Entities and repositories in the PHP files of a directory.
pub fn from_php_dir(root: &Path, dir: &str, annotations: bool) -> (Vec<Entity>, Vec<Repository>) {
    let base = root.join(dir);
    let mut entities = Vec::new();
    let mut repositories = Vec::new();
    if !base.is_dir() {
        return (entities, repositories);
    }
    let mut parser = parser::new_parser();
    for entry in walkdir::WalkDir::new(&base)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !(e.file_type().is_dir()
                && matches!(n.as_ref(), "vendor" | "node_modules" | "var" | ".git"))
        })
        .flatten()
    {
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|e| e.to_str()) != Some("php")
        {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        if !(source.contains("ORM") || source.contains("Repository") || source.contains("Mapping"))
        {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        let index = parser::index_with(&mut parser, &rel, &source);
        for class in &index.classes {
            if let Some(e) = entity_from_class(&index, class, annotations) {
                entities.push(e);
            } else if let Some(r) = repository_from_class(&index, &source, class) {
                repositories.push(r);
            }
        }
    }
    (entities, repositories)
}

fn resolve_relative(name: &str, namespace: &str) -> String {
    let name = name.trim_start_matches('\\');
    if name.contains('\\') || namespace.is_empty() {
        name.to_string()
    } else {
        format!("{namespace}\\{name}")
    }
}

fn namespace_of(fqn: &str) -> &str {
    fqn.rsplit_once('\\').map(|(ns, _)| ns).unwrap_or("")
}

fn xml_attr(e: &quick_xml::events::BytesStart, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| {
            a.normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|v| v.into_owned())
        })
}

/// Entities of an XML mapping file (`*.orm.xml`).
pub fn from_xml(path: &str, xml: &str) -> Vec<Entity> {
    let mut reader = Reader::from_str(xml);
    let mut out: Vec<Entity> = Vec::new();
    let mut current: Option<Entity> = None;
    let mut pending_id: Option<Field> = None;
    loop {
        let pos = reader.buffer_position() as usize;
        let event = match reader.read_event() {
            Ok(e) => e,
            Err(_) => break,
        };
        let line = || {
            // The event starts at the first `<` after the previous position.
            let start = xml[pos.min(xml.len())..]
                .find('<')
                .map(|i| pos + i)
                .unwrap_or(pos);
            line_col(xml, start).0
        };
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(event, Event::Empty(_));
                let tag = e.name().as_ref().to_string();
                match tag.as_str() {
                    "entity" | "embeddable" | "mapped-superclass" => {
                        let class = xml_attr(e, "name").unwrap_or_default();
                        let entity = Entity {
                            name: class.rsplit('\\').next().unwrap_or(&class).to_string(),
                            table: xml_attr(e, "table"),
                            repository_class: xml_attr(e, "repository-class"),
                            path: path.to_string(),
                            line: line(),
                            source: MappingSource::Xml,
                            embeddable: tag == "embeddable",
                            mapped_superclass: tag == "mapped-superclass",
                            fields: Vec::new(),
                            associations: Vec::new(),
                            class,
                        };
                        if empty {
                            out.push(entity);
                        } else {
                            current = Some(entity);
                        }
                    }
                    "id" | "field" | "embedded" => {
                        let Some(entity) = current.as_mut() else {
                            continue;
                        };
                        let embedded = tag == "embedded";
                        let ns = namespace_of(&entity.class).to_string();
                        let f = Field {
                            name: xml_attr(
                                e,
                                if tag == "field" || tag == "id" || embedded {
                                    "name"
                                } else {
                                    "field"
                                },
                            )
                            .unwrap_or_default(),
                            type_name: if embedded {
                                resolve_relative(&xml_attr(e, "class").unwrap_or_default(), &ns)
                            } else {
                                xml_attr(e, "type").unwrap_or_else(|| "string".into())
                            },
                            column: xml_attr(e, "column"),
                            length: xml_attr(e, "length").and_then(|l| l.parse().ok()),
                            nullable: xml_attr(e, "nullable").as_deref() == Some("true"),
                            unique: xml_attr(e, "unique").as_deref() == Some("true"),
                            id: tag == "id",
                            generated: false,
                            embedded,
                            line: line(),
                        };
                        if tag == "id" && !empty {
                            pending_id = Some(f);
                        } else {
                            entity.fields.push(f);
                        }
                    }
                    "generator" => {
                        if let Some(f) = pending_id.as_mut() {
                            f.generated = xml_attr(e, "strategy").as_deref() != Some("NONE");
                        }
                    }
                    t => {
                        let (Some(kind), Some(entity)) =
                            (AssociationKind::parse(t), current.as_mut())
                        else {
                            continue;
                        };
                        let ns = namespace_of(&entity.class).to_string();
                        entity.associations.push(Association {
                            name: xml_attr(e, "field").unwrap_or_default(),
                            kind,
                            target: resolve_relative(
                                &xml_attr(e, "target-entity").unwrap_or_default(),
                                &ns,
                            ),
                            mapped_by: xml_attr(e, "mapped-by"),
                            inversed_by: xml_attr(e, "inversed-by"),
                            line: line(),
                        });
                    }
                }
            }
            Event::End(ref e) => match e.name().as_ref() {
                "id" => {
                    if let (Some(f), Some(entity)) = (pending_id.take(), current.as_mut()) {
                        entity.fields.push(f);
                    }
                }
                "entity" | "embeddable" | "mapped-superclass" => {
                    if let Some(entity) = current.take() {
                        out.push(entity);
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

fn ystr(y: &Yaml) -> Option<String> {
    match y {
        Yaml::String(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Line of `key:` at or after `after`, at any indentation.
fn yaml_key_line(text: &str, key: &str, after: u32) -> u32 {
    text.lines()
        .enumerate()
        .skip(after as usize)
        .find(|(_, l)| {
            let t = l.trim_start();
            t.starts_with(&format!("{key}:")) || t.starts_with(&format!("'{key}':"))
        })
        .map(|(i, _)| i as u32)
        .unwrap_or(after)
}

/// Entities of a YAML mapping file (`*.orm.yml`).
pub fn from_yaml(path: &str, text: &str) -> Vec<Entity> {
    let Ok(docs) = YamlLoader::load_from_str(text) else {
        return Vec::new();
    };
    let Some(Yaml::Hash(root)) = docs.first() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (k, v) in root {
        let Some(class) = ystr(k) else { continue };
        let line = yaml_key_line(text, &class, 0);
        let kind = v["type"].as_str().unwrap_or("entity");
        let ns = namespace_of(&class).to_string();
        let mut entity = Entity {
            name: class.rsplit('\\').next().unwrap_or(&class).to_string(),
            table: v["table"].as_str().map(str::to_string),
            repository_class: v["repositoryClass"].as_str().map(str::to_string),
            path: path.to_string(),
            line,
            source: MappingSource::Yaml,
            embeddable: kind == "embeddable",
            mapped_superclass: kind == "mappedSuperclass",
            fields: Vec::new(),
            associations: Vec::new(),
            class: class.clone(),
        };
        for (section, is_id) in [("id", true), ("fields", false)] {
            if let Some(h) = v[section].as_hash() {
                let section_line = yaml_key_line(text, section, line);
                for (name, def) in h {
                    let Some(name) = ystr(name) else { continue };
                    entity.fields.push(Field {
                        line: yaml_key_line(text, &name, section_line + 1),
                        type_name: def["type"].as_str().unwrap_or("string").to_string(),
                        column: def["column"].as_str().map(str::to_string),
                        length: def["length"].as_i64().map(|l| l as u32),
                        nullable: def["nullable"].as_bool().unwrap_or(false),
                        unique: def["unique"].as_bool().unwrap_or(false),
                        id: is_id,
                        generated: is_id && !def["generator"].is_badvalue(),
                        embedded: false,
                        name,
                    });
                }
            }
        }
        if let Some(h) = v["embedded"].as_hash() {
            for (name, def) in h {
                let Some(name) = ystr(name) else { continue };
                entity.fields.push(Field {
                    line: yaml_key_line(text, &name, line),
                    type_name: resolve_relative(def["class"].as_str().unwrap_or_default(), &ns),
                    embedded: true,
                    name,
                    ..Default::default()
                });
            }
        }
        for section in ["manyToOne", "oneToMany", "manyToMany", "oneToOne"] {
            let Some(h) = v[section].as_hash() else {
                continue;
            };
            let kind = AssociationKind::parse(section).expect("known association");
            let section_line = yaml_key_line(text, section, line);
            for (name, def) in h {
                let Some(name) = ystr(name) else { continue };
                entity.associations.push(Association {
                    line: yaml_key_line(text, &name, section_line + 1),
                    kind,
                    target: resolve_relative(def["targetEntity"].as_str().unwrap_or_default(), &ns),
                    mapped_by: def["mappedBy"].as_str().map(str::to_string),
                    inversed_by: def["inversedBy"].as_str().map(str::to_string),
                    name,
                });
            }
        }
        out.push(entity);
    }
    out
}

/// Entities of the XML/YAML mapping files in a directory.
pub fn from_mapping_dir(root: &Path, dir: &str) -> Vec<Entity> {
    let base = root.join(dir);
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&base) else {
        return out;
    };
    let mut files: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    files.sort();
    for p in files {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let rel = format!("{}/{name}", dir.trim_end_matches('/'));
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        if name.ends_with(".orm.xml") || name.ends_with(".dcm.xml") {
            out.extend(from_xml(&rel, &text));
        } else if name.ends_with(".orm.yml")
            || name.ends_with(".orm.yaml")
            || name.ends_with(".dcm.yml")
        {
            out.extend(from_yaml(&rel, &text));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    fn entity(src: &str) -> Entity {
        let idx = parser::index_source("src/Entity/X.php", src);
        entity_from_class(&idx, &idx.classes[0], true).unwrap()
    }

    #[test]
    fn attributes() {
        let e = entity(fixture::ORDER);
        assert_eq!(e.class, "App\\Entity\\Order");
        assert_eq!(e.table.as_deref(), Some("orders"));
        assert_eq!(
            e.repository_class.as_deref(),
            Some("App\\Repository\\OrderRepository")
        );
        assert_eq!(e.source, MappingSource::Attribute);
        let id = e.field("id").unwrap();
        assert!(id.id && id.generated);
        assert_eq!(id.type_name, "integer");
        assert_eq!(e.field("status").unwrap().length, Some(32));
        assert_eq!(e.field("total").unwrap().type_name, "decimal");
        let paid = e.field("paidAt").unwrap();
        assert_eq!(paid.type_name, "datetime_immutable");
        assert!(paid.nullable);
        let customer = e.association("customer").unwrap();
        assert_eq!(customer.kind, AssociationKind::ManyToOne);
        assert_eq!(customer.target, "App\\Entity\\Customer");
        assert_eq!(customer.inversed_by.as_deref(), Some("orders"));
        let items = e.association("items").unwrap();
        assert_eq!(items.target, "App\\Entity\\OrderItem");
        assert_eq!(items.mapped_by.as_deref(), Some("order"));
        let address = e.field("shippingAddress").unwrap();
        assert!(address.embedded);
        assert_eq!(address.type_name, "App\\Entity\\Address");
        assert_eq!(e.field("status").unwrap().line, 20);
    }

    #[test]
    fn legacy_annotations() {
        let e = entity(fixture::CUSTOMER);
        assert_eq!(e.source, MappingSource::Annotation);
        assert_eq!(e.table.as_deref(), Some("customers"));
        assert_eq!(
            e.repository_class.as_deref(),
            Some("App\\Repository\\CustomerRepository")
        );
        let email = e.field("email").unwrap();
        assert_eq!(email.type_name, "string");
        assert_eq!(email.length, Some(180));
        assert!(email.unique);
        assert!(e.field("id").unwrap().id);
        let orders = e.association("orders").unwrap();
        assert_eq!(orders.kind, AssociationKind::OneToMany);
        assert_eq!(orders.target, "App\\Entity\\Order");
    }

    #[test]
    fn embeddable() {
        let e = entity(fixture::ADDRESS);
        assert!(e.embeddable);
        assert_eq!(e.fields.len(), 2);
    }

    #[test]
    fn repositories() {
        let idx = parser::index_source(
            "src/Repository/OrderRepository.php",
            fixture::ORDER_REPOSITORY,
        );
        let r = repository_from_class(&idx, fixture::ORDER_REPOSITORY, &idx.classes[0]).unwrap();
        assert_eq!(r.entity.as_deref(), Some("App\\Entity\\Order"));
    }

    #[test]
    fn xml_mapping() {
        let e = &from_xml("config/doctrine/OrderItem.orm.xml", fixture::ITEM_XML)[0];
        assert_eq!(e.class, "App\\Entity\\OrderItem");
        assert_eq!(e.table.as_deref(), Some("order_items"));
        assert_eq!(e.line, 2);
        let id = e.field("id").unwrap();
        assert!(id.id && id.generated);
        assert_eq!(e.field("price").unwrap().type_name, "decimal");
        assert_eq!(e.field("quantity").unwrap().line, 6);
        let order = e.association("order").unwrap();
        assert_eq!(order.target, "App\\Entity\\Order");
        assert_eq!(order.line, 8);
        assert_eq!(
            e.association("product").unwrap().target,
            "App\\Entity\\Product"
        );
    }

    #[test]
    fn yaml_mapping() {
        let e = &from_yaml("config/doctrine/Product.orm.yml", fixture::PRODUCT_YML)[0];
        assert_eq!(e.name, "Product");
        assert_eq!(
            e.repository_class.as_deref(),
            Some("App\\Repository\\ProductRepository")
        );
        assert!(e.field("id").unwrap().generated);
        assert_eq!(e.field("name").unwrap().length, Some(255));
        assert_eq!(e.field("name").unwrap().line, 9);
        let tags = e.association("tags").unwrap();
        assert_eq!(tags.kind, AssociationKind::ManyToMany);
        assert_eq!(tags.target, "App\\Entity\\Tag");
    }

    #[test]
    fn type_inference() {
        assert_eq!(
            infer_type(Some("?\\DateTimeImmutable")),
            "datetime_immutable"
        );
        assert_eq!(infer_type(Some("bool")), "boolean");
        assert_eq!(infer_type(None), "string");
        assert_eq!(
            collection_element("Collection<int, OrderItem>").as_deref(),
            Some("OrderItem")
        );
    }
}
