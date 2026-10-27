//! The Database Explorer side of [`DbService`]: the model of each data source (cached in
//! memory and in the user cache dir, so the tree shows without connecting), the schema
//! filter applied when introspecting, incremental refresh of a schema or table, virtual
//! foreign keys and columns from the project, DDL data sources and Quick Documentation.

use std::path::{Path, PathBuf};

use forge_dbmodel::ddl::{self, DdlFile};
use forge_dbmodel::{
    DbModel, ModelWarning, ObjectDoc, ObjectRef, SchemaFilter, SchemaInfo, SchemaName, TableInfo,
    VirtualObjects, VIRTUAL_FILE,
};

use super::DbService;
use crate::config::{DataSource, Driver};
use crate::error::{DbError, DbResult};
use crate::introspect;

/// Bumped when the cached model's shape changes; older caches are ignored.
const CACHE_VERSION: u32 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheFile {
    version: u32,
    model: DbModel,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// FNV-1a, for stable cache file names.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Compare file names naturally, so `V2__x.sql` sorts before `V10__y.sql`.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut x, mut y) = (a.as_bytes(), b.as_bytes());
    loop {
        match (x.first(), y.first()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let n = x.iter().take_while(|c| c.is_ascii_digit()).count();
                let m = y.iter().take_while(|c| c.is_ascii_digit()).count();
                let (p, q) = (&x[..n], &y[..m]);
                let strip = |s: &[u8]| -> Vec<u8> {
                    let i = s.iter().position(|c| *c != b'0').unwrap_or(s.len());
                    s[i..].to_vec()
                };
                let (ps, qs) = (strip(p), strip(q));
                let ord = ps.len().cmp(&qs.len()).then_with(|| ps.cmp(&qs));
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
                x = &x[n..];
                y = &y[m..];
            }
            (Some(c), Some(d)) => {
                let ord = c.to_ascii_lowercase().cmp(&d.to_ascii_lowercase());
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
                x = &x[1..];
                y = &y[1..];
            }
        }
    }
}

fn walk_sql(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut list: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    list.sort_by(|a, b| {
        natural_cmp(
            &a.file_name().unwrap_or_default().to_string_lossy(),
            &b.file_name().unwrap_or_default().to_string_lossy(),
        )
    });
    for p in list {
        if p.is_dir() {
            walk_sql(&p, out);
        } else if p
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("sql") || e.eq_ignore_ascii_case("ddl"))
        {
            out.push(p);
        }
    }
}

impl DbService {
    /// Keep explorer models in `dir` (the user cache dir; never the project).
    pub fn with_cache_dir(mut self, dir: PathBuf) -> Self {
        self.cache_dir = Some(dir);
        self
    }

    fn cache_file(&self, id: &str) -> Option<PathBuf> {
        let dir = self.cache_dir.as_ref()?;
        let root = self
            .project_root()
            .map(|r| r.display().to_string())
            .unwrap_or_default();
        let safe: String = id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        Some(dir.join(format!(
            "{safe}-{:016x}.json",
            fnv1a(&format!("{root}\0{id}"))
        )))
    }

    fn cached_model(&self, id: &str) -> Option<DbModel> {
        if let Some(m) = self.models.lock().expect("poisoned").get(id) {
            return Some(m.clone());
        }
        let text = std::fs::read_to_string(self.cache_file(id)?).ok()?;
        let file: CacheFile = serde_json::from_str(&text).ok()?;
        (file.version == CACHE_VERSION).then(|| {
            self.models
                .lock()
                .expect("poisoned")
                .insert(id.into(), file.model.clone());
            file.model
        })
    }

    fn store_model(&self, id: &str, model: &DbModel) {
        self.models
            .lock()
            .expect("poisoned")
            .insert(id.into(), model.clone());
        let Some(path) = self.cache_file(id) else {
            return;
        };
        let file = CacheFile {
            version: CACHE_VERSION,
            model: model.clone(),
        };
        let written = path
            .parent()
            .map(std::fs::create_dir_all)
            .transpose()
            .and_then(|_| std::fs::write(&path, serde_json::to_vec(&file).unwrap_or_default()));
        if let Err(e) = written {
            tracing::warn!("cannot cache the model of {id}: {e}");
        }
    }

