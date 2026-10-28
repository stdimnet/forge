//! Entity Framework Core, like Rider's EF Core actions: the projects that use EF Core and
//! their provider, the `DbContext` classes with their `DbSet`s and the migrations in the
//! sources (`[Migration("…")]` with `[DbContext(typeof(…))]`), `dotnet ef` command lines
//! (migrations add / remove / list, database update, dbcontext scaffold / list) and their
//! output (`migrations list` as JSON or text, typical failures with a hint).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::command::DotnetCommand;
use crate::context::DotnetContext;
use crate::csharp::{attribute_list, tokenize, Attribute};
use crate::msbuild::DotnetProject;
use crate::paths::file_name;

/// A project that references EF Core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EfProject {
    pub name: String,
    pub path: String,
    /// The database provider package (`Npgsql.EntityFrameworkCore.PostgreSQL`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// References `Microsoft.EntityFrameworkCore.Design` (needed by the startup project).
    pub design: bool,
    /// Can start the application (a startup project for `dotnet ef`).
    pub runnable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbSetInfo {
    pub name: String,
    pub entity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbContextInfo {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub base: String,
    pub sets: Vec<DbSetInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

/// A migration: from the sources, from `dotnet ef migrations list`, or both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Migration {
    /// `20241001120000_InitialCreate`.
    pub id: String,
    /// `InitialCreate`.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default)]
    pub line: u32,
    /// Applied to the database (`None`: not asked yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

impl Migration {
    /// `20241001120000` → `2024-10-01 12:00:00`.
    pub fn timestamp(&self) -> Option<String> {
        let ts = self.id.split('_').next()?;
        if ts.len() != 14 || !ts.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        Some(format!(
            "{}-{}-{} {}:{}:{}",
            &ts[0..4],
            &ts[4..6],
            &ts[6..8],
            &ts[8..10],
            &ts[10..12],
            &ts[12..14]
        ))
    }
}

/// The EF Core model of the solution.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EfModel {
    pub projects: Vec<EfProject>,
    pub contexts: Vec<DbContextInfo>,
    pub migrations: Vec<Migration>,
    /// `dotnet-ef` is in the local tool manifest (`.config/dotnet-tools.json`).
    #[serde(default)]
    pub local_tool: bool,
}

const PROVIDERS: &[&str] = &[
    "Npgsql.EntityFrameworkCore.PostgreSQL",
    "Microsoft.EntityFrameworkCore.SqlServer",
    "Microsoft.EntityFrameworkCore.Sqlite",
    "Pomelo.EntityFrameworkCore.MySql",
    "MySql.EntityFrameworkCore",
    "Oracle.EntityFrameworkCore",
    "Microsoft.EntityFrameworkCore.Cosmos",
    "Microsoft.EntityFrameworkCore.InMemory",
    "FirebirdSql.EntityFrameworkCore.Firebird",
];

/// Providers `dbcontext scaffold` offers.
pub const SCAFFOLD_PROVIDERS: &[&str] = &[
    "Npgsql.EntityFrameworkCore.PostgreSQL",
    "Microsoft.EntityFrameworkCore.SqlServer",
    "Microsoft.EntityFrameworkCore.Sqlite",
    "Pomelo.EntityFrameworkCore.MySql",
    "Oracle.EntityFrameworkCore",
];

/// The projects that use EF Core.
pub fn ef_projects(projects: &[DotnetProject]) -> Vec<EfProject> {
    projects
        .iter()
        .filter_map(|p| {
            let ef = p.packages.iter().find(|r| {
                r.id.eq_ignore_ascii_case("Microsoft.EntityFrameworkCore")
                    || r.id.starts_with("Microsoft.EntityFrameworkCore.")
                    || PROVIDERS.iter().any(|x| x.eq_ignore_ascii_case(&r.id))
                    || r.id.contains("EntityFrameworkCore")
            })?;
            let provider = p
                .packages
                .iter()
                .find(|r| PROVIDERS.iter().any(|x| x.eq_ignore_ascii_case(&r.id)))
                .map(|r| r.id.clone());
            Some(EfProject {
                name: p.name.clone(),
                path: p.path.clone(),
                provider,
                design: p.packages.iter().any(|r| {
                    r.id.eq_ignore_ascii_case("Microsoft.EntityFrameworkCore.Design")
                }),
                runnable: p.is_runnable(),
                version: Some(ef.version.clone()).filter(|v| !v.is_empty()),
            })
        })
        .collect()
}

fn attr<'a>(attrs: &'a [Attribute], name: &str) -> Option<&'a Attribute> {
    attrs.iter().find(|a| a.name == name)
}

