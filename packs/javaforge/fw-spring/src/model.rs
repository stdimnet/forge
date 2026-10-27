//! The Spring Boot / Jakarta EE model of a project, from its Java and Kotlin sources and
//! configuration files: beans (stereotypes, `@Bean` methods, Spring Data repositories, CDI
//! beans and producers) and their injection points (constructors, `@Autowired` / `@Inject`
//! fields and setters, `@Bean` method parameters) with the beans that match them, Spring MVC
//! and JAX-RS endpoints, JPA entities, `@ConfigurationProperties` classes (turned into
//! configuration metadata), the APIs in use (`jakarta.*` / `javax.*`), profiles and the
//! server settings of the configuration files. Also the gutter markers of a file.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::config::{self, canonical, Format};
use crate::metadata::{Metadata, Property};
use crate::scan::{
    self, find, has, Annotation, Callable, ClassInfo, ClassKind, Lang, SourceFile, TypeRef, Val,
};
use crate::Location;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bean {
    pub name: String,
    /// The bean's type: the class (qualified) or a `@Bean` method's return type as written.
    #[serde(rename = "type")]
    pub type_name: String,
    /// `component`, `service`, `repository`, `controller`, `restController`,
    /// `configuration`, `application`, `bean`, `dataRepository`, `cdi`, `producer`, `ejb`,
    /// `resource`.
    pub kind: String,
    /// `spring` or `cdi`.
    pub framework: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub primary: bool,
    /// `@Qualifier("…")` / `@Named("…")` on the bean.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<String>,
    pub location: Location,
    /// `@Bean` / `@Produces` methods: the declaring class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_in: Option<String>,
    /// Indices of the injection points it is a candidate for.
    #[serde(default)]
    pub injected_into: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Injection {
    /// The class that receives the dependency.
    pub owner: String,
    /// The field or parameter name.
    pub member: String,
    /// `field`, `constructor`, `setter`, `beanMethod` or `producer`.
    pub kind: String,
    /// As written (`List<PaymentProvider>`).
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<String>,
    /// A collection / map / array of beans.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub many: bool,
    pub location: Location,
    /// Indices of the matching beans (primary ones first).
    pub candidates: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointParam {
    pub name: String,
    /// `path`, `query`, `header` or `body`.
    pub kind: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    /// `GET`, `POST`, … or `ANY`.
    pub method: String,
    pub path: String,
    /// `OrderController.find`.
    pub handler: String,
    pub class: String,
    /// `spring` or `jaxrs`.
    pub framework: String,
    pub location: Location,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub produces: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<EndpointParam>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityField {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub id: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub generated: bool,
    /// `manyToOne`, `oneToMany`, `manyToMany`, `oneToOne`, `embedded`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    /// The related entity (simple name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    /// The JPQL name (`@Entity(name = …)` or the simple class name).
    pub name: String,
    pub class: String,
    /// `entity`, `embeddable` or `mappedSuperclass`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    pub location: Location,
    pub fields: Vec<EntityField>,
    /// Spring Data repositories of the entity (qualified names).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repositories: Vec<String>,
    /// `jakarta` or `javax`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigClass {
    pub prefix: String,
    pub class: String,
    pub location: Location,
    /// The keys it binds.
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Application {
    pub class: String,
    pub name: String,
    /// The class to run: the class, or the Kotlin file facade (`ShopApplicationKt`) of a
    /// Kotlin application with a top-level `main`.
    pub main_class: String,
    /// Declared in a Kotlin file.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub kotlin: bool,
    pub location: Location,
    /// `spring` (`@SpringBootApplication`) or `jaxrs` (`@ApplicationPath`).
    pub kind: String,
}

/// A Java EE / Jakarta EE or Spring API the sources use.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Api {
    pub id: String,
    pub name: String,
    /// `jakarta`, `javax` or `spring`.
    pub namespace: String,
    /// Files importing it.
    pub files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFileInfo {
    pub path: String,
    pub format: Format,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub keys: usize,
}

/// A configuration value with where it applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigValue {
    /// Canonical key.
    pub key: String,
    pub value: String,
    /// The profile of the file name or of the document's `spring.config.activate.on-profile`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub location: Location,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub beans: Vec<Bean>,
    pub injections: Vec<Injection>,
    pub endpoints: Vec<Endpoint>,
    pub entities: Vec<Entity>,
    pub config_classes: Vec<ConfigClass>,
    pub applications: Vec<Application>,
    pub apis: Vec<Api>,
    pub profiles: Vec<String>,
    pub config_files: Vec<ConfigFileInfo>,
    pub config_values: Vec<ConfigValue>,
    /// Enum constants of the project's enums (qualified name → constants).
    pub enums: BTreeMap<String, Vec<String>>,
    /// Project classes (qualified name → declaration).
    pub classes: BTreeMap<String, Location>,
    /// Metadata of the project: `@ConfigurationProperties` classes and the project's own
    /// `META-INF/(additional-)spring-configuration-metadata.json`.
    pub metadata: Metadata,
    /// Uses Spring (any stereotype, Spring Boot application or Spring import).
    pub spring: bool,
    /// Uses Jakarta EE / Java EE APIs.
    pub jakarta: bool,
}

/// Spring stereotypes and the bean kind they declare.
const STEREOTYPES: &[(&str, &str)] = &[
    ("Component", "component"),
    ("Service", "service"),
    ("Repository", "repository"),
    ("Controller", "controller"),
    ("RestController", "restController"),
    ("Configuration", "configuration"),
    ("SpringBootApplication", "application"),
    ("ControllerAdvice", "component"),
    ("RestControllerAdvice", "component"),
    ("SpringBootConfiguration", "configuration"),
    ("AutoConfiguration", "configuration"),
];

/// CDI / EJB bean-defining annotations and their scope.
const CDI: &[(&str, &str, &str)] = &[
    ("ApplicationScoped", "cdi", "application"),
    ("RequestScoped", "cdi", "request"),
    ("SessionScoped", "cdi", "session"),
    ("ConversationScoped", "cdi", "conversation"),
    ("Dependent", "cdi", "dependent"),
    ("Singleton", "cdi", "singleton"),
    ("Stateless", "ejb", "stateless"),
    ("Stateful", "ejb", "stateful"),
];

const MAPPINGS: &[(&str, &str)] = &[
    ("GetMapping", "GET"),
    ("PostMapping", "POST"),
    ("PutMapping", "PUT"),
    ("DeleteMapping", "DELETE"),
    ("PatchMapping", "PATCH"),
    ("RequestMapping", ""),
];

const JAXRS_METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

const SCALARS: &[&str] = &[
    "String",
    "CharSequence",
    "int",
    "Int",
    "Integer",
    "long",
    "Long",
    "short",
    "Short",
    "byte",
    "Byte",
    "boolean",
    "Boolean",
    "double",
    "Double",
    "float",
    "Float",
    "char",
    "Char",
    "Character",
    "BigDecimal",
    "BigInteger",
    "Duration",
    "Object",
    "Any",
    "Number",
    "UUID",
    "Instant",
    "LocalDate",
    "LocalDateTime",
    "DataSize",
    "Class",
    "Charset",
    "Locale",
    "Pattern",
    "URI",
    "URL",
    "Path",
    "File",
    "Resource",
];

/// Wrappers that hold one bean, and collections that hold many.
const SINGLE_WRAPPERS: &[&str] = &[
    "Optional",
    "ObjectProvider",
    "ObjectFactory",
    "Provider",
    "Instance",
    "Lazy",
    "Supplier",
];
const MANY_WRAPPERS: &[&str] = &[
    "List",
    "Set",
    "Collection",
    "Iterable",
    "Stream",
    "SortedSet",
    "MutableList",
];

/// APIs recognized from imports: (package prefix, id, name, namespace).
const APIS: &[(&str, &str, &str, &str)] = &[
    (
        "jakarta.ws.rs",
        "jaxrs",
        "Jakarta RESTful Web Services (JAX-RS)",
        "jakarta",
    ),
    (
        "javax.ws.rs",
        "jaxrs",
        "Java API for RESTful Web Services (JAX-RS)",
        "javax",
    ),
    (
        "jakarta.persistence",
        "jpa",
        "Jakarta Persistence (JPA)",
        "jakarta",
    ),
    (
        "javax.persistence",
        "jpa",
        "Java Persistence API (JPA)",
        "javax",
    ),
    (
        "jakarta.enterprise",
        "cdi",
        "Jakarta Contexts and Dependency Injection (CDI)",
        "jakarta",
    ),
    (
        "javax.enterprise",
        "cdi",
        "Contexts and Dependency Injection (CDI)",
        "javax",
    ),
    (
        "jakarta.inject",
        "inject",
        "Jakarta Dependency Injection",
        "jakarta",
    ),
    (
        "javax.inject",
        "inject",
        "Dependency Injection for Java (JSR-330)",
        "javax",
    ),
    (
        "jakarta.ejb",
        "ejb",
        "Jakarta Enterprise Beans (EJB)",
        "jakarta",
    ),
    ("javax.ejb", "ejb", "Enterprise JavaBeans (EJB)", "javax"),
    (
        "jakarta.validation",
        "validation",
        "Jakarta Bean Validation",
        "jakarta",
    ),
    ("javax.validation", "validation", "Bean Validation", "javax"),
    ("jakarta.servlet", "servlet", "Jakarta Servlet", "jakarta"),
    ("javax.servlet", "servlet", "Java Servlet", "javax"),
    (
        "jakarta.transaction",
        "jta",
        "Jakarta Transactions",
        "jakarta",
    ),
    ("javax.transaction", "jta", "Java Transaction API", "javax"),
    (
        "jakarta.json",
        "json",
        "Jakarta JSON Processing / Binding",
        "jakarta",
    ),
    (
        "org.springframework.web.bind",
        "spring-mvc",
        "Spring Web MVC",
        "spring",
    ),
    (
        "org.springframework.web.reactive",
        "spring-webflux",
        "Spring WebFlux",
        "spring",
    ),
    (
        "org.springframework.data.jpa",
        "spring-data-jpa",
        "Spring Data JPA",
        "spring",
    ),
    (
        "org.springframework.data",
        "spring-data",
        "Spring Data",
        "spring",
    ),
    (
        "org.springframework.boot",
        "spring-boot",
        "Spring Boot",
        "spring",
    ),
];

/// Java Introspector's decapitalize: `OrderService` → `orderService`, `URLMapper` stays.
pub fn decapitalize(name: &str) -> String {
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(a), Some(b)) if a.is_uppercase() && b.is_uppercase() => name.to_string(),
        (Some(a), _) => a.to_lowercase().chain(name.chars().skip(1)).collect(),
        _ => String::new(),
    }
}

fn media_type(v: &Val) -> String {
    match v {
        Val::Str(s) => s.clone(),
        Val::Expr(_) => {
            let c = v.simple().trim_end_matches("_VALUE");
            match c {
                "APPLICATION_JSON" => "application/json".into(),
                "APPLICATION_XML" => "application/xml".into(),
                "TEXT_PLAIN" => "text/plain".into(),
                "TEXT_HTML" => "text/html".into(),
                "TEXT_XML" => "text/xml".into(),
                "MULTIPART_FORM_DATA" => "multipart/form-data".into(),
                "APPLICATION_FORM_URLENCODED" => "application/x-www-form-urlencoded".into(),
                "APPLICATION_OCTET_STREAM" => "application/octet-stream".into(),
                "TEXT_EVENT_STREAM" | "SERVER_SENT_EVENTS" => "text/event-stream".into(),
                "APPLICATION_NDJSON" => "application/x-ndjson".into(),
                other => other.to_ascii_lowercase().replace('_', "/"),
            }
        }
    }
}

/// Join path segments: `/api/` + `orders/{id}` → `/api/orders/{id}`.
pub fn join_paths(parts: &[&str]) -> String {
    let mut out = String::new();
    for p in parts {
        let p = p.trim();
        if p.is_empty() || p == "/" {
            continue;
        }
        if !out.ends_with('/') && !p.starts_with('/') {
            out.push('/');
        }
        if out.ends_with('/') && p.starts_with('/') {
            out.pop();
        }
        out.push_str(p);
    }
    while out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    if !out.starts_with('/') {
        out.insert(0, '/');
    }
    out
}

/// Replace Kotlin `$NAME` / `${NAME}` templates with constants.
fn kotlin_templates(s: &str, consts: &BTreeMap<String, String>) -> String {
    if !s.contains('$') {
        return s.to_string();
    }
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' {
            let (name, next) = if chars.get(i + 1) == Some(&'{') {
                let end = chars[i + 2..]
                    .iter()
                    .position(|c| *c == '}')
                    .map(|p| i + 2 + p);
                match end {
                    Some(e) => (chars[i + 2..e].iter().collect::<String>(), e + 1),
                    None => (String::new(), i + 1),
                }
            } else {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                (chars[i + 1..j].iter().collect::<String>(), j)
            };
            let simple = name.rsplit('.').next().unwrap_or(&name);
            if let Some(v) = consts.get(simple) {
                out.push_str(v);
                i = next;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

struct Index<'a> {
    files: &'a [SourceFile],
    /// Simple class name → (file, class) indices.
    by_simple: HashMap<&'a str, Vec<(usize, usize)>>,
    /// Every string constant of the project.
    consts: BTreeMap<String, String>,
    /// Project annotation types that are (meta-annotated) Spring stereotypes.
    custom_stereotypes: HashMap<String, &'static str>,
}

impl<'a> Index<'a> {
    fn new(files: &'a [SourceFile]) -> Self {
        let mut by_simple: HashMap<&str, Vec<(usize, usize)>> = HashMap::new();
        let mut consts = BTreeMap::new();
        for (fi, f) in files.iter().enumerate() {
            for (ci, c) in f.classes.iter().enumerate() {
                by_simple.entry(c.name.as_str()).or_default().push((fi, ci));
            }
            for (k, v) in &f.constants {
                consts.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        // Constants made of other files' constants.
        let exprs: BTreeMap<String, String> = files
            .iter()
            .flat_map(|f| f.constant_exprs.iter().map(|(k, v)| (k.clone(), v.clone())))
            .collect();
        scan::resolve_constants(&mut consts, &exprs);
        let mut custom_stereotypes = HashMap::new();
        for f in files {
            for c in f.classes.iter().filter(|c| c.kind == ClassKind::Annotation) {
                if let Some((_, kind)) = STEREOTYPES.iter().find(|(s, _)| has(&c.annotations, s)) {
                    custom_stereotypes.insert(c.name.clone(), *kind);
                }
            }
        }
        Self {
            files,
            by_simple,
            consts,
            custom_stereotypes,
        }
    }

    fn class(&self, simple: &str) -> Option<&'a ClassInfo> {
        let (fi, ci) = *self.by_simple.get(simple)?.first()?;
        Some(&self.files[fi].classes[ci])
    }

    /// Simple names of a class's supertypes, transitively.
    fn supers(&self, simple: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut todo = vec![simple.to_string()];
        while let Some(s) = todo.pop() {
            for &(fi, ci) in self.by_simple.get(s.as_str()).into_iter().flatten() {
                for t in &self.files[fi].classes[ci].supertypes {
                    let n = t.simple().to_string();
                    if out.insert(n.clone()) {
                        todo.push(n);
                    }
                }
            }
        }
        out
    }

    fn resolve(&self, v: &Val, file: &SourceFile) -> Option<String> {
        let s = v
            .resolve(&file.constants)
            .or_else(|| v.resolve(&self.consts))?;
        Some(if file.lang == Lang::Kotlin {
            kotlin_templates(&kotlin_templates(&s, &file.constants), &self.consts)
        } else {
            s
        })
    }

    fn strings(&self, a: &Annotation, names: &[&str], file: &SourceFile) -> Vec<String> {
        a.first_arg(names)
            .map(|vals| vals.iter().filter_map(|v| self.resolve(v, file)).collect())
            .unwrap_or_default()
    }

    fn qualify(&self, simple: &str) -> String {
        self.class(simple)
            .map(|c| c.qualified.clone())
            .unwrap_or_else(|| simple.to_string())
    }
}

fn location(file: &SourceFile, line: u32, column: u32) -> Location {
    Location {
        path: file.path.clone(),
        line,
        column,
    }
}

/// The bean type an injection point asks for: (simple name, many).
pub fn injected_type(type_text: &str) -> (String, bool) {
    let mut t = TypeRef::parse(type_text);
    let mut many = type_text.trim_end_matches('?').ends_with("[]");
    for _ in 0..4 {
        let simple = t.simple().to_string();
        if SINGLE_WRAPPERS.contains(&simple.as_str()) && !t.args.is_empty() {
            t = TypeRef::parse(&t.args[0]);
        } else if MANY_WRAPPERS.contains(&simple.as_str()) && !t.args.is_empty() {
            many = true;
            t = TypeRef::parse(&t.args[0]);
        } else if simple.ends_with("Map") && t.args.len() == 2 {
            many = true;
            t = TypeRef::parse(&t.args[1]);
        } else {
            break;
        }
    }
    (t.simple().to_string(), many)
}

fn qualifier_of(list: &[Annotation]) -> Option<String> {
    find(list, "Qualifier")
        .or_else(|| find(list, "Named"))
        .and_then(|a| a.string(&["value"]))
}

fn profiles_of(list: &[Annotation]) -> Vec<String> {
    let Some(a) = find(list, "Profile") else {
        return Vec::new();
    };
    a.first_arg(&["value"])
        .unwrap_or_default()
        .iter()
        .filter_map(Val::as_str)
        .flat_map(|s| {
            s.split(['&', '|', '!', '(', ')', ' '])
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Analyze a project: `files` are (path, text) pairs of its sources (`.java`, `.kt`),
/// configuration files (`application*.properties|yml|yaml`) and its own
/// `META-INF/*spring-configuration-metadata.json`.
pub fn analyze(files: &[(String, String)]) -> Model {
    let mut project = Project::default();
    for (path, text) in files {
        project.set_file(path, text);
    }
    project.model()
}

/// Whether a project file is read by the analysis.
pub fn is_relevant(path: &str) -> bool {
    Lang::of_path(path).is_some() || is_metadata_file(path) || config::config_file(path).is_some()
}

fn is_metadata_file(path: &str) -> bool {
    path.ends_with("spring-configuration-metadata.json") && path.contains("META-INF")
}

/// The scanned files of a project, so an edited file is scanned again on its own.
#[derive(Debug, Clone, Default)]
pub struct Project {
    sources: BTreeMap<String, SourceFile>,
    configs: BTreeMap<String, String>,
    metadata: BTreeMap<String, String>,
}

impl Project {
    /// Add or replace a file (ignored when the analysis does not read it).
    pub fn set_file(&mut self, path: &str, text: &str) {
        if Lang::of_path(path).is_some() {
            self.sources
                .insert(path.to_string(), scan::parse(path, text));
        } else if is_metadata_file(path) {
            self.metadata.insert(path.to_string(), text.to_string());
        } else if config::config_file(path).is_some() {
            self.configs.insert(path.to_string(), text.to_string());
        }
    }

    pub fn remove_file(&mut self, path: &str) {
        self.sources.remove(path);
        self.configs.remove(path);
        self.metadata.remove(path);
    }

    pub fn len(&self) -> usize {
        self.sources.len() + self.configs.len() + self.metadata.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The model of the files.
    pub fn model(&self) -> Model {
        let mut model = Model::default();
        for (path, text) in &self.configs {
            if let Some((format, profile)) = config::config_file(path) {
                read_config(&mut model, path, text, format, profile);
            }
        }
        let sources: Vec<SourceFile> = self.sources.values().cloned().collect();
        analyze_sources(&mut model, &sources);
        for (path, text) in &self.metadata {
            let name = path.rsplit('/').next().unwrap_or(path).to_string();
            let _ = model.metadata.add_json(text, &format!("project: {name}"));
        }
        model
    }
}

fn read_config(model: &mut Model, path: &str, text: &str, format: Format, profile: Option<String>) {
    let entries = config::parse(format, text);
    // Documents activated by `spring.config.activate.on-profile` (or the older
    // `spring.profiles` in a document).
    let mut doc_profiles: HashMap<usize, String> = HashMap::new();
    for e in &entries {
        let k = canonical(&e.key);
        if (k == "spring.config.activate.on-profile" || (k == "spring.profiles" && e.document > 0))
            && e.value.is_some()
        {
            doc_profiles.insert(e.document, e.value.clone().unwrap_or_default());
        }
    }
    let mut profiles: Vec<String> = profile.iter().cloned().collect();
    for e in &entries {
        let Some(value) = &e.value else { continue };
        let k = canonical(&e.key);
        if k.starts_with("spring.profiles.active")
            || k.starts_with("spring.profiles.include")
            || k == "spring.config.activate.on-profile"
            || k.starts_with("spring.profiles.group.")
        {
            profiles.extend(
                value
                    .split([',', '&', '|', '!', ' '])
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            );
        }
        if let Some(group) = k.strip_prefix("spring.profiles.group.") {
            profiles.push(group.to_string());
        }
        model.config_values.push(ConfigValue {
            key: k,
            value: value.clone(),
            profile: doc_profiles
                .get(&e.document)
                .cloned()
                .or_else(|| profile.clone()),
            location: Location {
                path: path.to_string(),
                line: e.line,
                column: 0,
            },
        });
    }
    for p in profiles {
        if !model.profiles.contains(&p) {
            model.profiles.push(p);
        }
    }
    model.config_files.push(ConfigFileInfo {
        path: path.to_string(),
        format,
        profile,
        keys: entries.iter().filter(|e| e.value.is_some()).count(),
    });
}

fn analyze_sources(model: &mut Model, files: &[SourceFile]) {
    let index = Index::new(files);
    // APIs.
    let mut apis: BTreeMap<(String, String), Api> = BTreeMap::new();
    for f in files {
        let mut seen = BTreeSet::new();
        for (prefix, id, name, ns) in APIS {
            if f.imports_from(prefix)
                || f.imports
                    .iter()
                    .any(|i| i.starts_with(&format!("{prefix}.")))
            {
                // `org.springframework.data.jpa` also matches `org.springframework.data`.
                if *id == "spring-data"
                    && f.imports_from("org.springframework.data.jpa")
                    && !f.imports.iter().any(|i| {
                        i.starts_with("org.springframework.data.")
                            && !i.starts_with("org.springframework.data.jpa")
                    })
                {
                    continue;
                }
                if seen.insert((*id, *ns)) {
                    apis.entry((id.to_string(), ns.to_string()))
                        .or_insert_with(|| Api {
                            id: id.to_string(),
                            name: name.to_string(),
                            namespace: ns.to_string(),
                            files: 0,
                        })
                        .files += 1;
                }
            }
        }
    }
    model.apis = apis.into_values().collect();
    model.jakarta = model.apis.iter().any(|a| a.namespace != "spring");
    model.spring = model.apis.iter().any(|a| a.namespace == "spring");

    for f in files {
        for c in &f.classes {
            model
                .classes
                .insert(c.qualified.clone(), location(f, c.line, c.column));
            if c.kind == ClassKind::Enum {
                model.enums.insert(c.qualified.clone(), c.constants.clone());
            }
        }
    }

    // JAX-RS application path.
    let mut app_path = String::new();
    for f in files {
        for c in &f.classes {
            if let Some(a) = c.annotation("ApplicationPath") {
                app_path = index
                    .strings(a, &["value"], f)
                    .into_iter()
                    .next()
                    .unwrap_or_default();
                model.applications.push(Application {
                    class: c.qualified.clone(),
                    name: c.name.clone(),
                    main_class: c.qualified.clone(),
                    kotlin: f.lang == Lang::Kotlin,
                    location: location(f, c.line, c.column),
                    kind: "jaxrs".into(),
                });
            }
            if c.annotation("SpringBootApplication").is_some() {
                let main_class = match f.lang {
                    // `fun main` next to the class compiles into the file facade.
                    Lang::Kotlin if !c.methods.iter().any(|m| m.name == "main") => {
                        let stem = f.path.rsplit(['/', '\\']).next().unwrap_or(&f.path);
                        let stem = stem.strip_suffix(".kt").unwrap_or(stem);
                        let mut facade: String =
                            stem.chars().take(1).flat_map(char::to_uppercase).collect();
                        facade.push_str(
                            &stem[stem.chars().next().map(char::len_utf8).unwrap_or(0)..],
                        );
                        match &f.package {
                            Some(p) if !p.is_empty() => format!("{p}.{facade}Kt"),
                            _ => format!("{facade}Kt"),
                        }
                    }
                    _ => c.qualified.clone(),
                };
                model.applications.push(Application {
                    class: c.qualified.clone(),
                    name: c.name.clone(),
                    main_class,
                    kotlin: f.lang == Lang::Kotlin,
                    location: location(f, c.line, c.column),
                    kind: "spring".into(),
                });
            }
        }
    }

    for f in files {
        for c in &f.classes {
            collect_beans(model, &index, f, c);
            collect_endpoints(model, &index, f, c, &app_path);
            collect_entity(model, &index, f, c);
            collect_config_class(model, &index, f, c);
        }
    }
    if model.beans.iter().any(|b| b.framework == "spring")
        || !model.applications.iter().all(|a| a.kind != "spring")
    {
        model.spring = true;
    }
    for f in files {
        for c in &f.classes {
            for p in profiles_of(&c.annotations)
                .into_iter()
                .chain(c.methods.iter().flat_map(|m| profiles_of(&m.annotations)))
            {
                if !model.profiles.contains(&p) {
                    model.profiles.push(p);
                }
            }
        }
    }
    model.profiles.sort();
    resolve_injections(model, &index);
    link_repositories(model, &index);
}

fn bean_kind(
    index: &Index,
    f: &SourceFile,
    c: &ClassInfo,
) -> Option<(String, String, Option<String>)> {
    if matches!(c.kind, ClassKind::Annotation | ClassKind::Enum) {
        return None;
    }
    if c.kind == ClassKind::Interface {
        // Spring Data repositories.
        if c.annotation("NoRepositoryBean").is_some() {
            return None;
        }
        let supers = index.supers(&c.name);
        let data = supers
            .iter()
            .any(|s| s.ends_with("Repository") && !s.is_empty())
            && (f
                .imports
                .iter()
                .any(|i| i.starts_with("org.springframework.data"))
                || supers.iter().any(|s| {
                    index.class(s).is_none()
                        && matches!(
                            s.as_str(),
                            "JpaRepository"
                                | "CrudRepository"
                                | "PagingAndSortingRepository"
                                | "ListCrudRepository"
                                | "Repository"
                                | "MongoRepository"
                                | "ReactiveCrudRepository"
                                | "CoroutineCrudRepository"
                                | "R2dbcRepository"
                        )
                }));
        return data.then(|| ("dataRepository".to_string(), "spring".to_string(), None));
    }
    if c.has_modifier("abstract") {
        return None;
    }
    for (s, kind) in STEREOTYPES {
        if c.annotation(s).is_some() {
            return Some((kind.to_string(), "spring".into(), None));
        }
    }
    for a in &c.annotations {
        if let Some(kind) = index.custom_stereotypes.get(a.simple()) {
            return Some((kind.to_string(), "spring".into(), None));
        }
    }
    // `@ConfigurationPropertiesScan` / `@EnableConfigurationProperties` register these.
    if c.annotation("ConfigurationProperties").is_some() {
        return Some(("configurationProperties".into(), "spring".into(), None));
    }
    for (s, kind, scope) in CDI {
        // `@Singleton` is also Spring-agnostic; only with a CDI / javax.inject import.
        if c.annotation(s).is_some() {
            return Some((kind.to_string(), "cdi".into(), Some(scope.to_string())));
        }
    }
    if c.annotation("Named").is_some() {
        return Some(("cdi".into(), "cdi".into(), None));
    }
    // JAX-RS resources are CDI beans (request scoped by default).
    if c.annotation("Path").is_some()
        && (f.imports_from("jakarta.ws.rs")
            || f.imports_from("javax.ws.rs")
            || f.imports.iter().any(|i| i.contains(".ws.rs.")))
    {
        return Some(("resource".into(), "cdi".into(), Some("request".into())));
    }
    None
}

fn collect_beans(model: &mut Model, index: &Index, f: &SourceFile, c: &ClassInfo) {
    let kind = bean_kind(index, f, c);
    if let Some((kind, framework, cdi_scope)) = &kind {
        let explicit = STEREOTYPES
            .iter()
            .filter_map(|(s, _)| c.annotation(s))
            .chain(
                c.annotations
                    .iter()
                    .filter(|a| index.custom_stereotypes.contains_key(a.simple())),
            )
            .chain(c.annotation("Named"))
            .find_map(|a| index.strings(a, &["value"], f).into_iter().next());
        let scope = cdi_scope.clone().or_else(|| {
            c.annotation("Scope").and_then(|a| {
                index
                    .strings(a, &["value", "scopeName"], f)
                    .into_iter()
                    .next()
            })
        });
        // Spring names scanned `@ConfigurationProperties` beans `<prefix>-<class>`.
        let properties_name = (kind == "configurationProperties")
            .then(|| c.annotation("ConfigurationProperties"))
            .flatten()
            .map(
                |a| match index.strings(a, &["prefix", "value"], f).into_iter().next() {
                    Some(p) if !p.is_empty() => format!("{p}-{}", c.qualified),
                    _ => c.qualified.clone(),
                },
            );
        model.beans.push(Bean {
            name: explicit
                .or(properties_name)
                .unwrap_or_else(|| decapitalize(&c.name)),
            type_name: c.qualified.clone(),
            kind: kind.clone(),
            framework: framework.clone(),
            scope,
            profiles: profiles_of(&c.annotations),
            primary: c.annotation("Primary").is_some() || c.annotation("Default").is_some(),
            qualifier: qualifier_of(&c.annotations).filter(|_| c.annotation("Qualifier").is_some()),
            location: location(f, c.line, c.column),
            declared_in: None,
            injected_into: Vec::new(),
        });
    }
    let spring_bean = kind.as_ref().is_some_and(|(_, fw, _)| fw == "spring");
    let cdi_bean = kind.as_ref().is_some_and(|(_, fw, _)| fw == "cdi");
    // `@Bean` / `@Produces` methods.
    for m in &c.methods {
        let producer = has(&m.annotations, "Produces")
            && (f.imports_from("jakarta.enterprise.inject")
                || f.imports_from("javax.enterprise.inject"));
        let Some(bean_ann) = find(&m.annotations, "Bean")
            .or_else(|| producer.then(|| find(&m.annotations, "Produces")).flatten())
        else {
            continue;
        };
        let names = if producer {
            Vec::new()
        } else {
            index.strings(bean_ann, &["value", "name"], f)
        };
        let (simple, _) = injected_type(&m.return_type);
        model.beans.push(Bean {
            name: names.into_iter().next().unwrap_or_else(|| m.name.clone()),
            type_name: if simple.is_empty() {
                m.return_type.clone()
            } else {
                index.qualify(&simple)
            },
            kind: if producer {
                "producer".into()
            } else {
                "bean".into()
            },
            framework: if producer {
                "cdi".into()
            } else {
                "spring".into()
            },
            scope: find(&m.annotations, "Scope").and_then(|a| {
                index
                    .strings(a, &["value", "scopeName"], f)
                    .into_iter()
                    .next()
            }),
            profiles: profiles_of(&m.annotations),
            primary: has(&m.annotations, "Primary"),
            qualifier: qualifier_of(&m.annotations),
            location: location(f, m.line, m.column),
            declared_in: Some(c.qualified.clone()),
            injected_into: Vec::new(),
        });
        for p in &m.params {
            push_injection(
                model,
                f,
                c,
                &p.name,
                &p.type_name,
                &p.annotations,
                if producer { "producer" } else { "beanMethod" },
                p.line,
                p.column,
            );
        }
    }
    if !spring_bean && !cdi_bean {
        return;
    }
    // Constructors.
    let marked: Vec<&Callable> = c
        .constructors
        .iter()
        .filter(|k| has(&k.annotations, "Autowired") || has(&k.annotations, "Inject"))
        .collect();
    let ctor = match (marked.first(), c.constructors.as_slice()) {
        (Some(k), _) => Some(*k),
        (None, [only]) if spring_bean => Some(only),
        _ => None,
    };
    if let Some(k) = ctor {
        for p in &k.params {
            push_injection(
                model,
                f,
                c,
                &p.name,
                &p.type_name,
                &p.annotations,
                "constructor",
                p.line,
                p.column,
            );
        }
    } else if spring_bean
        && c.constructors.is_empty()
        && (c.annotation("RequiredArgsConstructor").is_some()
            || c.annotation("AllArgsConstructor").is_some())
    {
        let all = c.annotation("AllArgsConstructor").is_some();
        for fd in c.fields.iter().filter(|fd| {
            !fd.has_modifier("static")
                && !fd.initialized
                && (all || fd.has_modifier("final") || has(&fd.annotations, "NonNull"))
        }) {
            push_injection(
                model,
                f,
                c,
                &fd.name,
                &fd.type_name,
                &fd.annotations,
                "constructor",
                fd.line,
                fd.column,
            );
        }
    }
    for fd in &c.fields {
        if has(&fd.annotations, "Autowired")
            || has(&fd.annotations, "Inject")
            || has(&fd.annotations, "Resource")
        {
            push_injection(
                model,
                f,
                c,
                &fd.name,
                &fd.type_name,
                &fd.annotations,
                "field",
                fd.line,
                fd.column,
            );
        }
    }
    for m in &c.methods {
        if (has(&m.annotations, "Autowired") || has(&m.annotations, "Inject"))
            && !has(&m.annotations, "Bean")
        {
            for p in &m.params {
                push_injection(
                    model,
                    f,
                    c,
                    &p.name,
                    &p.type_name,
                    &p.annotations,
                    "setter",
                    p.line,
                    p.column,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_injection(
    model: &mut Model,
    f: &SourceFile,
    c: &ClassInfo,
    member: &str,
    type_name: &str,
    annotations: &[Annotation],
    kind: &str,
    line: u32,
    column: u32,
) {
    if [
        "Value",
        "ConfigProperty",
        "Context",
        "BeanParam",
        "PathParam",
        "QueryParam",
    ]
    .iter()
    .any(|a| has(annotations, a))
    {
        return;
    }
    let (simple, many) = injected_type(type_name);
    if simple.is_empty()
        || SCALARS.contains(&simple.as_str())
        || simple.chars().next().is_some_and(|ch| ch.is_lowercase())
    {
        return;
    }
    let qualifier = qualifier_of(annotations)
        .or_else(|| find(annotations, "Resource").and_then(|a| a.string(&["name"])));
    model.injections.push(Injection {
        owner: c.qualified.clone(),
        member: member.to_string(),
        kind: kind.to_string(),
        type_name: type_name.to_string(),
        qualifier,
        many,
        location: location(f, line, column),
        candidates: Vec::new(),
    });
}

fn resolve_injections(model: &mut Model, index: &Index) {
    let types: Vec<BTreeSet<String>> = model
        .beans
        .iter()
        .map(|b| {
            let simple = b
                .type_name
                .rsplit('.')
                .next()
                .unwrap_or(&b.type_name)
                .to_string();
            let mut s = index.supers(&simple);
            s.insert(simple);
            s
        })
        .collect();
    for (ii, inj) in model.injections.iter_mut().enumerate() {
        let (simple, _) = injected_type(&inj.type_name);
        let mut found: Vec<usize> = types
            .iter()
            .enumerate()
            .filter(|(bi, t)| t.contains(&simple) && model.beans[*bi].type_name != inj.owner)
            .map(|(bi, _)| bi)
            .collect();
        if let Some(q) = &inj.qualifier {
            let named: Vec<usize> = found
                .iter()
                .copied()
                .filter(|b| {
                    model.beans[*b].name == *q || model.beans[*b].qualifier.as_deref() == Some(q)
                })
                .collect();
            if !named.is_empty() {
                found = named;
            }
        } else if found.len() > 1 && !inj.many {
            // Spring falls back to the parameter / field name.
            if let Some(b) = found
                .iter()
                .copied()
                .find(|b| model.beans[*b].name == inj.member)
            {
                if !found.iter().any(|x| model.beans[*x].primary) {
                    found = vec![b];
                }
            }
        }
        found.sort_by_key(|b| !model.beans[*b].primary);
        for b in &found {
            model.beans[*b].injected_into.push(ii);
        }
        inj.candidates = found;
    }
}

fn collect_endpoints(
    model: &mut Model,
    index: &Index,
    f: &SourceFile,
    c: &ClassInfo,
    app_path: &str,
) {
    if c.annotation("FeignClient").is_some() || c.annotation("HttpExchange").is_some() {
        return;
    }
    // Spring MVC / WebFlux.
    let controller = c.annotation("RestController").is_some()
        || c.annotation("Controller").is_some()
        || c.annotation("RequestMapping").is_some();
    if controller {
        let class_mapping = c.annotation("RequestMapping");
        let prefixes = class_mapping
            .map(|a| index.strings(a, &["value", "path"], f))
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| vec![String::new()]);
        let class_produces: Vec<String> = class_mapping
            .and_then(|a| a.arg("produces"))
            .map(|v| v.iter().map(media_type).collect())
            .unwrap_or_default();
        for m in &c.methods {
            for (ann, method) in MAPPINGS {
                let Some(a) = find(&m.annotations, ann) else {
                    continue;
                };
                let mut paths = index.strings(a, &["value", "path"], f);
                if paths.is_empty() {
                    paths.push(String::new());
                }
                let methods: Vec<String> = if method.is_empty() {
                    let ms: Vec<String> = a
                        .arg("method")
                        .unwrap_or_default()
                        .iter()
                        .map(|v| v.simple().to_string())
                        .collect();
                    if ms.is_empty() {
                        vec!["ANY".into()]
                    } else {
                        ms
                    }
                } else {
                    vec![method.to_string()]
                };
                let produces: Vec<String> = a
                    .arg("produces")
                    .map(|v| v.iter().map(media_type).collect())
                    .unwrap_or_else(|| class_produces.clone());
                let consumes: Vec<String> = a
                    .arg("consumes")
                    .map(|v| v.iter().map(media_type).collect())
                    .unwrap_or_default();
                let params = spring_params(m);
                for prefix in &prefixes {
                    for p in &paths {
                        for verb in &methods {
                            model.endpoints.push(Endpoint {
                                method: verb.clone(),
                                path: join_paths(&[prefix, p]),
                                handler: format!("{}.{}", c.name, m.name),
                                class: c.qualified.clone(),
                                framework: "spring".into(),
                                location: location(f, m.line, m.column),
                                produces: produces.clone(),
                                consumes: consumes.clone(),
                                params: params.clone(),
                            });
                        }
                    }
                }
            }
        }
    }
    // JAX-RS.
    let jaxrs_file = f.imports_from("jakarta.ws.rs")
        || f.imports_from("javax.ws.rs")
        || f.imports
            .iter()
            .any(|i| i.starts_with("jakarta.ws.rs.") || i.starts_with("javax.ws.rs."));
    let Some(class_path) = c.annotation("Path") else {
        return;
    };
    if !jaxrs_file && !class_path.name.contains("ws.rs") {
        return;
    }
    let base = index
        .strings(class_path, &["value"], f)
        .into_iter()
        .next()
        .unwrap_or_default();
    let class_produces: Vec<String> = c
        .annotation("Produces")
        .and_then(|a| a.arg("value"))
        .map(|v| v.iter().map(media_type).collect())
        .unwrap_or_default();
    let class_consumes: Vec<String> = c
        .annotation("Consumes")
        .and_then(|a| a.arg("value"))
        .map(|v| v.iter().map(media_type).collect())
        .unwrap_or_default();
    for m in &c.methods {
        let Some(verb) = JAXRS_METHODS.iter().find(|v| has(&m.annotations, v)) else {
            continue;
        };
        let sub = find(&m.annotations, "Path")
            .and_then(|a| index.strings(a, &["value"], f).into_iter().next())
            .unwrap_or_default();
        let params = m
            .params
            .iter()
            .filter_map(|p| {
                let (kind, ann) = [
                    ("path", "PathParam"),
                    ("query", "QueryParam"),
                    ("header", "HeaderParam"),
                ]
                .iter()
                .find_map(|(k, a)| find(&p.annotations, a).map(|x| (*k, Some(x))))
                .unwrap_or(("body", None));
                if kind == "body"
                    && (p.annotations.iter().any(|a| {
                        matches!(
                            a.simple(),
                            "Context"
                                | "Suspended"
                                | "BeanParam"
                                | "FormParam"
                                | "CookieParam"
                                | "MatrixParam"
                        )
                    }) || matches!(*verb, "GET" | "DELETE" | "HEAD" | "OPTIONS"))
                {
                    return None;
                }
                Some(EndpointParam {
                    name: ann
                        .and_then(|a| a.string(&["value"]))
                        .unwrap_or_else(|| p.name.clone()),
                    kind: kind.into(),
                    type_name: p.type_name.clone(),
                })
            })
            .collect();
        model.endpoints.push(Endpoint {
            method: verb.to_string(),
            path: join_paths(&[app_path, &base, &sub]),
            handler: format!("{}.{}", c.name, m.name),
            class: c.qualified.clone(),
            framework: "jaxrs".into(),
            location: location(f, m.line, m.column),
            produces: find(&m.annotations, "Produces")
                .and_then(|a| a.arg("value"))
                .map(|v| v.iter().map(media_type).collect())
                .unwrap_or_else(|| class_produces.clone()),
            consumes: find(&m.annotations, "Consumes")
                .and_then(|a| a.arg("value"))
                .map(|v| v.iter().map(media_type).collect())
                .unwrap_or_else(|| class_consumes.clone()),
            params,
        });
    }
}

fn spring_params(m: &Callable) -> Vec<EndpointParam> {
    m.params
        .iter()
        .filter_map(|p| {
            let (kind, ann) = [
                ("path", "PathVariable"),
                ("query", "RequestParam"),
                ("header", "RequestHeader"),
                ("body", "RequestBody"),
            ]
            .iter()
            .find_map(|(k, a)| find(&p.annotations, a).map(|x| (*k, x)))?;
            Some(EndpointParam {
                name: ann
                    .string(&["value", "name"])
                    .unwrap_or_else(|| p.name.clone()),
                kind: kind.into(),
                type_name: p.type_name.clone(),
            })
        })
        .collect()
}

fn collect_entity(model: &mut Model, index: &Index, f: &SourceFile, c: &ClassInfo) {
    let (kind, ann) = match (
        c.annotation("Entity"),
        c.annotation("Embeddable"),
        c.annotation("MappedSuperclass"),
    ) {
        (Some(a), _, _) => ("entity", a),
        (_, Some(a), _) => ("embeddable", a),
        (_, _, Some(a)) => ("mappedSuperclass", a),
        _ => return,
    };
    let namespace = if f.imports_from("jakarta.persistence")
        || f.imports
            .iter()
            .any(|i| i.starts_with("jakarta.persistence"))
    {
        Some("jakarta".to_string())
    } else if f.imports.iter().any(|i| i.starts_with("javax.persistence")) {
        Some("javax".to_string())
    } else if ann.name.contains("persistence") {
        Some(
            if ann.name.starts_with("javax") {
                "javax"
            } else {
                "jakarta"
            }
            .to_string(),
        )
    } else if f
        .imports
        .iter()
        .any(|i| i.starts_with("org.springframework.data"))
    {
        None
    } else {
        return;
    };
    let fields = c
        .fields
        .iter()
        .filter(|fd| {
            !fd.has_modifier("static")
                && !has(&fd.annotations, "Transient")
                && !fd.has_modifier("transient")
                && !fd.has_modifier("const")
        })
        .map(|fd| {
            let relation = [
                ("ManyToOne", "manyToOne"),
                ("OneToMany", "oneToMany"),
                ("ManyToMany", "manyToMany"),
                ("OneToOne", "oneToOne"),
                ("Embedded", "embedded"),
                ("ElementCollection", "elementCollection"),
            ]
            .iter()
            .find(|(a, _)| has(&fd.annotations, a))
            .map(|(_, r)| r.to_string());
            let column = find(&fd.annotations, "Column")
                .or_else(|| find(&fd.annotations, "JoinColumn"))
                .and_then(|a| index.strings(a, &["name"], f).into_iter().next());
            EntityField {
                name: fd.name.clone(),
                type_name: fd.type_name.clone(),
                column,
                id: has(&fd.annotations, "Id") || has(&fd.annotations, "EmbeddedId"),
                generated: has(&fd.annotations, "GeneratedValue"),
                target: relation.as_ref().map(|_| injected_type(&fd.type_name).0),
                relation,
                line: fd.line,
            }
        })
        .collect();
    model.entities.push(Entity {
        name: index
            .strings(ann, &["name"], f)
            .into_iter()
            .next()
            .unwrap_or_else(|| c.name.clone()),
        class: c.qualified.clone(),
        kind: kind.into(),
        table: c
            .annotation("Table")
            .and_then(|a| index.strings(a, &["name", "value"], f).into_iter().next()),
        location: location(f, c.line, c.column),
        fields,
        repositories: Vec::new(),
        namespace,
    });
}

fn link_repositories(model: &mut Model, index: &Index) {
    for b in model.beans.iter().filter(|b| b.kind == "dataRepository") {
        let simple = b.type_name.rsplit('.').next().unwrap_or(&b.type_name);
        let Some(c) = index.class(simple) else {
            continue;
        };
        // The entity is the first type argument of a repository supertype, also through
        // project interfaces.
        let mut todo: Vec<&ClassInfo> = vec![c];
        let mut entity = None;
        while let Some(x) = todo.pop() {
            for t in &x.supertypes {
                if t.simple().ends_with("Repository") {
                    if let Some(a) = t.args.first() {
                        entity = Some(injected_type(a).0);
                        break;
                    }
                    if let Some(p) = index.class(t.simple()) {
                        todo.push(p);
                    }
                }
            }
            if entity.is_some() {
                break;
            }
        }
        if let Some(e) = entity {
            if let Some(ent) = model
                .entities
                .iter_mut()
                .find(|x| x.class.rsplit('.').next() == Some(e.as_str()))
            {
                ent.repositories.push(b.type_name.clone());
            }
        }
    }
}

fn collect_config_class(model: &mut Model, index: &Index, f: &SourceFile, c: &ClassInfo) {
    let Some(a) = c.annotation("ConfigurationProperties") else {
        return;
    };
    let Some(prefix) = index.strings(a, &["prefix", "value"], f).into_iter().next() else {
        return;
    };
    let mut keys = Vec::new();
    config_properties(model, index, f, c, &prefix, &c.qualified, &mut keys, 0);
    model.config_classes.push(ConfigClass {
        prefix,
        class: c.qualified.clone(),
        location: location(f, c.line, c.column),
        keys,
    });
}

#[allow(clippy::too_many_arguments)]
fn config_properties(
    model: &mut Model,
    index: &Index,
    f: &SourceFile,
    c: &ClassInfo,
    prefix: &str,
    source: &str,
    keys: &mut Vec<String>,
    depth: usize,
) {
    for fd in c.fields.iter().filter(|fd| {
        !fd.has_modifier("static") && !fd.has_modifier("const") && !fd.has_modifier("transient")
    }) {
        let key = format!("{prefix}.{}", config::kebab(&fd.name));
        let t = TypeRef::parse(&fd.type_name);
        let nested = index.class(t.simple()).filter(|n| {
            t.args.is_empty()
                && matches!(n.kind, ClassKind::Class | ClassKind::Record)
                && !SCALARS.contains(&t.simple())
        });
        if let (Some(n), true) = (nested, depth < 4) {
            let file = index
                .files
                .iter()
                .find(|x| x.classes.iter().any(|y| std::ptr::eq(y, n)))
                .unwrap_or(f);
            config_properties(model, index, file, n, &key, source, keys, depth + 1);
            continue;
        }
        let type_name = match index.class(t.simple()) {
            Some(e) if e.kind == ClassKind::Enum => e.qualified.clone(),
            _ => fd.type_name.trim_end_matches('?').to_string(),
        };
        let boolean = matches!(t.simple(), "boolean" | "Boolean");
        model.metadata.add_property(Property {
            name: key.clone(),
            type_name: Some(if boolean {
                "java.lang.Boolean".into()
            } else {
                type_name
            }),
            description: fd.doc.clone(),
            source_type: Some(source.to_string()),
            default_value: fd.default.clone().map(serde_json::Value::String),
            location: Some(location(f, fd.line, fd.column)),
            origin: Some("project".into()),
            ..Property::default()
        });
        keys.push(key);
    }
}

// ---- Gutter ---------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub path: String,
    pub line: u32,
    pub column: u32,
    /// Shown in the Choose Declaration popup.
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    /// 0-based.
    pub line: u32,
    /// `bean`, `injection`, `endpoint`, `entity`, `repository` or `configProperties`.
    pub kind: String,
    pub title: String,
    pub targets: Vec<Target>,
    /// `endpoint`: indices into [`Model::endpoints`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<usize>,
}

fn target(l: &Location, preview: String) -> Target {
    Target {
        path: l.path.clone(),
        line: l.line,
        column: l.column,
        preview,
    }
}

fn short(q: &str) -> &str {
    q.rsplit('.').next().unwrap_or(q)
}

/// Gutter markers of one file of the model.
pub fn gutter(model: &Model, path: &str) -> Vec<Marker> {
    let mut out: Vec<Marker> = Vec::new();
    for b in model.beans.iter().filter(|b| b.location.path == path) {
        let targets: Vec<Target> = b
            .injected_into
            .iter()
            .map(|i| {
                let inj = &model.injections[*i];
                target(
                    &inj.location,
                    format!("{}.{} ({})", short(&inj.owner), inj.member, inj.type_name),
                )
            })
            .collect();
        let what = match b.framework.as_str() {
            "cdi" => "CDI bean",
            _ => "Spring bean",
        };
        let title = match targets.len() {
            0 => format!("{what} '{}'", b.name),
            1 => format!("{what} '{}' · injected into {}", b.name, targets[0].preview),
            n => format!("{what} '{}' · injected into {n} places", b.name),
        };
        out.push(Marker {
            line: b.location.line,
            kind: "bean".into(),
            title,
            targets,
            endpoints: Vec::new(),
        });
    }
    for inj in model.injections.iter().filter(|i| i.location.path == path) {
        let targets: Vec<Target> = inj
            .candidates
            .iter()
            .map(|b| {
                let bean = &model.beans[*b];
                target(
                    &bean.location,
                    format!("{} ({})", bean.name, short(&bean.type_name)),
                )
            })
            .collect();
        let title = match targets.len() {
            0 => format!("Autowired: {} (no bean in the project; a library or auto-configuration provides it)", inj.type_name),
            1 => format!("Autowired: {}", targets[0].preview),
            n => format!("Autowired candidates: {n} beans of {}", injected_type(&inj.type_name).0),
        };
        out.push(Marker {
            line: inj.location.line,
            kind: "injection".into(),
            title,
            targets,
            endpoints: Vec::new(),
        });
    }
    let mut by_line: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, e) in model
        .endpoints
        .iter()
        .enumerate()
        .filter(|(_, e)| e.location.path == path)
    {
        by_line.entry(e.location.line).or_default().push(i);
    }
    for (line, list) in by_line {
        let titles: Vec<String> = list
            .iter()
            .map(|i| {
                format!(
                    "{} {}",
                    model.endpoints[*i].method, model.endpoints[*i].path
                )
            })
            .collect();
        out.push(Marker {
            line,
            kind: "endpoint".into(),
            title: titles.join(", "),
            targets: Vec::new(),
            endpoints: list,
        });
    }
    for e in model.entities.iter().filter(|e| e.location.path == path) {
        let targets: Vec<Target> = e
            .repositories
            .iter()
            .filter_map(|r| {
                model
                    .classes
                    .get(r)
                    .map(|l| target(l, short(r).to_string()))
            })
            .collect();
        let table = e
            .table
            .as_deref()
            .map(|t| format!(" (table {t})"))
            .unwrap_or_default();
        out.push(Marker {
            line: e.location.line,
            kind: "entity".into(),
            title: format!(
                "JPA {} '{}'{table}",
                if e.kind == "entity" {
                    "entity"
                } else {
                    e.kind.as_str()
                },
                e.name
            ),
            targets,
            endpoints: Vec::new(),
        });
    }
    for e in &model.entities {
        for r in &e.repositories {
            if let Some(l) = model.classes.get(r).filter(|l| l.path == path) {
                out.push(Marker {
                    line: l.line,
                    kind: "repository".into(),
                    title: format!("Spring Data repository of '{}'", e.name),
                    targets: vec![target(&e.location, short(&e.class).to_string())],
                    endpoints: Vec::new(),
                });
            }
        }
    }
    for c in model
        .config_classes
        .iter()
        .filter(|c| c.location.path == path)
    {
        let prefix = canonical(&c.prefix);
        let targets: Vec<Target> = model
            .config_values
            .iter()
            .filter(|v| v.key == prefix || v.key.starts_with(&format!("{prefix}.")))
            .map(|v| target(&v.location, format!("{}={}", v.key, v.value)))
            .collect();
        out.push(Marker {
            line: c.location.line,
            kind: "configProperties".into(),
            title: format!(
                "Configuration properties '{}' · {} value(s) in the configuration files",
                c.prefix,
                targets.len()
            ),
            targets,
            endpoints: Vec::new(),
        });
    }
    // A bean class that is also an entity / injection on the same line keeps both icons.
    out.sort_by_key(|m| m.line);
    out
}

/// A configuration value for the active `profiles` (later profiles win, like Spring).
pub fn setting<'a>(model: &'a Model, key: &str, profiles: &[String]) -> Option<&'a str> {
    let key = canonical(key);
    let mut found: Option<&ConfigValue> = None;
    let rank = |v: &ConfigValue| -> Option<usize> {
        match &v.profile {
            None => Some(0),
            Some(p) => profiles.iter().position(|x| x == p).map(|i| i + 1),
        }
    };
    for v in model.config_values.iter().filter(|v| v.key == key) {
        let Some(r) = rank(v) else { continue };
        let props_over_yaml = |v: &ConfigValue| v.location.path.ends_with(".properties");
        match found {
            Some(f) if rank(f) > Some(r) => {}
            Some(f) if rank(f) == Some(r) && props_over_yaml(f) && !props_over_yaml(v) => {}
            _ => found = Some(v),
        }
    }
    found.map(|v| v.value.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(p, t)| (p.to_string(), t.to_string()))
            .collect()
    }

    const ORDER_CONTROLLER: &str = r#"
package com.acme.shop.web;

import com.acme.shop.orders.OrderService;
import org.springframework.http.MediaType;
import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping(value = Paths.ORDERS, produces = MediaType.APPLICATION_JSON_VALUE)
public class OrderController {
    private final OrderService orders;

    public OrderController(OrderService orders) {
        this.orders = orders;
    }

    @GetMapping
    public List<Order> list(@RequestParam(defaultValue = "0") int page) { return orders.all(); }

    @GetMapping("/{id}")
    public Order find(@PathVariable Long id) { return orders.find(id); }

    @PostMapping(consumes = MediaType.APPLICATION_JSON_VALUE)
    public Order create(@RequestBody NewOrder order) { return orders.create(order); }

    @RequestMapping(path = "/{id}", method = {RequestMethod.PUT, RequestMethod.PATCH})
    public Order update(@PathVariable("id") Long orderId, @RequestBody Order order) { return order; }
}
"#;

    const PATHS: &str = "package com.acme.shop.web;\npublic final class Paths { public static final String ORDERS = \"/api/orders\"; }\n";

    const SERVICE: &str = r#"
package com.acme.shop.orders

import org.springframework.stereotype.Service
import org.springframework.beans.factory.annotation.Autowired

@Service
class OrderService(
    private val repository: OrderRepository,
    private val notifiers: List<Notifier>,
    @Qualifier("fast") private val payments: PaymentGateway,
) {
    @Autowired
    lateinit var clock: java.time.Clock
}
"#;

    const REPO: &str = "package com.acme.shop.orders;\nimport org.springframework.data.jpa.repository.JpaRepository;\npublic interface OrderRepository extends JpaRepository<Order, Long> {}\n";

    const ENTITY: &str = r#"
package com.acme.shop.orders;

import jakarta.persistence.*;

@Entity
@Table(name = "orders")
public class Order {
    @Id @GeneratedValue private Long id;
    @Column(name = "customer_email") private String email;
    @ManyToOne @JoinColumn(name = "customer_id") private Customer customer;
    @OneToMany(mappedBy = "order") private List<OrderLine> lines;
    @Transient private int cached;
    private static final long serialVersionUID = 1L;
}
"#;

    const CONFIG: &str = r#"
package com.acme.shop.config;

import org.springframework.context.annotation.*;

@Configuration
@Profile("!test")
public class ShopConfig {
    @Bean
    public java.time.Clock clock() { return java.time.Clock.systemUTC(); }

    @Bean("fast")
    @Primary
    public PaymentGateway fastGateway(GatewayProperties props) { return new StripeGateway(props); }

    @Bean
    public PaymentGateway slowGateway() { return new InvoiceGateway(); }
}

@Component
class MailNotifier implements Notifier {}

@Component
class SmsNotifier implements Notifier {}

interface Notifier {}

/** Payment gateway settings. */
@ConfigurationProperties(prefix = "shop.gateway")
record GatewayProperties(
    /** API key of the gateway. */
    String apiKey,
    Mode mode,
    boolean sandbox,
    Retry retry
) {}

record Retry(int attempts, Duration backoff) {}

enum Mode { LIVE, TEST }
"#;

    const JAXRS: &str = r#"
package com.acme.legacy;

import jakarta.inject.Inject;
import jakarta.ws.rs.*;
import jakarta.ws.rs.core.MediaType;
import jakarta.enterprise.context.ApplicationScoped;

@ApplicationPath("/rest")
public class LegacyApp extends Application {}

@Path("/customers")
@Produces(MediaType.APPLICATION_JSON)
public class CustomerResource {
    @Inject CustomerStore store;

    @GET
    @Path("{id}")
    public Customer get(@PathParam("id") long id, @QueryParam("expand") String expand) { return store.find(id); }

    @POST
    public void add(Customer c) {}
}

@ApplicationScoped
class CustomerStore {}
"#;

    fn model() -> Model {
        analyze(&files(&[
            ("src/main/java/com/acme/shop/web/OrderController.java", ORDER_CONTROLLER),
            ("src/main/java/com/acme/shop/web/Paths.java", PATHS),
            ("src/main/kotlin/com/acme/shop/orders/OrderService.kt", SERVICE),
            ("src/main/java/com/acme/shop/orders/OrderRepository.java", REPO),
            ("src/main/java/com/acme/shop/orders/Order.java", ENTITY),
            ("src/main/java/com/acme/shop/config/ShopConfig.java", CONFIG),
            ("src/main/java/com/acme/legacy/CustomerResource.java", JAXRS),
            ("src/main/resources/application.yml", "server:\n  port: 8081\nshop:\n  gateway:\n    api-key: secret\n    mode: live\nspring:\n  profiles:\n    active: dev\n"),
            ("src/main/resources/application-prod.properties", "server.port=80\n"),
            ("src/main/resources/application-dev.properties", "server.port=8082\n"),
            ("src/main/resources/META-INF/additional-spring-configuration-metadata.json", r#"{"properties":[{"name":"shop.banner","type":"java.lang.String","description":"Banner text."}]}"#),
        ]))
    }

    #[test]
    fn beans_and_injection_points() {
        let m = model();
        let names: Vec<(&str, &str)> = m
            .beans
            .iter()
            .map(|b| (b.name.as_str(), b.kind.as_str()))
            .collect();
        for expected in [
            ("orderController", "restController"),
            ("orderService", "service"),
            ("orderRepository", "dataRepository"),
            ("shopConfig", "configuration"),
            ("clock", "bean"),
            ("fast", "bean"),
            ("slowGateway", "bean"),
            ("mailNotifier", "component"),
            ("customerStore", "cdi"),
            ("customerResource", "resource"),
        ] {
            assert!(names.contains(&expected), "{expected:?} in {names:?}");
        }
        let fast = m.beans.iter().find(|b| b.name == "fast").unwrap();
        assert!(fast.primary);
        assert_eq!(
            fast.declared_in.as_deref(),
            Some("com.acme.shop.config.ShopConfig")
        );
        let config = m.beans.iter().find(|b| b.name == "shopConfig").unwrap();
        assert_eq!(config.profiles, ["test"]);
        let inj = |member: &str| m.injections.iter().find(|i| i.member == member).unwrap();
        let cand = |member: &str| {
            inj(member)
                .candidates
                .iter()
                .map(|b| m.beans[*b].name.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(inj("orders").kind, "constructor");
        assert_eq!(cand("orders"), ["orderService"]);
        assert_eq!(cand("repository"), ["orderRepository"]);
        assert_eq!(cand("notifiers"), ["mailNotifier", "smsNotifier"]);
        assert!(inj("notifiers").many);
        assert_eq!(cand("payments"), ["fast"]);
        assert_eq!(cand("clock"), ["clock"]);
        assert_eq!(inj("clock").kind, "field");
        assert_eq!(inj("props").kind, "beanMethod");
        assert_eq!(
            cand("props"),
            ["shop.gateway-com.acme.shop.config.GatewayProperties"]
        );
        assert_eq!(cand("store"), ["customerStore"]);
        let service = m.beans.iter().find(|b| b.name == "orderService").unwrap();
        assert_eq!(service.injected_into.len(), 1);
    }

    #[test]
    fn spring_and_jaxrs_endpoints() {
        let m = model();
        let eps: Vec<(&str, &str, &str)> = m
            .endpoints
            .iter()
            .map(|e| (e.method.as_str(), e.path.as_str(), e.handler.as_str()))
            .collect();
        assert_eq!(
            eps,
            [
                ("GET", "/rest/customers/{id}", "CustomerResource.get"),
                ("POST", "/rest/customers", "CustomerResource.add"),
                ("GET", "/api/orders", "OrderController.list"),
                ("GET", "/api/orders/{id}", "OrderController.find"),
                ("POST", "/api/orders", "OrderController.create"),
                ("PUT", "/api/orders/{id}", "OrderController.update"),
                ("PATCH", "/api/orders/{id}", "OrderController.update"),
            ]
        );
        let create = &m.endpoints[4];
        assert_eq!(create.consumes, ["application/json"]);
        assert_eq!(create.produces, ["application/json"]);
        assert_eq!(create.params[0].kind, "body");
        assert_eq!(m.endpoints[6].params[0].name, "id");
        let get = &m.endpoints[0];
        assert_eq!(
            get.params
                .iter()
                .map(|p| (p.name.as_str(), p.kind.as_str()))
                .collect::<Vec<_>>(),
            [("id", "path"), ("expand", "query")]
        );
        assert_eq!(m.endpoints[1].params[0].kind, "body");
        assert_eq!(m.endpoints[3].location.line, 20);
    }

    #[test]
    fn entities_config_classes_apis_and_profiles() {
        let m = model();
        let order = &m.entities[0];
        assert_eq!(
            (
                order.name.as_str(),
                order.table.as_deref(),
                order.namespace.as_deref()
            ),
            ("Order", Some("orders"), Some("jakarta"))
        );
        assert_eq!(
            order
                .fields
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["id", "email", "customer", "lines"]
        );
        assert!(order.fields[0].id && order.fields[0].generated);
        assert_eq!(order.fields[1].column.as_deref(), Some("customer_email"));
        assert_eq!(
            (
                order.fields[2].relation.as_deref(),
                order.fields[2].target.as_deref()
            ),
            (Some("manyToOne"), Some("Customer"))
        );
        assert_eq!(order.fields[3].target.as_deref(), Some("OrderLine"));
        assert_eq!(order.repositories, ["com.acme.shop.orders.OrderRepository"]);

        let gw = &m.config_classes[0];
        assert_eq!(gw.prefix, "shop.gateway");
        assert_eq!(
            gw.keys,
            [
                "shop.gateway.api-key",
                "shop.gateway.mode",
                "shop.gateway.sandbox",
                "shop.gateway.retry.attempts",
                "shop.gateway.retry.backoff"
            ]
        );
        let key = m.metadata.get("shop.gateway.apiKey").unwrap();
        assert_eq!(key.description.as_deref(), Some("API key of the gateway."));
        assert_eq!(key.location.as_ref().unwrap().line, 31);
        assert_eq!(
            m.metadata
                .get("shop.gateway.mode")
                .unwrap()
                .type_name
                .as_deref(),
            Some("com.acme.shop.config.Mode")
        );
        assert_eq!(m.enums["com.acme.shop.config.Mode"], ["LIVE", "TEST"]);
        assert!(m.metadata.get("shop.gateway.sandbox").unwrap().is_boolean());
        assert!(m.metadata.get("shop.banner").is_some());

        let ids: Vec<(&str, &str)> = m
            .apis
            .iter()
            .map(|a| (a.id.as_str(), a.namespace.as_str()))
            .collect();
        assert!(
            ids.contains(&("jaxrs", "jakarta"))
                && ids.contains(&("jpa", "jakarta"))
                && ids.contains(&("cdi", "jakarta"))
        );
        assert!(ids.contains(&("spring-data-jpa", "spring")));
        assert!(m.spring && m.jakarta);
        assert_eq!(m.profiles, ["dev", "prod", "test"]);
        assert_eq!(
            m.applications
                .iter()
                .map(|a| a.kind.as_str())
                .collect::<Vec<_>>(),
            ["jaxrs"]
        );

        assert_eq!(setting(&m, "server.port", &[]), Some("8081"));
        assert_eq!(setting(&m, "server.port", &["dev".into()]), Some("8082"));
        assert_eq!(
            setting(&m, "server.port", &["dev".into(), "prod".into()]),
            Some("80")
        );
        assert_eq!(setting(&m, "server.servlet.context-path", &[]), None);
    }

    #[test]
    fn gutter_markers_link_beans_injections_and_endpoints() {
        let m = model();
        let svc = gutter(&m, "src/main/kotlin/com/acme/shop/orders/OrderService.kt");
        let kinds: Vec<(&str, u32)> = svc.iter().map(|x| (x.kind.as_str(), x.line)).collect();
        assert_eq!(kinds[0], ("bean", 7));
        assert!(kinds.contains(&("injection", 8)));
        let repo_inj = svc
            .iter()
            .find(|x| x.kind == "injection" && x.line == 8)
            .unwrap();
        assert_eq!(
            repo_inj.targets[0].path,
            "src/main/java/com/acme/shop/orders/OrderRepository.java"
        );
        let bean = &svc[0];
        assert_eq!(
            bean.targets[0].path,
            "src/main/java/com/acme/shop/web/OrderController.java"
        );
        assert!(bean.title.contains("injected into OrderController.orders"));
        let ctl = gutter(&m, "src/main/java/com/acme/shop/web/OrderController.java");
        let update = ctl
            .iter()
            .find(|x| x.kind == "endpoint" && x.title.starts_with("PUT"))
            .unwrap();
        assert_eq!(update.title, "PUT /api/orders/{id}, PATCH /api/orders/{id}");
        assert_eq!(update.endpoints.len(), 2);
        let order = gutter(&m, "src/main/java/com/acme/shop/orders/Order.java");
        assert_eq!(order[0].title, "JPA entity 'Order' (table orders)");
        let repo = gutter(
            &m,
            "src/main/java/com/acme/shop/orders/OrderRepository.java",
        );
        assert!(repo.iter().any(|x| x.kind == "repository"));
        let cfg = gutter(&m, "src/main/java/com/acme/shop/config/ShopConfig.java");
        let props = cfg.iter().find(|x| x.kind == "configProperties").unwrap();
        assert_eq!(props.targets.len(), 2);
    }

    #[test]
    fn applications_and_incremental_updates() {
        let mut p = Project::default();
        p.set_file("src/main/kotlin/com/acme/shop/shopApplication.kt", "package com.acme.shop\n@SpringBootApplication\nclass ShopApplication\nfun main(args: Array<String>) { runApplication<ShopApplication>(*args) }\n");
        p.set_file("src/main/java/com/acme/Other.java", "package com.acme;\n@SpringBootApplication public class Other { public static void main(String[] a) {} }");
        p.set_file("README.md", "ignored");
        assert_eq!(p.len(), 2);
        let m = p.model();
        let mains: Vec<(&str, bool)> = m
            .applications
            .iter()
            .map(|a| (a.main_class.as_str(), a.kotlin))
            .collect();
        assert_eq!(
            mains,
            [
                ("com.acme.Other", false),
                ("com.acme.shop.ShopApplicationKt", true)
            ]
        );
        p.set_file(
            "src/main/java/com/acme/Other.java",
            "package com.acme;\npublic class Other {}",
        );
        assert_eq!(p.model().applications.len(), 1);
        p.remove_file("src/main/kotlin/com/acme/shop/shopApplication.kt");
        assert!(p.model().applications.is_empty());
        assert!(
            is_relevant("src/main/resources/application-dev.yml")
                && !is_relevant("build.gradle.kts")
        );
    }

    #[test]
    fn helpers() {
        assert_eq!(join_paths(&["/api/", "/orders/"]), "/api/orders");
        assert_eq!(join_paths(&["", ""]), "/");
        assert_eq!(join_paths(&["rest", "{id}"]), "/rest/{id}");
        assert_eq!(decapitalize("URLMapper"), "URLMapper");
        assert_eq!(decapitalize("OrderService"), "orderService");
        assert_eq!(injected_type("Optional<List<Foo>>"), ("Foo".into(), true));
        assert_eq!(injected_type("Map<String, Bar>"), ("Bar".into(), true));
        assert_eq!(injected_type("ObjectProvider<Baz>?"), ("Baz".into(), false));
        let consts = BTreeMap::from([("BASE".to_string(), "/api".to_string())]);
        assert_eq!(kotlin_templates("$BASE/{id}", &consts), "/api/{id}");
        assert_eq!(kotlin_templates("${Paths.BASE}/x", &consts), "/api/x");
    }
}