    /// Forget the cached model (data source deleted or changed).
    pub(super) fn forget_model(&self, id: &str) {
        self.models.lock().expect("poisoned").remove(id);
        if let Some(p) = self.cache_file(id) {
            let _ = std::fs::remove_file(p);
        }
    }

    fn schema_filter(ds: &DataSource) -> DbResult<SchemaFilter> {
        SchemaFilter::parse(&ds.options.schemas)
            .map_err(|e| DbError::Config(format!("{}: schema filter: {e}", ds.name)))
    }

    /// Apply the data source's current filter and the project's virtual objects.
    fn present(&self, ds: &DataSource, mut model: DbModel) -> DbResult<DbModel> {
        Self::schema_filter(ds)?.apply(&mut model.schemas);
        model.retain_visible();
        let virtuals = self.virtual_objects(&ds.id).unwrap_or_default();
        for s in model.loaded.values_mut() {
            virtuals.apply(&ds.id, s);
        }
        Ok(model)
    }

    /// The model when it is known without connecting (cached, or a DDL data source): what
    /// the SQL editor resolves names against while typing.
    pub fn model_if_cached(&self, id: &str) -> DbResult<Option<DbModel>> {
        let ds = self.get(id)?;
        if ds.driver == Driver::Ddl {
            let model = self.ddl_model(&ds, false)?;
            return self.present(&ds, model).map(Some);
        }
        match self.cached_model(id) {
            Some(m) => self.present(&ds, m).map(Some),
            None => Ok(None),
        }
    }

    /// The explorer model of a data source. Without `refresh` it is the cached one (no
    /// connection needed); refreshing reloads the schema list and server objects and
    /// re-introspects the schemas that were loaded and are still visible.
    pub async fn model(&self, id: &str, refresh: bool) -> DbResult<DbModel> {
        let ds = self.get(id)?;
        if ds.driver == Driver::Ddl {
            let model = self.ddl_model(&ds, refresh)?;
            return self.present(&ds, model);
        }
        if !refresh {
            if let Some(m) = self.cached_model(id) {
                return self.present(&ds, m);
            }
        }
        let previous = self.cached_model(id);
        let pool = self.pool(id).await?;
        let mut conn = pool.acquire().await?;
        let mut model = DbModel::new(ds.dialect());
        model.schemas = introspect::schemas(&mut conn).await?;
        Self::schema_filter(&ds)?.apply(&mut model.schemas);
        if ds.dialect().is_sql() {
            let server = introspect::server_objects(&mut conn).await;
            model.catalog = server.catalog;
            model.catalogs = server.catalogs;
            model.roles = server.roles;
        }
        let reload: Vec<String> = previous
            .map(|p| p.loaded.into_keys().collect::<Vec<String>>())
            .unwrap_or_default()
            .into_iter()
            .filter(|n| model.is_visible(n))
            .collect();
        for name in reload {
            match introspect::schema(&mut conn, &name, None).await {
                Ok(info) => {
                    model.loaded.insert(name, info);
                }
                Err(e) => model.warnings.push(ModelWarning {
                    file: String::new(),
                    line: 0,
                    message: format!("{name}: {e}"),
                }),
            }
        }
        model.refreshed_at = now_ms();
        self.store_model(id, &model);
        self.present(&ds, model)
    }

    /// Schemas of a data source with their filter result (connects on first use).
    pub async fn schemas(&self, id: &str) -> DbResult<Vec<SchemaName>> {
        Ok(self.model(id, false).await?.schemas)
    }