/// `DbContext` classes and migrations of a C# file.
pub fn scan_source(path: &str, text: &str) -> (Vec<DbContextInfo>, Vec<Migration>) {
    if !text.contains("DbContext") && !text.contains("Migration") {
        return (Vec::new(), Vec::new());
    }
    let tokens = tokenize(text);
    let mut contexts: Vec<DbContextInfo> = Vec::new();
    let mut migrations = Vec::new();
    let mut pending: Vec<Attribute> = Vec::new();
    let mut i = 0;
    // The context whose body we are in, with the brace depth of its body.
    let mut depth = 0i32;
    let mut current: Option<(usize, i32)> = None;
    while i < tokens.len() {
        let t = &tokens[i];
        if t.is('[') {
            let (a, end) = attribute_list(&tokens, i);
            pending.extend(a);
            i = end;
            continue;
        }
        if t.is('{') {
            depth += 1;
        } else if t.is('}') {
            depth -= 1;
            if current.is_some_and(|(_, d)| depth < d) {
                current = None;
            }
        } else if t.is(';') {
            pending.clear();
        } else if t.is_ident("class") {
            if let Some(name) = tokens.get(i + 1).and_then(|n| n.ident()) {
                let mut j = i + 2;
                let mut base = None;
                while j < tokens.len() && !tokens[j].is('{') && !tokens[j].is(';') {
                    if tokens[j].is(':') {
                        base = tokens
                            .get(j + 1)
                            .and_then(|b| b.ident())
                            .map(str::to_string);
                    }
                    j += 1;
                }
                if let Some(m) = attr(&pending, "Migration").and_then(|a| a.strings.first()) {
                    if !migrations.iter().any(|x: &Migration| x.id == *m) {
                        migrations.push(Migration {
                            name: m.split_once('_').map(|(_, n)| n).unwrap_or(m).to_string(),
                            id: m.clone(),
                            context: attr(&pending, "DbContext")
                                .and_then(|a| a.types.first().cloned()),
                            file: Some(path.to_string()),
                            line: t.line,
                            applied: None,
                            project: None,
                        });
                    }
                } else if let Some(b) =
                    base.filter(|b| b.ends_with("DbContext") && b != "ModelSnapshot")
                {
                    contexts.push(DbContextInfo {
                        name: name.to_string(),
                        file: path.to_string(),
                        line: t.line,
                        base: b,
                        sets: Vec::new(),
                        project: None,
                    });
                    current = Some((contexts.len() - 1, depth + 1));
                }
                pending.clear();
                i = j;
                continue;
            }
        } else if t.is_ident("DbSet") && tokens.get(i + 1).is_some_and(|n| n.is('<')) {
            if let Some((ci, d)) = current {
                if depth == d {
                    let entity = tokens
                        .get(i + 2)
                        .and_then(|n| n.ident())
                        .unwrap_or("")
                        .to_string();
                    let close = crate::csharp::matching(&tokens, i + 1);
                    let name = tokens[close..]
                        .iter()
                        .find(|n| n.ident().is_some())
                        .and_then(|n| n.ident())
                        .unwrap_or("")
                        .to_string();
                    if !name.is_empty() {
                        contexts[ci].sets.push(DbSetInfo { name, entity });
                    }
                    i = close;
                    continue;
                }
            }
        }
        i += 1;
    }
    (contexts, migrations)
}

/// Whether `.config/dotnet-tools.json` installs `dotnet-ef`.
pub fn has_local_tool(manifest: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(manifest)
        .ok()
        .and_then(|v| {
            v.get("tools")
                .and_then(|t| t.as_object())
                .map(|t| t.contains_key("dotnet-ef"))
        })
        .unwrap_or(false)
}

