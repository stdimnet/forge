//! Virtual foreign keys and columns: relations and computed columns the user defines in the
//! project (`.forge/database/virtual.toml`, shared with the team like the data sources)
//! for databases that do not declare them. They are merged into the introspected model,
//! so the tree, Quick Documentation, join completion and the diagram use them like real
//! ones.

use serde::{Deserialize, Serialize};

use crate::model::{ColumnInfo, ForeignKey, SchemaInfo};

/// The project file relative to the project root.
pub const VIRTUAL_FILE: &str = ".forge/database/virtual.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VirtualForeignKey {
    /// Set when stored; the UI leaves it out.
    #[serde(default)]
    pub data_source: String,
    pub schema: String,
    pub table: String,
    /// Generated from the tables and columns when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub columns: Vec<String>,
    /// Empty means the same schema.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ref_schema: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
}

impl VirtualForeignKey {
    pub fn effective_name(&self) -> String {
        if self.name.is_empty() {
            format!(
                "vfk_{}_{}_{}",
                self.table,
                self.columns.join("_"),
                self.ref_table
            )
        } else {
            self.name.clone()
        }
    }

    fn ref_schema(&self) -> &str {
        if self.ref_schema.is_empty() {
            &self.schema
        } else {
            &self.ref_schema
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VirtualColumn {
    #[serde(default)]
    pub data_source: String,
    pub schema: String,
    pub table: String,
    pub name: String,
    /// SQL expression over the table's columns, e.g. `price * quantity`.
    pub expression: String,
    #[serde(default, rename = "type", skip_serializing_if = "String::is_empty")]
    pub data_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
}

/// The contents of [`VIRTUAL_FILE`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VirtualObjects {
    #[serde(default, rename = "foreignKey", skip_serializing_if = "Vec::is_empty")]
    pub foreign_keys: Vec<VirtualForeignKey>,
    #[serde(default, rename = "column", skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<VirtualColumn>,
}

impl VirtualObjects {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    pub fn to_text(&self) -> Result<String, String> {
        let body = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        Ok(format!(
            "# Forge virtual foreign keys and columns (Database tool window).\n\n{body}"
        ))
    }

    pub fn is_empty(&self) -> bool {
        self.foreign_keys.is_empty() && self.columns.is_empty()
    }

    /// The objects of one data source.
    pub fn of(&self, data_source: &str) -> VirtualObjects {
        VirtualObjects {
            foreign_keys: self
                .foreign_keys
                .iter()
                .filter(|f| f.data_source == data_source)
                .cloned()
                .collect(),
            columns: self
                .columns
                .iter()
                .filter(|c| c.data_source == data_source)
                .cloned()
                .collect(),
        }
    }

    /// Replace the objects of `data_source` with `objects` (whose data source is set).
    pub fn replace(&mut self, data_source: &str, objects: VirtualObjects) {
        self.foreign_keys.retain(|f| f.data_source != data_source);
        self.columns.retain(|c| c.data_source != data_source);
        self.foreign_keys
            .extend(objects.foreign_keys.into_iter().map(|mut f| {
                f.data_source = data_source.into();
                f
            }));
        self.columns
            .extend(objects.columns.into_iter().map(|mut c| {
                c.data_source = data_source.into();
                c
            }));
    }

    /// Check a data source's objects against its model (when the tables are loaded).
    pub fn validate(&self) -> Result<(), String> {
        for f in &self.foreign_keys {
            if f.table.is_empty() || f.ref_table.is_empty() {
                return Err("a virtual foreign key needs a table and a referenced table".into());
            }
            if f.columns.is_empty() || f.columns.len() != f.ref_columns.len() {
                return Err(format!(
                    "virtual foreign key {}: the columns and referenced columns must pair up",
                    f.effective_name()
                ));
            }
        }
        for c in &self.columns {
            if c.name.trim().is_empty() || c.expression.trim().is_empty() {
                return Err(format!(
                    "virtual column {}.{} needs a name and an expression",
                    c.table, c.name
                ));
            }
        }
        Ok(())
    }

    /// Merge the objects of `data_source` into a schema of its model. Objects of tables the
    /// schema does not have are skipped.
    pub fn apply(&self, data_source: &str, schema: &mut SchemaInfo) {
        let schema_name = schema.name.clone();
        for f in self
            .foreign_keys
            .iter()
            .filter(|f| f.data_source == data_source && f.schema == schema_name)
        {
            let Some(t) = schema.table_mut(&f.table) else {
                continue;
            };
            let name = f.effective_name();
            t.foreign_keys.retain(|k| !(k.is_virtual && k.name == name));
            t.foreign_keys.push(ForeignKey {
                name,
                columns: f.columns.clone(),
                ref_schema: f.ref_schema().into(),
                ref_table: f.ref_table.clone(),
                ref_columns: f.ref_columns.clone(),
                on_update: String::new(),
                on_delete: String::new(),
                is_virtual: true,
            });
        }
        for c in self
            .columns
            .iter()
            .filter(|c| c.data_source == data_source && c.schema == schema_name)
        {
            let Some(t) = schema.table_mut(&c.table) else {
                continue;
            };
            t.columns.retain(|x| !(x.is_virtual && x.name == c.name));
            t.columns.push(ColumnInfo {
                name: c.name.clone(),
                data_type: c.data_type.clone(),
                nullable: true,
                default: None,
                auto_increment: false,
                comment: c.comment.clone(),
                is_virtual: true,
                expression: Some(c.expression.clone()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TableInfo;

    fn fk() -> VirtualForeignKey {
        VirtualForeignKey {
            data_source: "shop".into(),
            schema: "public".into(),
            table: "orders".into(),
            name: String::new(),
            columns: vec!["customer".into()],
            ref_schema: String::new(),
            ref_table: "users".into(),
            ref_columns: vec!["id".into()],
        }
    }

    #[test]
    fn round_trips_through_toml() {
        let mut v = VirtualObjects::default();
        v.foreign_keys.push(fk());
        v.columns.push(VirtualColumn {
            data_source: "shop".into(),
            schema: "public".into(),
            table: "orders".into(),
            name: "gross".into(),
            expression: "total * 1.19".into(),
            data_type: "numeric".into(),
            comment: String::new(),
        });
        let text = v.to_text().unwrap();
        assert!(text.contains("[[foreignKey]]"), "{text}");
        assert!(text.contains("refTable = \"users\""), "{text}");
        assert!(text.contains("[[column]]"), "{text}");
        assert_eq!(VirtualObjects::parse(&text).unwrap(), v);
        assert!(VirtualObjects::parse("").unwrap().is_empty());
    }

    #[test]
    fn replaces_one_data_source() {
        let mut v = VirtualObjects::default();
        v.foreign_keys.push(fk());
        v.foreign_keys.push(VirtualForeignKey {
            data_source: "other".into(),
            ..fk()
        });
        v.replace("shop", VirtualObjects::default());
        assert_eq!(v.foreign_keys.len(), 1);
        assert_eq!(v.foreign_keys[0].data_source, "other");
        assert!(v.of("shop").is_empty());
    }

    #[test]
    fn applies_to_a_schema_once() {
        let mut v = VirtualObjects::default();
        v.foreign_keys.push(fk());
        let mut s = SchemaInfo {
            name: "public".into(),
            tables: vec![TableInfo {
                name: "orders".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        v.apply("shop", &mut s);
        v.apply("shop", &mut s);
        v.apply("other", &mut s);
        let fks = &s.tables[0].foreign_keys;
        assert_eq!(fks.len(), 1);
        assert!(fks[0].is_virtual);
        assert_eq!(fks[0].name, "vfk_orders_customer_users");
        assert_eq!(fks[0].ref_schema, "public");
    }

    #[test]
    fn validates_pairs() {
        let mut v = VirtualObjects::default();
        let mut bad = fk();
        bad.ref_columns.push("x".into());
        v.foreign_keys.push(bad);
        assert!(v.validate().is_err());
        v.foreign_keys[0] = fk();
        assert!(v.validate().is_ok());
    }
}