    /// Introspect one schema (refused when the schema filter hides it) and keep it in the
    /// cached model.
    pub async fn introspect(&self, id: &str, schema: &str) -> DbResult<SchemaInfo> {
        let ds = self.get(id)?;
        let mut model = self.model(id, false).await?;
        if ds.driver == Driver::Ddl {
            return model
                .loaded
                .remove(schema)
                .ok_or_else(|| DbError::Sql(format!("schema {schema} not found")));
        }
        if !model.schemas.iter().any(|s| s.name == schema) {
            // A schema created after the last refresh.
            model = self.model(id, true).await?;
        }
        if !model.is_visible(schema) {
            return Err(DbError::Refused(format!(
                "schema {schema} is hidden by the schema filter of {}",
                ds.name
            )));
        }
        let mut conn = self.pool(id).await?.acquire().await?;
        let info = introspect::schema(&mut conn, schema, None).await?;
        let mut cached = self.cached_model(id).unwrap_or(model);
        cached.loaded.insert(schema.into(), info.clone());
        self.store_model(id, &cached);
        let mut info = info;
        if let Ok(v) = self.virtual_objects(id) {
            v.apply(id, &mut info);
        }
        Ok(info)
    }

    /// Introspect one table again (its node's "Refresh") and merge it into the cache.
    pub async fn table_info(&self, id: &str, schema: &str, table: &str) -> DbResult<TableInfo> {
        let ds = self.get(id)?;
        if ds.driver == Driver::Ddl {
            let model = self.model(id, true).await?;
            return model
                .table(schema, table)
                .cloned()
                .ok_or_else(|| DbError::Sql(format!("table {schema}.{table} not found")));
        }
        let mut conn = self.pool(id).await?.acquire().await?;
        let info = introspect::schema(&mut conn, schema, Some(table)).await?;
        let mut found = info
            .tables
            .into_iter()
            .next()
            .ok_or_else(|| DbError::Sql(format!("table {schema}.{table} not found")))?;
        if let Some(mut m) = self.cached_model(id) {
            if let Some(s) = m.loaded.get_mut(schema) {
                s.merge_table(found.clone());
                self.store_model(id, &m);
            }
        }
        let mut one = SchemaInfo {
            name: schema.into(),
            tables: vec![found.clone()],
            ..Default::default()
        };
        if let Ok(v) = self.virtual_objects(id) {
            v.apply(id, &mut one);
            found = one.tables.remove(0);
        }
        Ok(found)
    }

    // ---- virtual foreign keys and columns ---------------------------------------------

    fn read_virtual_file(&self) -> DbResult<VirtualObjects> {
        let text = if let Some(files) = self.project_files() {
            files
                .read(VIRTUAL_FILE)
                .map_err(|e| DbError::Io(e.to_string()))?
        } else if let Some(root) = self.project_root() {
            match std::fs::read_to_string(root.join(VIRTUAL_FILE)) {
                Ok(t) => Some(t),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(DbError::Io(e.to_string())),
            }
        } else {
            None
        };
        match text {
            Some(t) => VirtualObjects::parse(&t)
                .map_err(|e| DbError::Config(format!("{VIRTUAL_FILE}: {e}"))),
            None => Ok(VirtualObjects::default()),
        }
    }

    /// Virtual foreign keys and columns of a data source.
    pub fn virtual_objects(&self, id: &str) -> DbResult<VirtualObjects> {
        Ok(self.read_virtual_file()?.of(id))
    }

