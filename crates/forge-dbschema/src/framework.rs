//! The migration of a schema compare as a framework migration file, for projects that keep
//! their schema in migrations: a Laravel migration (schema builder calls, `up` and `down`)
//! or a Doctrine Migrations class (`addSql` with the dialect's statements). `down` is the
//! migration back from the source to the target.

use forge_dbmodel::{generate, ColumnInfo, ForeignKey, IndexInfo, SchemaInfo, TableInfo};
use forge_sql::Dialect;
use serde::{Deserialize, Serialize};

use crate::compare::{self, DiffKind, ItemCategory};
use crate::migrate::{migration, MigrationOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameworkKind {
    Laravel,
    Doctrine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationFile {
    /// Project-relative path (`database/migrations/…`, `migrations/Version….php`).
    pub path: String,
    pub text: String,
}

/// `(year, month, day, hour, minute, second)` of Unix seconds (UTC).
fn civil(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's days-from-civil, inverted.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        y,
        m,
        d,
        (rem / 3600) as u32,
        (rem % 3600 / 60) as u32,
        (rem % 60) as u32,
    )
}

/// A PHP single-quoted string.
fn php(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn php_list(items: &[String]) -> String {
    format!(
        "[{}]",
        items.iter().map(|c| php(c)).collect::<Vec<_>>().join(", ")
    )
}

/// `snake_case` words of a name for the file name.
fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

// ---- Laravel ----------------------------------------------------------------------------

/// Arguments in `(…)` of a type: `varchar(255)` → `["255"]`.
fn type_args(t: &str) -> (String, Vec<String>) {
    match (t.find('('), t.rfind(')')) {
        (Some(a), Some(b)) if b > a => (
            t[..a].trim().to_string(),
            t[a + 1..b]
                .split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect(),
        ),
        _ => (t.trim().to_string(), Vec::new()),
    }
}

/// The schema builder call that creates a column, without modifiers; `true` when it is an
/// auto-incrementing key (`id()`, `increments()`), which implies the primary key.
fn blueprint_type(c: &ColumnInfo, single_pk: bool) -> (String, bool) {
    let norm = compare::normalize_type(&c.data_type);
    let unsigned = norm.contains("unsigned");
    let norm = norm.replace(" unsigned", "");
    let (base, args) = type_args(&norm);
    let n = php(&c.name);
    let auto = c.auto_increment && single_pk;
    let call = |m: &str| format!("{m}({n})");
    let with = |m: &str, a: &[String]| {
        if a.is_empty() {
            format!("{m}({n})")
        } else {
            format!("{m}({n}, {})", a.join(", "))
        }
    };
    let s = match base.as_str() {
        "bigint" | "bigserial" if auto => {
            return (
                if c.name == "id" {
                    "id()".into()
                } else {
                    call("bigIncrements")
                },
                true,
            );
        }
        "integer" | "mediumint" | "serial" if auto => return (call("increments"), true),
        "bigint" => call(if unsigned {
            "unsignedBigInteger"
        } else {
            "bigInteger"
        }),
        "integer" | "mediumint" => call(if unsigned {
            "unsignedInteger"
        } else {
            "integer"
        }),
        "smallint" => call(if unsigned {
            "unsignedSmallInteger"
        } else {
            "smallInteger"
        }),
        "tinyint" if args.first().map(String::as_str) == Some("1") => call("boolean"),
        "tinyint" => call(if unsigned {
            "unsignedTinyInteger"
        } else {
            "tinyInteger"
        }),
        "boolean" => call("boolean"),
        "varchar" | "nvarchar" | "varchar2" => with("string", &args),
        "char" | "nchar" => with("char", &args),
        "text" | "clob" => call("text"),
        "mediumtext" => call("mediumText"),
        "longtext" => call("longText"),
        "numeric" => with("decimal", &args),
        "double" => call("double"),
        "real" | "float" => call("float"),
        "date" => call("date"),
        "datetime" | "datetime2" => call("dateTime"),
        "timestamp" => call("timestamp"),
        "timestamptz" => call("timestampTz"),
        "time" => call("time"),
        "year" => call("year"),
        "json" => call("json"),
        "jsonb" => call("jsonb"),
        "uuid" | "uniqueidentifier" => call("uuid"),
        "blob" | "bytea" | "binary" | "varbinary" | "longblob" => call("binary"),
        "enum" => format!("enum({n}, [{}])", args.join(", ")),
        _ => format!("string({n}) /* {} */", c.data_type),
    };
    (s, false)
}

fn php_default(d: &str) -> String {
    let t = d.trim();
    if t.eq_ignore_ascii_case("CURRENT_TIMESTAMP")
        || t.eq_ignore_ascii_case("now()")
        || t.eq_ignore_ascii_case("CURRENT_TIMESTAMP()")
    {
        return "useCurrent()".into();
    }
    if t.parse::<f64>().is_ok() || t.eq_ignore_ascii_case("true") || t.eq_ignore_ascii_case("false")
    {
        return format!("default({})", t.to_ascii_lowercase());
    }
    if let Some(inner) = t.strip_prefix('\'') {
        if let Some(end) = inner.rfind('\'') {
            return format!("default({})", php(&inner[..end].replace("''", "'")));
        }
    }
    format!("default(DB::raw({}))", php(t))
}

fn blueprint_column(c: &ColumnInfo, single_pk: bool, change: bool) -> (String, bool) {
    let (mut s, auto) = blueprint_type(c, single_pk);
    if c.nullable {
        s.push_str("->nullable()");
    } else if change {
        s.push_str("->nullable(false)");
    }
    if let Some(d) = &c.default {
        s.push_str(&format!("->{}", php_default(d)));
    }
    if c.auto_increment && !auto {
        s.push_str("->autoIncrement()");
    }
    if !c.comment.is_empty() {
        s.push_str(&format!("->comment({})", php(&c.comment)));
    }
    if change {
        s.push_str("->change()");
    }
    (format!("$table->{s};"), auto)
}

fn blueprint_index(i: &IndexInfo) -> String {
    let m = if i.unique { "unique" } else { "index" };
    if compare::generated_index_name(&i.name) {
        format!("$table->{m}({});", php_list(&i.columns))
    } else {
        format!("$table->{m}({}, {});", php_list(&i.columns), php(&i.name))
    }
}

fn blueprint_fk(f: &ForeignKey) -> String {
    let mut s = format!(
        "$table->foreign({}, {})->references({})->on({})",
        php_list(&f.columns),
        php(&f.name),
        php_list(&f.ref_columns),
        php(&f.ref_table)
    );
    for (what, rule) in [("onDelete", &f.on_delete), ("onUpdate", &f.on_update)] {
        if !rule.is_empty() && !rule.eq_ignore_ascii_case("NO ACTION") {
            s.push_str(&format!("->{what}({})", php(&rule.to_ascii_lowercase())));
        }
    }
    s.push(';');
    s
}

fn closure(call: &str, table: &str, lines: &[String]) -> String {
    let mut s = format!(
        "        Schema::{call}({}, function (Blueprint $table) {{\n",
        php(table)
    );
    for l in lines {
        s.push_str(&format!("            {l}\n"));
    }
    s.push_str("        });\n");
    s
}

fn create_lines(t: &TableInfo) -> Vec<String> {
    let mut lines = Vec::new();
    let single = t.primary_key.len() == 1;
    let mut implied_pk = false;
    for c in t.columns.iter().filter(|c| !c.is_virtual) {
        let in_pk = single && t.primary_key[0].eq_ignore_ascii_case(&c.name);
        let (l, auto) = blueprint_column(c, in_pk, false);
        implied_pk |= auto;
        lines.push(l);
    }
    if !t.primary_key.is_empty() && !implied_pk {
        lines.push(format!("$table->primary({});", php_list(&t.primary_key)));
    }
    for i in compare::plain_indexes(t) {
        lines.push(blueprint_index(i));
    }
    for f in compare::real_fks(t) {
        lines.push(blueprint_fk(f));
    }
    if !t.comment.is_empty() {
        lines.push(format!("$table->comment({});", php(&t.comment)));
    }
    lines
}

/// The schema builder calls that turn `target` into `source`.
fn laravel_body(
    d: Dialect,
    source: &SchemaInfo,
    target: &SchemaInfo,
    opts: &MigrationOptions,
) -> String {
    let mut out = String::new();
    let pairs = compare::pairs(source, target, &opts.compare);
    for (s, t) in &pairs {
        if let (None, Some(t)) = (s, t) {
            if !opts.drops {
                continue;
            }
            if t.kind.is_view() {
                out.push_str(&format!(
                    "        DB::statement({});\n",
                    php(&format!("DROP VIEW {}", d.quote_ident(&t.name)))
                ));
            } else {
                out.push_str(&format!(
                    "        Schema::dropIfExists({});\n",
                    php(&t.name)
                ));
            }
        }
    }
    for (s, t) in &pairs {
        match (s, t) {
            (Some(s), None) if s.kind.is_view() => {
                let sql = generate::table(d, "", s);
                out.push_str(&format!(
                    "        DB::statement({});\n",
                    php(sql.trim().trim_end_matches(';'))
                ));
            }
            (Some(s), None) => out.push_str(&closure("create", &s.name, &create_lines(s))),
            (Some(s), Some(t)) => {
                let items = compare::table_items(s, t, &opts.compare);
                if items.is_empty() {
                    continue;
                }
                if s.kind.is_view() || t.kind.is_view() {
                    out.push_str(&format!(
                        "        DB::statement({});\n",
                        php(&format!("DROP VIEW {}", d.quote_ident(&t.name)))
                    ));
                    let sql = generate::table(d, "", s);
                    out.push_str(&format!(
                        "        DB::statement({});\n",
                        php(sql.trim().trim_end_matches(';'))
                    ));
                    continue;
                }
                let single = s.primary_key.len() == 1;
                let mut lines = Vec::new();
                for i in items {
                    match (i.category, i.kind) {
                        (ItemCategory::Column, DiffKind::Removed) => {
                            if opts.drops {
                                lines.push(format!("$table->dropColumn({});", php(&i.name)));
                            }
                        }
                        (ItemCategory::Column, kind) => {
                            if let Some(c) = s.column(&i.name) {
                                let in_pk =
                                    single && s.primary_key[0].eq_ignore_ascii_case(&c.name);
                                lines.push(
                                    blueprint_column(
                                        c,
                                        in_pk && kind == DiffKind::Added,
                                        kind == DiffKind::Changed,
                                    )
                                    .0,
                                );
                            }
                        }
                        (ItemCategory::PrimaryKey, kind) => {
                            if kind != DiffKind::Added {
                                lines.push("$table->dropPrimary();".into());
                            }
                            if kind != DiffKind::Removed {
                                lines.push(format!(
                                    "$table->primary({});",
                                    php_list(&s.primary_key)
                                ));
                            }
                        }
                        (ItemCategory::Index, kind) => {
                            if kind != DiffKind::Added {
                                if let Some(x) = compare::plain_indexes(t).find(|x| {
                                    x.name == i.name
                                        || s.indexes
                                            .iter()
                                            .any(|y| y.name == i.name && compare::same_index(x, y))
                                }) {
                                    let m = if x.unique { "dropUnique" } else { "dropIndex" };
                                    lines.push(format!("$table->{m}({});", php(&x.name)));
                                }
                            }
                            if kind != DiffKind::Removed {
                                if let Some(x) =
                                    compare::plain_indexes(s).find(|x| x.name == i.name)
                                {
                                    lines.push(blueprint_index(x));
                                }
                            }
                        }
                        (ItemCategory::ForeignKey, kind) => {
                            if kind == DiffKind::Removed {
                                lines.push(format!("$table->dropForeign({});", php(&i.name)));
                                continue;
                            }
                            let Some(sf) = compare::real_fks(s).find(|f| f.name == i.name) else {
                                continue;
                            };
                            if kind == DiffKind::Changed {
                                if let Some(tf) =
                                    compare::real_fks(t).find(|f| compare::same_fk(sf, f))
                                {
                                    lines.push(format!("$table->dropForeign({});", php(&tf.name)));
                                }
                            }
                            lines.push(blueprint_fk(sf));
                        }
                        (ItemCategory::Comment, _) => {
                            lines.push(format!("$table->comment({});", php(&s.comment)))
                        }
                        (ItemCategory::Check, _) => lines.push(format!(
                            "// Check constraint {} ({}) is not supported by the schema builder.",
                            i.name,
                            i.source.or(i.target).unwrap_or_default()
                        )),
                        (ItemCategory::Definition, _) => {}
                    }
                }
                if !lines.is_empty() {
                    out.push_str(&closure("table", &s.name, &lines));
                }
            }
            _ => {}
        }
    }
    if out.is_empty() {
        out.push_str("        //\n");
    }
    out
}

fn laravel(
    d: Dialect,
    source: &SchemaInfo,
    target: &SchemaInfo,
    opts: &MigrationOptions,
) -> String {
    format!(
        "<?php\n\nuse Illuminate\\Database\\Migrations\\Migration;\nuse Illuminate\\Database\\Schema\\Blueprint;\nuse Illuminate\\Support\\Facades\\DB;\nuse Illuminate\\Support\\Facades\\Schema;\n\n\
         // Generated by Forge from comparing {} with {}.\nreturn new class extends Migration\n{{\n    /**\n     * Run the migrations.\n     */\n    public function up(): void\n    {{\n{}    }}\n\n    /**\n     * Reverse the migrations.\n     */\n    public function down(): void\n    {{\n{}    }}\n}};\n",
        source.name,
        target.name,
        laravel_body(d, source, target, opts),
        laravel_body(d, target, source, opts),
    )
}

// ---- Doctrine ---------------------------------------------------------------------------

fn add_sql(
    d: Dialect,
    source: &SchemaInfo,
    target: &SchemaInfo,
    opts: &MigrationOptions,
) -> String {
    let m = migration(d, source, target, opts);
    let mut out = String::new();
    for w in &m.warnings {
        out.push_str(&format!("        // Warning: {w}\n"));
    }
    for st in &m.statements {
        out.push_str(&format!("        $this->addSql({});\n", php(&st.sql)));
    }
    if m.statements.is_empty() {
        out.push_str("        // The schemas are identical.\n");
    }
    out
}

fn doctrine(
    d: Dialect,
    class: &str,
    source: &SchemaInfo,
    target: &SchemaInfo,
    opts: &MigrationOptions,
) -> String {
    // `down` runs the statements of the way back in the same (target) schema.
    let mut back_source = target.clone();
    back_source.name = source.name.clone();
    let mut back_target = source.clone();
    back_target.name = target.name.clone();
    format!(
        "<?php\n\ndeclare(strict_types=1);\n\nnamespace DoctrineMigrations;\n\nuse Doctrine\\DBAL\\Schema\\Schema;\nuse Doctrine\\Migrations\\AbstractMigration;\n\n\
         final class {class} extends AbstractMigration\n{{\n    public function getDescription(): string\n    {{\n        return {};\n    }}\n\n    public function up(Schema $schema): void\n    {{\n{}    }}\n\n    public function down(Schema $schema): void\n    {{\n{}    }}\n}}\n",
        php(&format!("Generated by Forge from comparing {} with {}", source.name, target.name)),
        add_sql(d, source, target, opts),
        add_sql(d, &back_source, &back_target, opts),
    )
}

/// The migration file of `kind` that turns `target` into `source`; `now` is the Unix time
/// the file is named after.
pub fn framework_migration(
    kind: FrameworkKind,
    d: Dialect,
    source: &SchemaInfo,
    target: &SchemaInfo,
    opts: &MigrationOptions,
    now: u64,
) -> MigrationFile {
    let (y, mo, da, h, mi, s) = civil(now);
    match kind {
        FrameworkKind::Laravel => {
            let name = slug(&format!("update {} schema", target.name));
            MigrationFile {
                path: format!(
                    "database/migrations/{y:04}_{mo:02}_{da:02}_{h:02}{mi:02}{s:02}_{name}.php"
                ),
                text: laravel(d, source, target, opts),
            }
        }
        FrameworkKind::Doctrine => {
            let class = format!("Version{y:04}{mo:02}{da:02}{h:02}{mi:02}{s:02}");
            MigrationFile {
                path: format!("migrations/{class}.php"),
                text: doctrine(d, &class, source, target, opts),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::tests::dev_and_prod;

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(1_793_191_545), (2026, 10, 28, 12, 45, 45));
        assert_eq!(civil(951_782_400), (2000, 2, 29, 0, 0, 0));
    }

    #[test]
    fn laravel_migration_file() {
        let (dev, prod) = dev_and_prod();
        let f = framework_migration(
            FrameworkKind::Laravel,
            Dialect::MySql,
            &dev,
            &prod,
            &MigrationOptions::default(),
            1_793_191_545,
        );
        assert_eq!(
            f.path,
            "database/migrations/2026_10_28_124545_update_prod_schema.php"
        );
        let t = &f.text;
        assert!(t.contains("return new class extends Migration"), "{t}");
        assert!(
            t.contains("        Schema::dropIfExists('legacy');\n"),
            "{t}"
        );
        assert!(t.contains("Schema::create('audit', function (Blueprint $table) {\n            $table->integer('id');\n            $table->text('message')->nullable();\n            $table->primary(['id']);\n        });"), "{t}");
        assert!(
            t.contains("$table->string('name', 150)->nullable(false)->change();"),
            "{t}"
        );
        assert!(
            t.contains("$table->timestamp('last_login')->nullable()->useCurrent();"),
            "{t}"
        );
        assert!(
            t.contains("$table->dropIndex('orders_user_id_index');"),
            "{t}"
        );
        assert!(t.contains("$table->dropForeign('orders_user_id_foreign');\n            $table->foreign(['user_id'], 'orders_user_id_foreign')->references(['id'])->on('users')->onDelete('restrict');"), "{t}");
        // down: the way back.
        let down = &t[t.find("function down").unwrap()..];
        assert!(down.contains("Schema::dropIfExists('audit');"), "{down}");
        assert!(down.contains("Schema::create('legacy'"), "{down}");
        assert!(down.contains("$table->dropColumn('last_login');"), "{down}");
        assert!(
            down.contains("$table->index(['user_id'], 'orders_user_id_index');"),
            "{down}"
        );
    }

    #[test]
    fn blueprint_types() {
        let c = |t: &str, auto: bool| ColumnInfo {
            name: "id".into(),
            data_type: t.into(),
            auto_increment: auto,
            ..Default::default()
        };
        assert_eq!(
            blueprint_type(&c("bigint unsigned", true), true),
            ("id()".into(), true)
        );
        assert_eq!(blueprint_type(&c("int", true), true).0, "increments('id')");
        assert_eq!(
            blueprint_type(&c("bigint unsigned", false), false).0,
            "unsignedBigInteger('id')"
        );
        assert_eq!(
            blueprint_type(&c("tinyint(1)", false), false).0,
            "boolean('id')"
        );
        assert_eq!(
            blueprint_type(&c("decimal(8, 2)", false), false).0,
            "decimal('id', 8, 2)"
        );
        assert_eq!(
            blueprint_type(&c("enum('a','b')", false), false).0,
            "enum('id', ['a', 'b'])"
        );
        assert_eq!(
            blueprint_type(&c("geometry", false), false).0,
            "string('id') /* geometry */"
        );
        assert_eq!(php_default("'it''s'"), "default('it\\'s')");
        assert_eq!(php_default("0"), "default(0)");
        assert_eq!(
            php_default("gen_random_uuid()"),
            "default(DB::raw('gen_random_uuid()'))"
        );
    }

    #[test]
    fn doctrine_migration_class() {
        let (dev, prod) = dev_and_prod();
        let f = framework_migration(
            FrameworkKind::Doctrine,
            Dialect::Postgres,
            &dev,
            &prod,
            &MigrationOptions::default(),
            1_793_191_545,
        );
        assert_eq!(f.path, "migrations/Version20261028124545.php");
        assert!(f
            .text
            .contains("final class Version20261028124545 extends AbstractMigration"));
        assert!(
            f.text
                .contains(r#"        $this->addSql('DROP TABLE "prod"."legacy"');"#),
            "{}",
            f.text
        );
        let down = &f.text[f.text.find("function down").unwrap()..];
        assert!(
            down.contains(r#"$this->addSql('DROP TABLE "prod"."audit"');"#),
            "{down}"
        );
        assert!(down.contains(r#"CREATE TABLE "prod"."legacy""#), "{down}");
    }
}
