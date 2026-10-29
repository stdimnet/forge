//! The data source model as the resolver sees it: tables, columns, schemas and routines by
//! name (case-insensitive, like unquoted SQL names), with "unknown" for schemas that were not
//! introspected so that nothing is reported against them.

use forge_dbmodel::{DbModel, RoutineInfo, TableInfo};

/// A lookup result: found, known to be missing, or not knowable (schema not loaded, no model).
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup<T> {
    Found(T),
    Missing,
    Unknown,
}

pub struct Catalog<'a> {
    pub model: &'a DbModel,
    /// Schema of unqualified names.
    pub default_schema: String,
}

impl<'a> Catalog<'a> {
    pub fn new(model: &'a DbModel, default_schema: Option<&str>) -> Self {
        let default_schema = default_schema
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| model.current_schema().map(str::to_string))
            .or_else(|| model.loaded.keys().next().cloned())
            .unwrap_or_else(|| forge_dbmodel::ddl::default_schema(model.dialect).to_string());
        Self {
            model,
            default_schema,
        }
    }

    /// The schema's real name when the model knows it.
    pub fn schema(&self, name: &str) -> Option<&'a str> {
        let m = self.model;
        m.loaded
            .keys()
            .map(String::as_str)
            .chain(m.schemas.iter().map(|s| s.name.as_str()))
            .find(|s| s.eq_ignore_ascii_case(name))
    }

    fn loaded(&self, schema: &str) -> Option<(&'a str, &'a forge_dbmodel::SchemaInfo)> {
        self.model
            .loaded
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(schema))
            .map(|(k, v)| (k.as_str(), v))
    }

    /// A table or view; unqualified names look in the default schema, then in the other
    /// loaded schemas.
    pub fn table(&self, schema: Option<&str>, name: &str) -> Lookup<(&'a str, &'a TableInfo)> {
        if let Some(s) = schema {
            return match self.loaded(s) {
                Some((sn, info)) => match info.table(name) {
                    Some(t) => Lookup::Found((sn, t)),
                    None => Lookup::Missing,
                },
                None if self.schema(s).is_some() => Lookup::Unknown,
                // Database-qualified names in engines with catalogs, linked servers, …
                None if self.model.schemas.is_empty() => Lookup::Unknown,
                None => Lookup::Missing,
            };
        }
        if let Some((sn, info)) = self.loaded(&self.default_schema) {
            if let Some(t) = info.table(name) {
                return Lookup::Found((sn, t));
            }
        }
        for (sn, info) in &self.model.loaded {
            if let Some(t) = info.table(name) {
                return Lookup::Found((sn.as_str(), t));
            }
        }
        if self.loaded(&self.default_schema).is_some() {
            Lookup::Missing
        } else {
            Lookup::Unknown
        }
    }

    /// A routine by name (functions are only navigated to, never reported as missing:
    /// built-in functions are not in the model).
    pub fn routine(&self, schema: Option<&str>, name: &str) -> Option<(&'a str, &'a RoutineInfo)> {
        let find = |sn: &'a str, info: &'a forge_dbmodel::SchemaInfo| {
            info.routines
                .iter()
                .find(|r| r.name.eq_ignore_ascii_case(name))
                .map(|r| (sn, r))
        };
        match schema {
            Some(s) => self.loaded(s).and_then(|(sn, info)| find(sn, info)),
            None => self
                .loaded(&self.default_schema)
                .and_then(|(sn, info)| find(sn, info))
                .or_else(|| {
                    self.model
                        .loaded
                        .iter()
                        .find_map(|(sn, info)| find(sn.as_str(), info))
                }),
        }
    }

    /// Whether a sequence of that name exists (`seq.NEXTVAL` in Oracle).
    pub fn is_sequence(&self, name: &str) -> bool {
        self.model.loaded.values().any(|s| {
            s.sequences
                .iter()
                .any(|q| q.name.eq_ignore_ascii_case(name))
        })
    }
}
