//! Tables and columns from `db/schema.rb` and the migrations in `db/migrate` (those newer
//! than the schema's version, or all of them without a schema): `create_table` with its
//! column calls, `t.references`, `t.timestamps`, `add_column`, `remove_column`,
//! `rename_column`, `add_reference`, `change_table`, `rename_table` and `drop_table`.

use serde::{Deserialize, Serialize};

use crate::source::{call_args, logical_lines, tokens, LineIndex, Tok, Val};
use crate::Location;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    /// `string`, `integer`, `references` columns are `bigint`, …
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub not_null: bool,
    pub location: Location,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
    /// The `create_table` in schema.rb or the migration that created it.
    pub location: Location,
    /// Migrations that create or change it (newest last).
    pub migrations: Vec<Location>,
}

const COLUMN_TYPES: &[&str] = &[
    "string",
    "text",
    "integer",
    "bigint",
    "float",
    "decimal",
    "numeric",
    "datetime",
    "timestamp",
    "timestamptz",
    "time",
    "date",
    "binary",
    "blob",
    "boolean",
    "json",
    "jsonb",
    "uuid",
    "inet",
    "cidr",
    "citext",
    "hstore",
    "money",
    "virtual",
    "column",
    "enum",
    "interval",
    "point",
    "xml",
    "tsvector",
];

/// The version of `ActiveRecord::Schema[7.1].define(version: 2024_10_01_093000)`.
pub fn schema_version(text: &str) -> Option<u64> {
    let i = text.find("version:")?;
    let digits: String = text[i + 8..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '_')
        .filter(|c| *c != '_')
        .collect();
    digits.parse().ok()
}

/// The version of a migration file: `db/migrate/20241001093000_create_articles.rb`.
pub fn migration_version(path: &str) -> Option<u64> {
    let name = path.rsplit('/').next()?;
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 3 {
        return None;
    }
    digits.parse().ok()
}

struct Builder {
    tables: Vec<Table>,
}

impl Builder {
    /// Record that a migration changes a table (once per file).
    fn touch(&mut self, table: &str, loc: &Location) {
        if let Some(t) = self.table(table) {
            if !t.migrations.iter().any(|m| m.path == loc.path) {
                t.migrations.push(loc.clone());
            }
        }
    }

    fn table(&mut self, name: &str) -> Option<&mut Table> {
        self.tables.iter_mut().find(|t| t.name == name)
    }

    fn add(&mut self, table: &str, name: &str, kind: &str, not_null: bool, loc: &Location) {
        if let Some(t) = self.table(table) {
            let col = Column {
                name: name.to_string(),
                kind: kind.to_string(),
                not_null,
                location: loc.clone(),
            };
            match t.columns.iter_mut().find(|c| c.name == name) {
                Some(c) => *c = col,
                None => t.columns.push(col),
            }
        }
    }

    fn reference(&mut self, table: &str, name: &str, args: &crate::source::Args, loc: &Location) {
        let not_null = matches!(args.get("null"), Some(Val::Bool(false)));
        let kind = match args.text("type") {
            Some(t) => t.to_string(),
            None => "bigint".into(),
        };
        self.add(table, &format!("{name}_id"), &kind, not_null, loc);
        if matches!(args.get("polymorphic"), Some(Val::Bool(true))) {
            self.add(table, &format!("{name}_type"), "string", not_null, loc);
        }
    }