/// Merge the sources' migrations with a `migrations list` answer (applied state, and the
/// ones only the database or the compiled assembly know).
pub fn merge_migrations(sources: &[Migration], listed: &[Migration]) -> Vec<Migration> {
    let mut out: Vec<Migration> = sources.to_vec();
    for l in listed {
        match out.iter_mut().find(|m| m.id == l.id) {
            Some(m) => m.applied = l.applied,
            None => out.push(l.clone()),
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

// ---- dotnet ef -------------------------------------------------------------------------------

/// A `dotnet ef` action of the EF Core dialogs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EfRequest {
    /// `addMigration`, `removeMigration`, `listMigrations`, `updateDatabase`,
    /// `dropDatabase`, `script`, `scaffold`, `listContexts`, `installTool`.
    pub action: String,
    /// The project with the DbContext (`--project`).
    pub project: String,
    /// The startup project (`--startup-project`; empty: the same).
    pub startup_project: String,
    pub context: String,
    /// Migration name (add) or target migration (update, script).
    pub name: String,
    pub output_dir: String,
    /// `dbcontext scaffold`: connection string and provider.
    pub connection: String,
    pub provider: String,
    /// `dbcontext scaffold`: only these tables.
    pub tables: Vec<String>,
    pub force: bool,
    /// `dbcontext scaffold --data-annotations`.
    pub data_annotations: bool,
    pub configuration: String,
    pub framework: String,
    pub no_build: bool,
}

fn quoted_ok(s: &str) -> Result<String, String> {
    let s = s.trim();
    if s.is_empty() {
        Err("a value is required".into())
    } else {
        Ok(s.to_string())
    }
}

/// The `dotnet ef …` command line of a request. `list` actions print JSON (`--json`), so
/// their output is parsed by [`parse_migrations_list`].
pub fn ef_command(r: &EfRequest, ctx: &DotnetContext) -> Result<DotnetCommand, String> {
    let s = |x: &str| x.to_string();
    if r.action == "installTool" {
        // `dotnet tool install --global dotnet-ef` (the UI asks first).
        return Ok(DotnetCommand::new(
            ctx,
            vec![s("tool"), s("install"), s("--global"), s("dotnet-ef")],
        ));
    }
    let mut args = vec![s("ef")];
    match r.action.as_str() {
        "addMigration" => {
            let name = quoted_ok(&r.name).map_err(|_| "Enter a migration name".to_string())?;
            if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Err(format!(
                    "'{name}' is not a valid migration name (letters, digits and _)"
                ));
            }
            args.extend([s("migrations"), s("add"), name]);
            if !r.output_dir.trim().is_empty() {
                args.extend([s("--output-dir"), r.output_dir.trim().to_string()]);
            }
        }
        "removeMigration" => {
            args.extend([s("migrations"), s("remove")]);
            if r.force {
                args.push(s("--force"));
            }
        }
        "listMigrations" => args.extend([s("migrations"), s("list"), s("--json")]),
        "updateDatabase" => {
            args.extend([s("database"), s("update")]);
            if !r.name.trim().is_empty() {
                args.push(r.name.trim().to_string());
            }
        }
        "dropDatabase" => args.extend([s("database"), s("drop"), s("--force")]),
        "script" => {
            args.extend([s("migrations"), s("script"), s("--idempotent")]);
            if !r.name.trim().is_empty() {
                args.extend([s("0"), r.name.trim().to_string()]);
            }
        }
        "listContexts" => args.extend([s("dbcontext"), s("list"), s("--json")]),
        "scaffold" => {
            let conn =
                quoted_ok(&r.connection).map_err(|_| "Enter a connection string".to_string())?;
            let provider = quoted_ok(&r.provider).map_err(|_| "Choose a provider".to_string())?;
            args.extend([s("dbcontext"), s("scaffold"), conn, provider]);
            let out = if r.output_dir.trim().is_empty() {
                "Models"
            } else {
                r.output_dir.trim()
            };
            args.extend([s("--output-dir"), out.to_string()]);
            if !r.context.trim().is_empty() {
                args.extend([s("--context"), r.context.trim().to_string()]);
            }
            for t in r.tables.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
                args.extend([s("--table"), t.to_string()]);
            }
            if r.data_annotations {
                args.push(s("--data-annotations"));
            }
            if r.force {
                args.push(s("--force"));
            }
        }
        other => return Err(format!("unknown EF Core action '{other}'")),
    }
    if r.project.trim().is_empty() {
        return Err("Choose the project with the DbContext".into());
    }
    args.extend([s("--project"), r.project.trim().to_string()]);
    if !r.startup_project.trim().is_empty() && r.startup_project.trim() != r.project.trim() {
        args.extend([s("--startup-project"), r.startup_project.trim().to_string()]);
    }
    if !r.context.trim().is_empty() && r.action != "scaffold" && r.action != "listContexts" {
        args.extend([s("--context"), r.context.trim().to_string()]);
    }
    let configuration = if r.configuration.trim().is_empty() {
        ctx.configuration()
    } else {
        r.configuration.trim().to_string()
    };
    args.extend([s("--configuration"), configuration]);
    if !r.framework.trim().is_empty() {
        args.extend([s("--framework"), r.framework.trim().to_string()]);
    }
    if r.no_build {
        args.push(s("--no-build"));
    }
    if r.action.starts_with("list") {
        args.push(s("--prefix-output"));
    }
    Ok(DotnetCommand::new(ctx, args))
}

