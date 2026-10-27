//! Requests and answers of the IPC commands and the WebAssembly bridge: Compare Structure
//! with either side a schema of a data source or a DDL snapshot (SQL text parsed with the
//! DDL data source parser), the migration script and, optionally, a framework migration file.

use forge_dbmodel::ddl::{self, DdlFile};
use forge_dbmodel::{ModelWarning, SchemaInfo};
use forge_sql::Dialect;
use serde::{Deserialize, Serialize};

use crate::compare::{compare, SchemaDiff};
use crate::framework::{framework_migration, FrameworkKind, MigrationFile};
use crate::migrate::{migration, Migration, MigrationOptions};

/// One side of a comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareSide {
    /// An introspected schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaInfo>,
    /// Or a DDL snapshot: `CREATE TABLE …` statements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ddl: Option<String>,
    /// The schema a DDL snapshot stands for (default: the dialect's default schema).
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareRequest {
    /// The dialect of the migration (the target's).
    pub dialect: Dialect,
    /// How the schema should be.
    pub source: CompareSide,
    /// The schema to migrate.
    pub target: CompareSide,
    #[serde(default)]
    pub options: MigrationOptions,
    /// Also generate a Laravel or Doctrine migration file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework: Option<FrameworkKind>,
    /// Unix seconds the migration file is named after.
    #[serde(default)]
    pub now: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareResult {
    pub diff: SchemaDiff,
    pub migration: Migration,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<MigrationFile>,
    /// Statements of a DDL snapshot that did not parse.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ModelWarning>,
}

fn resolve(
    d: Dialect,
    side: &CompareSide,
    label: &str,
    warnings: &mut Vec<ModelWarning>,
) -> Result<SchemaInfo, String> {
    if let Some(s) = &side.schema {
        return Ok(s.clone());
    }
    let Some(text) = &side.ddl else {
        return Err(format!("The {label} has no schema"));
    };
    let name = if side.name.is_empty() {
        ddl::default_schema(d).to_string()
    } else {
        side.name.clone()
    };
    let parsed = ddl::parse(
        d,
        &name,
        &[DdlFile {
            path: format!("{label}.sql"),
            text: text.clone(),
        }],
    );
    warnings.extend(parsed.warnings);
    let mut schemas = parsed.schemas;
    let i = schemas.iter().position(|s| s.name == name).unwrap_or(0);
    Ok(if schemas.is_empty() {
        SchemaInfo {
            name,
            ..Default::default()
        }
    } else {
        schemas.swap_remove(i)
    })
}

/// Compare Structure and the migration of the target to the source.
pub fn compare_schemas(req: &CompareRequest) -> Result<CompareResult, String> {
    let mut warnings = Vec::new();
    let source = resolve(req.dialect, &req.source, "source", &mut warnings)?;
    let target = resolve(req.dialect, &req.target, "target", &mut warnings)?;
    let diff = compare(&source, &target, &req.options.compare);
    let migration = migration(req.dialect, &source, &target, &req.options);
    let file = req
        .framework
        .map(|k| framework_migration(k, req.dialect, &source, &target, &req.options, req.now));
    Ok(CompareResult {
        diff,
        migration,
        file,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_a_schema_with_a_ddl_snapshot() {
        let req: CompareRequest = serde_json::from_value(serde_json::json!({
            "dialect": "postgres",
            "source": { "ddl": "CREATE TABLE users (id bigint PRIMARY KEY, email varchar(255) NOT NULL);\nCREATE TABLE nope (;" },
            "target": { "schema": { "name": "public", "tables": [
                { "name": "users", "kind": "table", "columns": [{ "name": "id", "dataType": "bigint", "nullable": false }], "primaryKey": ["id"] }
            ] } },
            "framework": "laravel",
            "now": 0
        }))
        .unwrap();
        let r = compare_schemas(&req).unwrap();
        assert_eq!(r.diff.changed, 1);
        assert_eq!(
            r.migration.statements[0].sql,
            r#"ALTER TABLE "public"."users" ADD COLUMN "email" VARCHAR(255) NOT NULL"#
        );
        assert_eq!(r.warnings.len(), 1);
        assert!(r
            .file
            .unwrap()
            .text
            .contains("$table->string('email', 255);"));
        let bad: CompareRequest = serde_json::from_value(serde_json::json!({
            "dialect": "mysql", "source": {}, "target": { "ddl": "" }
        }))
        .unwrap();
        assert!(compare_schemas(&bad).is_err());
    }
}
