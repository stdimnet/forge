//! Entity mapping metadata as the DQL functions need it. `phpforge-orm-doctrine` fills it
//! from PHP attributes, annotations, XML and YAML; browser mode passes it in as JSON.

use serde::{Deserialize, Serialize};

/// A position in a project file (relative `/` path, 0-based line and byte column).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub path: String,
    pub line: u32,
    #[serde(default)]
    pub column: u32,
}

impl Location {
    pub fn new(path: impl Into<String>, line: u32, column: u32) -> Self {
        Self {
            path: path.into(),
            line,
            column,
        }
    }
}

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
    /// `manyToOne`, `many-to-one`, `ManyToOne`, ...
    pub fn parse(s: &str) -> Option<Self> {
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

/// Entities and repositories of a project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub entities: Vec<Entity>,
    pub repositories: Vec<Repository>,
}

impl Snapshot {
    /// Entity by FQN, legacy `Bundle:Entity` alias or (unique) short name.
    pub fn entity(&self, name: &str) -> Option<&Entity> {
        let name = name.trim().trim_start_matches('\\').replace("\\\\", "\\");
        if let Some(e) = self
            .entities
            .iter()
            .find(|e| e.class.eq_ignore_ascii_case(&name))
        {
            return Some(e);
        }
        let short = name
            .rsplit([':', '\\'])
            .next()
            .unwrap_or(&name)
            .to_ascii_lowercase();
        self.entities
            .iter()
            .find(|e| e.name.to_ascii_lowercase() == short)
    }

    /// Entity managed by a repository class.
    pub fn entity_of_repository(&self, repository: &str) -> Option<&Entity> {
        let repository = repository.trim_start_matches('\\');
        self.repositories
            .iter()
            .find(|r| r.class.eq_ignore_ascii_case(repository))
            .and_then(|r| r.entity.as_deref())
            .and_then(|e| self.entity(e))
            .or_else(|| {
                self.entities.iter().find(|e| {
                    e.repository_class
                        .as_deref()
                        .is_some_and(|r| r.eq_ignore_ascii_case(repository))
                })
            })
    }
}