/// The answer of `dotnet ef migrations list --json --prefix-output` (or of the plain text
/// form): the migrations with their applied state, or the failure with a hint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EfOutput {
    pub migrations: Vec<Migration>,
    /// `dbcontext list`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contexts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// What to do about the error (install the tool, add the Design package…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// `dotnet-ef` is missing (the UI offers to install it).
    #[serde(default)]
    pub tool_missing: bool,
}

/// Parse the output of a `migrations list` or `dbcontext list` run.
pub fn parse_ef_output(output: &str) -> EfOutput {
    let mut out = EfOutput::default();
    // `--prefix-output` marks lines `data:`, `info:`, `error:`, `warn:`, `verbose:`.
    let mut data = String::new();
    let mut errors = Vec::new();
    let mut plain = Vec::new();
    for line in output.lines() {
        let l = line.trim_end();
        if let Some(d) = l.strip_prefix("data:") {
            data.push_str(d.trim_start_matches(' '));
            data.push('\n');
        } else if let Some(e) = l.strip_prefix("error:") {
            errors.push(e.trim().to_string());
        } else if l.starts_with("info:") || l.starts_with("warn:") || l.starts_with("verbose:") {
        } else {
            plain.push(l.to_string());
        }
    }
    let lower = output.to_ascii_lowercase();
    if lower.contains("could not execute because the specified command or file was not found")
        || lower.contains("no executable found matching command \"dotnet-ef\"")
        || lower.contains("'ef' is not a")
    {
        out.tool_missing = true;
        out.error = Some("The dotnet-ef tool is not installed".into());
        out.hint = Some("Install it with dotnet tool install --global dotnet-ef".into());
        return out;
    }
    if data.trim().is_empty() {
        // Without --prefix-output: the JSON array is the last bracketed block.
        let text = plain.join("\n");
        if let (Some(s), Some(e)) = (text.find('['), text.rfind(']')) {
            if s < e {
                data = text[s..=e].to_string();
            }
        }
    }
    if let Ok(serde_json::Value::Array(items)) =
        serde_json::from_str::<serde_json::Value>(data.trim())
    {
        for v in items {
            if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
                out.migrations.push(Migration {
                    id: id.to_string(),
                    name: v
                        .get("name")
                        .and_then(|x| x.as_str())
                        .unwrap_or_else(|| id.split_once('_').map(|(_, n)| n).unwrap_or(id))
                        .to_string(),
                    context: None,
                    file: None,
                    line: 0,
                    applied: v.get("applied").and_then(|x| x.as_bool()),
                    project: None,
                });
            } else if let Some(name) = v
                .get("fullName")
                .or_else(|| v.get("safeName"))
                .and_then(|x| x.as_str())
            {
                out.contexts.push(name.to_string());
            }
        }
    } else {
        // Plain `migrations list`: one id per line, `(Pending)` for those not applied.
        for l in &plain {
            let t = l.trim();
            let (id, pending) = match t.strip_suffix("(Pending)") {
                Some(x) => (x.trim(), true),
                None => (t, false),
            };
            let ts = id.split('_').next().unwrap_or("");
            if ts.len() == 14
                && ts.chars().all(|c| c.is_ascii_digit())
                && id.contains('_')
                && !id.contains(' ')
            {
                out.migrations.push(Migration {
                    id: id.to_string(),
                    name: id.split_once('_').map(|(_, n)| n).unwrap_or(id).to_string(),
                    context: None,
                    file: None,
                    line: 0,
                    applied: Some(!pending),
                    project: None,
                });
            }
        }
    }
    if !errors.is_empty()
        || (out.migrations.is_empty() && out.contexts.is_empty() && lower.contains("error"))
    {
        let message = errors
            .first()
            .cloned()
            .or_else(|| {
                plain
                    .iter()
                    .find(|l| l.to_ascii_lowercase().contains("error"))
                    .cloned()
            })
            .unwrap_or_else(|| "dotnet ef failed".into());
        out.hint = hint(&message);
        out.error = Some(message);
    }
    out
}