    /// A column call on the table variable: `t.string :title, null: false`.
    fn column_call(&mut self, table: &str, method: &str, args: &crate::source::Args, loc: &Location) {
        let names: Vec<String> = args
            .positional
            .iter()
            .filter_map(|v| match v {
                Val::Str(s) | Val::Sym(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        let not_null = matches!(args.get("null"), Some(Val::Bool(false)));
        match method {
            "timestamps" => {
                for n in ["created_at", "updated_at"] {
                    self.add(table, n, "datetime", !matches!(args.get("null"), Some(Val::Bool(true))), loc);
                }
            }
            "references" | "belongs_to" => {
                for n in names {
                    self.reference(table, &n, args, loc);
                }
            }
            "remove" => {
                if let Some(t) = self.table(table) {
                    t.columns.retain(|c| !names.contains(&c.name));
                }
            }
            "remove_references" | "remove_belongs_to" => {
                if let Some(t) = self.table(table) {
                    t.columns
                        .retain(|c| !names.iter().any(|n| c.name == format!("{n}_id") || c.name == format!("{n}_type")));
                }
            }
            "rename" => {
                if let (Some(from), Some(to)) = (names.first(), names.get(1)) {
                    if let Some(t) = self.table(table) {
                        if let Some(c) = t.columns.iter_mut().find(|c| &c.name == from) {
                            c.name = to.clone();
                        }
                    }
                }
            }
            "column" => {
                if let (Some(n), Some(k)) = (names.first(), names.get(1)) {
                    self.add(table, n, k, not_null, loc);
                }
            }
            m if COLUMN_TYPES.contains(&m) => {
                for n in names {
                    self.add(table, &n, m, not_null, loc);
                }
            }
            _ => {}
        }
    }

    fn file(&mut self, path: &str, text: &str, migration: bool) {
        let idx = LineIndex::new(text);
        // The table a `create_table` / `change_table` block works on, with its block
        // variable, and the depth of `do` blocks inside it.
        let mut open: Option<(String, String, usize)> = None;
        for l in logical_lines(text) {
            let toks = tokens(&l.text);
            let (line, character) = idx.position(l.start);
            let loc = Location {
                path: path.to_string(),
                line,
                character,
            };
            let Some(first) = toks.first() else { continue };
            if let Some((table, var, depth)) = &mut open {
                if first.is_ident("end") {
                    if *depth == 0 {
                        open = None;
                    } else {
                        *depth -= 1;
                    }
                    continue;
                }
                if toks.iter().any(|t| t.is_ident("do")) {
                    *depth += 1;
                }
                // `t.string "title"`.
                if first.ident() == Some(var.as_str())
                    && toks.get(1).is_some_and(|t| t.is_punct('.'))
                {
                    if let Some(method) = toks.get(2).and_then(|t| t.ident()) {
                        let method = method.to_string();
                        let table = table.clone();
                        let args = call_args(&toks[2..]);
                        self.column_call(&table, &method, &args, &loc);
                    }
                }
                continue;
            }
            let Tok::Ident(method) = &first.tok else { continue };
            let args = call_args(&toks);
            let pos: Vec<String> = args
                .positional
                .iter()
                .filter_map(|v| match v {
                    Val::Str(s) | Val::Sym(s) => Some(s.clone()),
                    _ => None,
                })
                .collect();
            let var = || -> String {
                // `do |t|`
                let bar = l.text.rfind('|').unwrap_or(0);
                let start = l.text[..bar].rfind('|').map(|i| i + 1).unwrap_or(bar);
                l.text[start..bar].trim().to_string()
            };
            match method.as_str() {
                "create_table" => {
                    let Some(name) = pos.first() else { continue };
                    self.tables.retain(|t| &t.name != name);
                    let mut t = Table {
                        name: name.clone(),
                        location: loc.clone(),
                        ..Table::default()
                    };
                    if migration {
                        t.migrations.push(loc.clone());
                    }
                    match args.get("id") {
                        Some(Val::Bool(false)) => {}
                        other => t.columns.push(Column {
                            name: args.text("primary_key").unwrap_or("id").to_string(),
                            kind: match other.and_then(Val::text) {
                                Some(k) => k.to_string(),
                                None => "bigint".into(),
                            },
                            not_null: true,
                            location: loc.clone(),
                        }),
                    }
                    self.tables.push(t);
                    if args.block {
                        open = Some((name.clone(), var(), 0));
                    }
                }
                "change_table" => {
                    let Some(name) = pos.first() else { continue };
                    if migration {
                        self.touch(name, &loc);
                    }
                    if args.block {
                        open = Some((name.clone(), var(), 0));
                    }
                }
                "drop_table" => {
                    if let Some(name) = pos.first() {
                        self.tables.retain(|t| &t.name != name);
                    }
                }
                "rename_table" => {
                    if let (Some(from), Some(to)) = (pos.first(), pos.get(1)) {
                        if let Some(t) = self.table(from) {
                            t.name = to.clone();
                        }
                    }
                }
                "add_column" => {
                    if let (Some(table), Some(col), Some(kind)) = (pos.first(), pos.get(1), pos.get(2)) {
                        let not_null = matches!(args.get("null"), Some(Val::Bool(false)));
                        self.add(table, col, kind, not_null, &loc);
                        self.touch(table, &loc);
                    }
                }
                "add_reference" | "add_belongs_to" => {
                    if let (Some(table), Some(name)) = (pos.first(), pos.get(1)) {
                        self.reference(table, name, &args, &loc);
                        self.touch(table, &loc);
                    }
                }
                "add_timestamps" => {
                    if let Some(table) = pos.first() {
                        for n in ["created_at", "updated_at"] {
                            self.add(table, n, "datetime", true, &loc);
                        }
                    }
                }
                "remove_column" | "remove_columns" => {
                    if let Some(table) = pos.first() {
                        let gone = &pos[1..];
                        // `remove_column :t, :c, :type` names the type third.
                        let gone: Vec<&String> = if method == "remove_column" {
                            gone.iter().take(1).collect()
                        } else {
                            gone.iter().collect()
                        };
                        if let Some(t) = self.table(table) {
                            t.columns.retain(|c| !gone.contains(&&c.name));
                        }
                        self.touch(table, &loc);
                    }
                }
                "remove_reference" | "remove_belongs_to" => {
                    if let (Some(table), Some(name)) = (pos.first(), pos.get(1)) {
                        if let Some(t) = self.table(table) {
                            t.columns.retain(|c| c.name != format!("{name}_id") && c.name != format!("{name}_type"));
                        }
                    }
                }
                "rename_column" => {
                    if let (Some(table), Some(from), Some(to)) = (pos.first(), pos.get(1), pos.get(2)) {
                        if let Some(t) = self.table(table) {
                            if let Some(c) = t.columns.iter_mut().find(|c| &c.name == from) {
                                c.name = to.clone();
                            }
                        }
                    }
                }
                "change_column" => {
                    if let (Some(table), Some(col), Some(kind)) = (pos.first(), pos.get(1), pos.get(2)) {
                        if let Some(t) = self.table(table) {
                            if let Some(c) = t.columns.iter_mut().find(|c| &c.name == col) {
                                c.kind = kind.clone();
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// The project's tables: schema.rb, then the migrations it does not contain yet. `files`
/// are (path, text) pairs of `db/schema.rb` and `db/migrate/*.rb`.
pub fn tables<'a>(files: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<Table> {
    let mut schema: Option<(&str, &str)> = None;
    let mut migrations: Vec<(u64, &str, &str)> = Vec::new();
    for (path, text) in files {
        if path == "db/schema.rb" {
            schema = Some((path, text));
        } else if path.starts_with("db/migrate/") && path.ends_with(".rb") {
            if let Some(v) = migration_version(path) {
                migrations.push((v, path, text));
            }
        }
    }
    migrations.sort();
    let mut b = Builder { tables: Vec::new() };
    let mut since = 0;
    if let Some((path, text)) = schema {
        b.file(path, text, false);
        since = schema_version(text).unwrap_or(0);
    }
    // Migrations add their locations even when schema.rb already has them.
    for (v, path, text) in &migrations {
        if *v <= since {
            let mut probe = Builder {
                tables: b
                    .tables
                    .iter()
                    .map(|t| Table {
                        name: t.name.clone(),
                        ..Table::default()
                    })
                    .collect(),
            };
            probe.file(path, text, true);
            for t in probe.tables {
                for m in t.migrations {
                    b.touch(&t.name, &m);
                }
            }
            continue;
        }
        b.file(path, text, true);
    }
    let mut tables = b.tables;
    tables.sort_by(|a, c| a.name.cmp(&c.name));
    tables
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_and_newer_migrations() {
        let schema = r#"ActiveRecord::Schema[7.1].define(version: 2024_10_01_000000) do
  create_table "articles", force: :cascade do |t|
    t.string "title", null: false
    t.text "body"
    t.references "user", null: false, foreign_key: true
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.index ["user_id"], name: "index_articles_on_user_id"
  end

  create_table "taggings", id: false do |t|
    t.bigint "tag_id"
  end
end
"#;
        let old = "class CreateArticles < ActiveRecord::Migration[7.1]\n  def change\n    create_table :articles do |t|\n      t.string :title\n    end\n  end\nend\n";
        let newer = "class AddStatusToArticles < ActiveRecord::Migration[7.1]\n  def change\n    add_column :articles, :status, :integer, default: 0, null: false\n    rename_column :articles, :body, :content\n    remove_column :articles, :created_at, :datetime\n    create_table :comments do |c|\n      c.belongs_to :commentable, polymorphic: true\n      c.text :body\n      c.timestamps\n    end\n  end\nend\n";
        let files = [
            ("db/schema.rb", schema),
            ("db/migrate/20240901000000_create_articles.rb", old),
            ("db/migrate/20241015000000_add_status_to_articles.rb", newer),
        ];
        let tables = tables(files.iter().map(|(p, t)| (*p, *t)));
        let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["articles", "comments", "taggings"]);
        let cols: Vec<String> = tables[0]
            .columns
            .iter()
            .map(|c| format!("{}:{}{}", c.name, c.kind, if c.not_null { "!" } else { "" }))
            .collect();
        assert_eq!(
            cols,
            ["id:bigint!", "title:string!", "content:text", "user_id:bigint!", "updated_at:datetime!", "status:integer!"]
        );
        assert_eq!(tables[0].location.line, 1);
        assert_eq!(tables[0].migrations.len(), 2);
        assert_eq!(tables[0].migrations[0].path, "db/migrate/20240901000000_create_articles.rb");
        let comments: Vec<&str> = tables[1].columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(comments, ["id", "commentable_id", "commentable_type", "body", "created_at", "updated_at"]);
        assert_eq!(tables[2].columns.len(), 1);
    }
}