    /// Replace the virtual objects of a data source in the project file.
    pub fn save_virtual_objects(&self, id: &str, objects: VirtualObjects) -> DbResult<()> {
        self.get(id)?;
        objects.validate().map_err(DbError::Config)?;
        let mut all = self.read_virtual_file()?;
        all.replace(id, objects);
        let text = all.to_text().map_err(DbError::Config)?;
        if let Some(files) = self.project_files() {
            return files
                .write(VIRTUAL_FILE, &text)
                .map_err(|e| DbError::Io(e.to_string()));
        }
        let root = self.project_root().ok_or_else(|| {
            DbError::Config("open a project to store virtual foreign keys".into())
        })?;
        let path = root.join(VIRTUAL_FILE);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| DbError::Io(e.to_string()))?;
        }
        std::fs::write(&path, text).map_err(|e| DbError::Io(format!("{}: {e}", path.display())))
    }

    // ---- DDL data sources -------------------------------------------------------------

    /// The files of a DDL data source in parse order (folders: their `.sql` files, sorted
    /// naturally, so migrations apply in order).
    pub fn ddl_files(&self, ds: &DataSource) -> DbResult<Vec<DdlFile>> {
        let mut out = Vec::new();
        let root = self.project_root();
        let remote = self.project_files();
        for entry in &ds.options.ddl_files {
            let rel = entry.trim().trim_start_matches("./");
            if rel.is_empty() {
                continue;
            }
            if let Some(files) = &remote {
                // Remote projects: files only (no listing through the backend).
                if let Some(text) = files.read(rel).map_err(|e| DbError::Io(e.to_string()))? {
                    out.push(DdlFile {
                        path: rel.into(),
                        text,
                    });
                }
                continue;
            }
            let path = match &root {
                Some(r) if Path::new(rel).is_relative() => r.join(rel),
                _ => PathBuf::from(rel),
            };
            let mut paths = Vec::new();
            if path.is_dir() {
                walk_sql(&path, &mut paths);
            } else {
                paths.push(path);
            }
            for p in paths {
                let text = std::fs::read_to_string(&p)
                    .map_err(|e| DbError::Io(format!("{}: {e}", p.display())))?;
                let shown = root
                    .as_ref()
                    .and_then(|r| p.strip_prefix(r).ok())
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push(DdlFile { path: shown, text });
            }
        }
        Ok(out)
    }

    fn ddl_model(&self, ds: &DataSource, refresh: bool) -> DbResult<DbModel> {
        if !refresh {
            if let Some(m) = self.models.lock().expect("poisoned").get(&ds.id) {
                return Ok(m.clone());
            }
        }
        let d = ds.dialect();
        let parsed = ddl::parse(d, &ds.database, &self.ddl_files(ds)?);
        let mut model = DbModel::new(d);
        let default = if ds.database.is_empty() {
            ddl::default_schema(d).to_string()
        } else {
            ds.database.clone()
        };
        model.schemas = parsed
            .schemas
            .iter()
            .map(|s| SchemaName::new(s.name.clone(), s.name == default))
            .collect();
        model.loaded = parsed
            .schemas
            .into_iter()
            .map(|s| (s.name.clone(), s))
            .collect();
        model.warnings = parsed.warnings;
        model.refreshed_at = now_ms();
        // Parsing is cheap: DDL models stay in memory only.
        self.models
            .lock()
            .expect("poisoned")
            .insert(ds.id.clone(), model.clone());
        Ok(model)
    }

    /// DDL data sources whose files include one of the changed project-relative paths;
    /// their models are dropped so the next request parses the files again.
    pub fn ddl_sources_touched(&self, changed: &[String]) -> Vec<String> {
        let list = self.lock().list.clone();
        let mut out = Vec::new();
        for ds in list.iter().filter(|d| d.driver == Driver::Ddl) {
            let hit = ds.options.ddl_files.iter().any(|f| {
                let f = f.trim().trim_start_matches("./").trim_end_matches('/');
                changed
                    .iter()
                    .any(|c| c == f || c.starts_with(&format!("{f}/")))
            });
            if hit {
                self.models.lock().expect("poisoned").remove(&ds.id);
                out.push(ds.id.clone());
            }
        }
        out
    }

    // ---- Quick Documentation ----------------------------------------------------------

    /// Quick Documentation of a tree node (from the model; no connection for cached
    /// schemas).
    pub async fn object_doc(&self, id: &str, schema: &str, obj: &ObjectRef) -> DbResult<ObjectDoc> {
        let ds = self.get(id)?;
        let model = self.model(id, false).await?;
        if obj.kind == "role" {
            return model
                .roles
                .iter()
                .find(|r| r.name == obj.name)
                .map(forge_dbmodel::doc::document_role)
                .ok_or_else(|| DbError::Sql(format!("role {} not found", obj.name)));
        }
        let info = match model.loaded.get(schema) {
            Some(s) => s.clone(),
            None => self.introspect(id, schema).await?,
        };
        forge_dbmodel::document(ds.dialect(), &info, obj)
            .ok_or_else(|| DbError::Sql(format!("{} {} not found", obj.kind, obj.name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order_of_migrations() {
        let mut v = vec!["V10__c.sql", "V2__b.sql", "v1__a.sql", "2024_01_02_x.sql"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            v,
            ["2024_01_02_x.sql", "v1__a.sql", "V2__b.sql", "V10__c.sql"]
        );
    }
}