fn hint(message: &str) -> Option<String> {
    let m = message.to_ascii_lowercase();
    if m.contains("microsoft.entityframeworkcore.design") {
        Some("Add the Microsoft.EntityFrameworkCore.Design package to the startup project".into())
    } else if m.contains("unable to create") && m.contains("dbcontext") {
        Some("Choose a startup project that registers the DbContext, or add an IDesignTimeDbContextFactory".into())
    } else if m.contains("more than one dbcontext") {
        Some("Choose the DbContext".into())
    } else if m.contains("build failed") {
        Some("Fix the build errors (Build tool window) and try again".into())
    } else if m.contains("connect")
        || m.contains("network-related")
        || m.contains("password authentication")
    {
        Some("Check the connection string and that the database server runs".into())
    } else {
        None
    }
}

/// The default folder of new migrations of a project (`Migrations` next to the project).
pub fn migrations_dir(project: &str) -> String {
    let dir = crate::paths::parent(project);
    if dir.is_empty() {
        "Migrations".into()
    } else {
        format!("{dir}/Migrations")
    }
}

/// The DbContexts of a project, by project path (for the dialogs).
pub fn contexts_by_project(contexts: &[DbContextInfo]) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in contexts {
        out.entry(c.project.clone().unwrap_or_default())
            .or_default()
            .push(c.name.clone());
    }
    out
}

/// Whether a file name looks like a migration's designer or snapshot (not listed itself).
pub fn is_generated(path: &str) -> bool {
    let n = file_name(path);
    n.ends_with(".Designer.cs") || n.ends_with("ModelSnapshot.cs")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn scans_contexts_and_migrations() {
        let (c, m) = scan_source(
            "src/Data/BookstoreContext.cs",
            &fixture("efcore/BookstoreContext.cs"),
        );
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "BookstoreContext");
        assert_eq!(c[0].base, "DbContext");
        assert_eq!(
            c[0].sets
                .iter()
                .map(|s| (s.name.as_str(), s.entity.as_str()))
                .collect::<Vec<_>>(),
            [("Books", "Book"), ("Authors", "Author")]
        );
        assert!(m.is_empty());
        let (c, m) = scan_source(
            "src/Data/Migrations/20241001120000_InitialCreate.Designer.cs",
            &fixture("efcore/20241001120000_InitialCreate.Designer.cs"),
        );
        assert!(c.is_empty());
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].id, "20241001120000_InitialCreate");
        assert_eq!(m[0].name, "InitialCreate");
        assert_eq!(m[0].context.as_deref(), Some("BookstoreContext"));
        assert_eq!(m[0].timestamp().as_deref(), Some("2024-10-01 12:00:00"));
    }

    #[test]
    fn command_lines() {
        let ctx = DotnetContext::default();
        let r = EfRequest {
            action: "addMigration".into(),
            name: "AddReviews".into(),
            project: "src/Data/Data.csproj".into(),
            startup_project: "src/Api/Api.csproj".into(),
            context: "BookstoreContext".into(),
            ..EfRequest::default()
        };
        assert_eq!(
            ef_command(&r, &ctx).unwrap().display(),
            "dotnet ef migrations add AddReviews --project src/Data/Data.csproj --startup-project src/Api/Api.csproj --context BookstoreContext --configuration Debug"
        );
        assert!(ef_command(
            &EfRequest {
                name: "bad name".into(),
                ..r.clone()
            },
            &ctx
        )
        .is_err());
        let list = EfRequest {
            action: "listMigrations".into(),
            ..r.clone()
        };
        assert!(ef_command(&list, &ctx)
            .unwrap()
            .display()
            .ends_with("--prefix-output"));
        let scaffold = EfRequest {
            action: "scaffold".into(),
            connection: "Host=localhost;Database=shop".into(),
            provider: "Npgsql.EntityFrameworkCore.PostgreSQL".into(),
            tables: vec!["books".into()],
            force: true,
            ..r.clone()
        };
        let c = ef_command(&scaffold, &ctx).unwrap();
        assert_eq!(
            &c.args[..5],
            [
                "ef",
                "dbcontext",
                "scaffold",
                "Host=localhost;Database=shop",
                "Npgsql.EntityFrameworkCore.PostgreSQL"
            ]
        );
        assert!(c.args.windows(2).any(|w| w == ["--table", "books"]));
        let update = EfRequest {
            action: "updateDatabase".into(),
            name: "InitialCreate".into(),
            ..r
        };
        assert!(ef_command(&update, &ctx)
            .unwrap()
            .display()
            .starts_with("dotnet ef database update InitialCreate --project"));
    }

    #[test]
    fn parses_list_output() {
        let o = parse_ef_output(&fixture("efcore/migrations-list.txt"));
        assert_eq!(o.error, None);
        assert_eq!(
            o.migrations
                .iter()
                .map(|m| (m.name.as_str(), m.applied))
                .collect::<Vec<_>>(),
            [
                ("InitialCreate", Some(true)),
                ("AddAuthors", Some(true)),
                ("AddReviews", Some(false))
            ]
        );
        let plain = parse_ef_output("Build started...\nBuild succeeded.\n20241001120000_InitialCreate\n20241005093000_AddAuthors (Pending)\n");
        assert_eq!(plain.migrations[1].applied, Some(false));
        let missing = parse_ef_output(
            "Could not execute because the specified command or file was not found.\n",
        );
        assert!(missing.tool_missing);
        let design = parse_ef_output("Build succeeded.\nerror: Your startup project 'Api' doesn't reference Microsoft.EntityFrameworkCore.Design.\n");
        assert!(design.hint.unwrap().contains("Design package"));
        let contexts = parse_ef_output("data: [\ndata:   {\"fullName\": \"Bookstore.Data.BookstoreContext\", \"safeName\": \"BookstoreContext\", \"name\": \"BookstoreContext\"}\ndata: ]\n");
        assert_eq!(contexts.contexts, ["Bookstore.Data.BookstoreContext"]);
    }

    #[test]
    fn merges_and_tools() {
        let src = vec![Migration {
            id: "1_A".into(),
            name: "A".into(),
            context: None,
            file: Some("a.cs".into()),
            line: 3,
            applied: None,
            project: None,
        }];
        let listed = vec![
            Migration {
                applied: Some(true),
                file: None,
                ..src[0].clone()
            },
            Migration {
                id: "2_B".into(),
                name: "B".into(),
                context: None,
                file: None,
                line: 0,
                applied: Some(false),
                project: None,
            },
        ];
        let m = merge_migrations(&src, &listed);
        assert_eq!(m.len(), 2);
        assert_eq!(
            (m[0].file.as_deref(), m[0].applied),
            (Some("a.cs"), Some(true))
        );
        assert!(has_local_tool(
            r#"{"version":1,"isRoot":true,"tools":{"dotnet-ef":{"version":"8.0.11","commands":["dotnet-ef"]}}}"#
        ));
        assert!(!has_local_tool("{}"));
        assert_eq!(
            migrations_dir("src/Data/Data.csproj"),
            "src/Data/Migrations"
        );
    }
}
