//! New Project generators (File | New Project). A generator either runs an official CLI
//! (`composer create-project`, `npm create vite`, `ng new`, ...) or writes a built-in
//! offline template. [`plan`] turns a request into the concrete steps, which the UI also
//! shows as a command preview.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use forge_core::{ForgeError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeneratorOption {
    pub id: String,
    pub label: String,
    /// `enum`, `bool` or `string`.
    pub kind: String,
    pub default: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<Choice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Generator {
    pub id: String,
    pub name: String,
    /// Group in the generator list: `PHP`, `Web`, `Lua` or `Other`.
    pub group: String,
    /// Pack that provides it (`phpforge`, `webforge`, `luaforge`, `core`).
    pub pack: String,
    pub description: String,
    /// Icon key for the UI (framework or language id).
    pub icon: String,
    /// Default project folder name.
    pub default_name: String,
    /// Executables the generator needs; empty for offline templates.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Offline generator to offer when a required tool is missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    pub options: Vec<GeneratorOption>,
}

fn choice(value: &str, label: &str) -> Choice {
    Choice {
        value: value.into(),
        label: label.into(),
    }
}

fn enum_opt(id: &str, label: &str, default: &str, choices: Vec<Choice>) -> GeneratorOption {
    GeneratorOption {
        id: id.into(),
        label: label.into(),
        kind: "enum".into(),
        default: json!(default),
        choices,
        description: None,
    }
}

fn bool_opt(id: &str, label: &str, default: bool, description: Option<&str>) -> GeneratorOption {
    GeneratorOption {
        id: id.into(),
        label: label.into(),
        kind: "bool".into(),
        default: json!(default),
        choices: Vec::new(),
        description: description.map(Into::into),
    }
}

fn string_opt(id: &str, label: &str, default: &str) -> GeneratorOption {
    GeneratorOption {
        id: id.into(),
        label: label.into(),
        kind: "string".into(),
        default: json!(default),
        choices: Vec::new(),
        description: None,
    }
}

fn php_version() -> GeneratorOption {
    enum_opt(
        "phpVersion",
        "PHP version",
        "8.3",
        vec![
            choice("8.4", "8.4"),
            choice("8.3", "8.3"),
            choice("8.2", "8.2"),
            choice("8.1", "8.1"),
        ],
    )
}

fn sample() -> GeneratorOption {
    bool_opt(
        "sample",
        "Create sample code",
        true,
        Some("Adds a small example to start from."),
    )
}

fn git() -> GeneratorOption {
    bool_opt("git", "Create Git repository", false, None)
}

fn install() -> GeneratorOption {
    bool_opt(
        "install",
        "Install dependencies",
        true,
        Some("Runs npm install after the project is created."),
    )
}

fn typescript() -> GeneratorOption {
    bool_opt("typescript", "TypeScript", true, None)
}

struct Def {
    id: &'static str,
    name: &'static str,
    group: &'static str,
    pack: &'static str,
    icon: &'static str,
    default_name: &'static str,
    description: &'static str,
    tools: &'static [&'static str],
    fallback: Option<&'static str>,
}

fn generator(d: Def, options: Vec<GeneratorOption>) -> Generator {
    Generator {
        id: d.id.into(),
        name: d.name.into(),
        group: d.group.into(),
        pack: d.pack.into(),
        description: d.description.into(),
        icon: d.icon.into(),
        default_name: d.default_name.into(),
        tools: d.tools.iter().map(|t| t.to_string()).collect(),
        fallback: d.fallback.map(Into::into),
        options,
    }
}

/// All generators in display order.
pub fn generators() -> Vec<Generator> {
    vec![
        generator(
            Def {
                id: "empty-php",
                name: "Empty PHP Project",
                group: "PHP",
                pack: "phpforge",
                icon: "php",
                default_name: "untitled",
                description: "A Composer project with PSR-4 autoloading, src/, tests/ and PHPUnit.",
                tools: &[],
                fallback: None,
            },
            vec![php_version(), sample(), git()],
        ),
        generator(
            Def {
                id: "laravel",
                name: "Laravel",
                group: "PHP",
                pack: "phpforge",
                icon: "laravel",
                default_name: "laravel-app",
                description: "A new Laravel application, optionally with a starter kit.",
                tools: &["composer"],
                fallback: Some("empty-php"),
            },
            vec![
                enum_opt(
                    "starterKit",
                    "Starter kit",
                    "none",
                    vec![
                        choice("none", "None"),
                        choice("react", "React"),
                        choice("vue", "Vue"),
                        choice("livewire", "Livewire"),
                    ],
                ),
                git(),
            ],
        ),
        generator(
            Def {
                id: "symfony",
                name: "Symfony",
                group: "PHP",
                pack: "phpforge",
                icon: "symfony",
                default_name: "symfony-app",
                description:
                    "A Symfony skeleton (microservice, console app, API) or a full web application.",
                tools: &["composer"],
                fallback: Some("empty-php"),
            },
            vec![
                enum_opt(
                    "variant",
                    "Project type",
                    "webapp",
                    vec![
                        choice("webapp", "Web application"),
                        choice("skeleton", "Skeleton (microservice, console, API)"),
                    ],
                ),
                git(),
            ],
        ),
        generator(
            Def {
                id: "composer-package",
                name: "Composer Package",
                group: "PHP",
                pack: "phpforge",
                icon: "composer",
                default_name: "my-package",
                description:
                    "A reusable library for Packagist with PHPUnit, README and MIT license.",
                tools: &[],
                fallback: None,
            },
            vec![
                string_opt("vendor", "Vendor", "acme"),
                php_version(),
                sample(),
                git(),
            ],
        ),
        generator(
            Def {
                id: "wordpress-plugin",
                name: "WordPress Plugin",
                group: "PHP",
                pack: "phpforge",
                icon: "wordpress",
                default_name: "my-plugin",
                description:
                    "A plugin with header, main class, shortcode, readme.txt and uninstall hook.",
                tools: &[],
                fallback: None,
            },
            vec![php_version(), git()],
        ),
        generator(
            Def {
                id: "empty-web",
                name: "Empty HTML/CSS/JS",
                group: "Web",
                pack: "webforge",
                icon: "html",
                default_name: "website",
                description: "A static website: index.html, a stylesheet and a script.",
                tools: &[],
                fallback: None,
            },
            vec![sample(), git()],
        ),
        generator(
            Def {
                id: "vite",
                name: "Vite + TypeScript",
                group: "Web",
                pack: "webforge",
                icon: "vite",
                default_name: "vite-project",
                description: "A framework-free Vite app with TypeScript.",
                tools: &["npm"],
                fallback: Some("empty-web"),
            },
            vec![install(), git()],
        ),
        generator(
            Def {
                id: "react",
                name: "React",
                group: "Web",
                pack: "webforge",
                icon: "react",
                default_name: "react-app",
                description: "A React app built with Vite.",
                tools: &["npm"],
                fallback: Some("empty-web"),
            },
            vec![typescript(), install(), git()],
        ),
        generator(
            Def {
                id: "vue",
                name: "Vue.js",
                group: "Web",
                pack: "webforge",
                icon: "vue",
                default_name: "vue-app",
                description: "A Vue 3 app built with Vite.",
                tools: &["npm"],
                fallback: Some("empty-web"),
            },
            vec![typescript(), install(), git()],
        ),
        generator(
            Def {
                id: "angular",
                name: "Angular",
                group: "Web",
                pack: "webforge",
                icon: "angular",
                default_name: "angular-app",
                description: "An Angular workspace created with the Angular CLI.",
                tools: &["npx"],
                fallback: Some("empty-web"),
            },
            vec![install(), git()],
        ),
        generator(
            Def {
                id: "svelte",
                name: "Svelte",
                group: "Web",
                pack: "webforge",
                icon: "svelte",
                default_name: "svelte-app",
                description: "A SvelteKit app created with the Svelte CLI (sv).",
                tools: &["npx"],
                fallback: Some("empty-web"),
            },
            vec![typescript(), install(), git()],
        ),
        generator(
            Def {
                id: "lua-script",
                name: "Lua Script",
                group: "Lua",
                pack: "luaforge",
                icon: "lua",
                default_name: "lua-script",
                description: "A Lua script with a module and a LuaLS configuration.",
                tools: &[],
                fallback: None,
            },
            vec![
                enum_opt(
                    "luaVersion",
                    "Lua version",
                    "5.4",
                    vec![
                        choice("5.4", "Lua 5.4"),
                        choice("5.3", "Lua 5.3"),
                        choice("5.1", "Lua 5.1"),
                        choice("luajit", "LuaJIT"),
                    ],
                ),
                sample(),
                git(),
            ],
        ),
        generator(
            Def {
                id: "love-game",
                name: "LÖVE Game",
                group: "Lua",
                pack: "luaforge",
                icon: "love",
                default_name: "love-game",
                description: "A LÖVE 11 game with main.lua, conf.lua and LÖVE API completion.",
                tools: &[],
                fallback: None,
            },
            vec![sample(), git()],
        ),
        generator(
            Def {
                id: "neovim-plugin",
                name: "Neovim Plugin",
                group: "Lua",
                pack: "luaforge",
                icon: "neovim",
                default_name: "my-plugin.nvim",
                description: "A Neovim plugin with setup(), a user command and a help file.",
                tools: &[],
                fallback: None,
            },
            vec![string_opt("vendor", "GitHub user", "me"), git()],
        ),
        generator(
            Def {
                id: "ruby-gem",
                name: "Ruby Gem",
                group: "Ruby",
                pack: "rubyforge",
                icon: "ruby",
                default_name: "my_gem",
                description:
                    "A gem created with Bundler (bundle gem): gemspec, Rakefile, tests and RuboCop.",
                tools: &["bundle"],
                fallback: Some("ruby-script"),
            },
            vec![
                enum_opt(
                    "testFramework",
                    "Test framework",
                    "minitest",
                    vec![
                        choice("minitest", "Minitest"),
                        choice("rspec", "RSpec"),
                        choice("test-unit", "test-unit"),
                    ],
                ),
                bool_opt("rubocop", "RuboCop", true, None),
                bool_opt("exe", "Executable (exe/)", false, None),
                git(),
            ],
        ),
        generator(
            Def {
                id: "ruby-script",
                name: "Ruby Project",
                group: "Ruby",
                pack: "rubyforge",
                icon: "ruby",
                default_name: "ruby-project",
                description:
                    "A Ruby project with a Gemfile, a Rakefile, Minitest tests and RuboCop.",
                tools: &[],
                fallback: None,
            },
            vec![sample(), git()],
        ),
        generator(
            Def {
                id: "dotnet",
                name: ".NET Project",
                group: ".NET",
                pack: "dotnetforge",
                icon: "dotnet",
                default_name: "MyApp",
                description: "A solution with a project from a dotnet new template: console, class library, ASP.NET Core, Blazor, worker or tests.",
                tools: &["dotnet"],
                fallback: None,
            },
            vec![
                enum_opt(
                    "template",
                    "Template",
                    "console",
                    vec![
                        choice("console", "Console App"),
                        choice("classlib", "Class Library"),
                        choice("webapi", "ASP.NET Core Web API"),
                        choice("mvc", "ASP.NET Core Web App (Model-View-Controller)"),
                        choice("webapp", "ASP.NET Core Web App (Razor Pages)"),
                        choice("blazor", "Blazor Web App"),
                        choice("worker", "Worker Service"),
                        choice("xunit", "xUnit Test Project"),
                        choice("nunit", "NUnit Test Project"),
                        choice("mstest", "MSTest Test Project"),
                    ],
                ),
                enum_opt(
                    "language",
                    "Language",
                    "C#",
                    vec![choice("C#", "C#"), choice("F#", "F#"), choice("VB", "Visual Basic")],
                ),
                enum_opt(
                    "framework",
                    "Framework",
                    "",
                    vec![
                        choice("", "The SDK's default"),
                        choice("net10.0", ".NET 10"),
                        choice("net9.0", ".NET 9"),
                        choice("net8.0", ".NET 8 (LTS)"),
                    ],
                ),
                GeneratorOption {
                    id: "otherTemplate".into(),
                    label: "Other template (short name)".into(),
                    kind: "string".into(),
                    default: json!(""),
                    choices: Vec::new(),
                    description: Some("Any template from dotnet new list, e.g. grpc or avalonia.app; replaces the template above.".into()),
                },
                bool_opt(
                    "solution",
                    "Create solution",
                    true,
                    Some("Puts the project into src/ and adds it to a new .sln."),
                ),
                git(),
            ],
        ),
        generator(
            Def {
                id: "empty",
                name: "Empty Project",
                group: "Other",
                pack: "core",
                icon: "folder",
                default_name: "untitled",
                description: "An empty folder. Language support activates for the files you add.",
                tools: &[],
                fallback: None,
            },
            vec![git()],
        ),
    ]
}

pub fn find_generator(id: &str) -> Option<Generator> {
    generators().into_iter().find(|g| g.id == id)
}

/// What the user filled in.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    pub generator: String,
    /// Parent directory; the project is created in `location/name`.
    pub location: String,
    pub name: String,
    #[serde(default)]
    pub options: BTreeMap<String, Value>,
}

impl CreateRequest {
    fn bool(&self, gen: &Generator, id: &str) -> bool {
        self.options
            .get(id)
            .and_then(Value::as_bool)
            .or_else(|| gen.options.iter().find(|o| o.id == id)?.default.as_bool())
            .unwrap_or(false)
    }

    fn string(&self, gen: &Generator, id: &str) -> String {
        self.options
            .get(id)
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                gen.options
                    .iter()
                    .find(|o| o.id == id)?
                    .default
                    .as_str()
                    .map(str::to_string)
            })
            .unwrap_or_default()
    }

    /// Options with the generator defaults filled in.
    fn effective_options(&self, gen: &Generator) -> BTreeMap<String, Value> {
        let mut out: BTreeMap<String, Value> = gen
            .options
            .iter()
            .map(|o| (o.id.clone(), o.default.clone()))
            .collect();
        for (k, v) in &self.options {
            if gen.options.iter().any(|o| &o.id == k) {
                out.insert(k.clone(), v.clone());
            }
        }
        out
    }
}

/// One thing the creation does.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Step {
    /// Run a program (no shell) in `cwd`.
    #[serde(rename_all = "camelCase")]
    Run {
        program: String,
        args: Vec<String>,
        cwd: PathBuf,
    },
    /// Write a built-in template into the project directory.
    #[serde(rename_all = "camelCase")]
    Template {
        template: String,
        options: BTreeMap<String, Value>,
        sample: bool,
    },
    /// `git init` in the project directory.
    GitInit,
}

impl Step {
    /// How the step reads in the preview and progress view.
    pub fn display(&self, name: &str) -> String {
        match self {
            Step::Run { program, args, .. } => {
                let mut s = program.clone();
                for a in args {
                    s.push(' ');
                    if a.contains(' ') {
                        s.push_str(&format!("\"{a}\""));
                    } else {
                        s.push_str(a);
                    }
                }
                s
            }
            Step::Template { .. } => format!("Create {name}/ from the built-in template"),
            Step::GitInit => "git init".into(),
        }
    }
}

/// The concrete steps for a request, plus what is missing to run them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Absolute project directory (`location/name`).
    pub target: PathBuf,
    pub steps: Vec<Step>,
    /// Command lines as shown in the preview, one per step.
    pub preview: Vec<String>,
    /// Files a built-in template writes (relative, `/`-separated), for the preview.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// Tools that are needed but not installed; the plan cannot run while non-empty.
    pub missing_tools: Vec<String>,
    /// Offline generator id the UI can offer instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    /// Problem with the name or location, shown under the form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Check a project folder name.
pub fn validate_name(name: &str) -> std::result::Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a project name".into());
    }
    if name == "." || name == ".." {
        return Err("Invalid project name".into());
    }
    if let Some(c) = name.chars().find(|c| {
        matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()
    }) {
        return Err(format!("The name must not contain '{c}'"));
    }
    Ok(())
}

/// Expand a leading `~` to the home directory.
pub fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return dirs_home().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        if let Some(home) = dirs_home() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn run(program: &str, args: &[&str], cwd: &Path) -> Step {
    Step::Run {
        program: program.into(),
        args: args.iter().map(|a| a.to_string()).collect(),
        cwd: cwd.to_path_buf(),
    }
}

/// Build the plan. `has_tool` says whether an executable is installed.
pub fn plan(req: &CreateRequest, has_tool: &dyn Fn(&str) -> bool) -> Result<Plan> {
    let gen = find_generator(&req.generator)
        .ok_or_else(|| ForgeError::Other(format!("unknown generator {}", req.generator)))?;
    let name = req.name.trim().to_string();
    let location = expand_home(req.location.trim());
    let target = location.join(&name);
    let mut error = validate_name(&name).err();
    if error.is_none() && req.location.trim().is_empty() {
        error = Some("Choose a location".into());
    }
    if error.is_none() && target.exists() && !is_empty_dir(&target) {
        error = Some(format!(
            "Directory {} already exists and is not empty",
            target.display()
        ));
    }

    let loc = location.as_path();
    let n = name.as_str();
    let tgt = target.as_path();
    let mut steps = Vec::new();
    let mut missing = Vec::new();
    let mut need = |tool: &str| {
        if !has_tool(tool) && !missing.iter().any(|m| m == tool) {
            missing.push(tool.to_string());
        }
    };
    let install = req.bool(&gen, "install");
    let ts = req.bool(&gen, "typescript");

    match gen.id.as_str() {
        "laravel" => {
            need("composer");
            let package = match req.string(&gen, "starterKit").as_str() {
                "react" => "laravel/react-starter-kit",
                "vue" => "laravel/vue-starter-kit",
                "livewire" => "laravel/livewire-starter-kit",
                _ => "laravel/laravel",
            };
            steps.push(run(
                "composer",
                &["create-project", package, n, "--no-interaction"],
                loc,
            ));
        }
        "symfony" => {
            let webapp = req.string(&gen, "variant") != "skeleton";
            if has_tool("symfony") {
                let mut args = vec!["new", n, "--no-git"];
                if webapp {
                    args.push("--webapp");
                }
                steps.push(run("symfony", &args, loc));
            } else {
                need("composer");
                steps.push(run(
                    "composer",
                    &["create-project", "symfony/skeleton", n, "--no-interaction"],
                    loc,
                ));
                if webapp {
                    steps.push(run(
                        "composer",
                        &["require", "webapp", "--no-interaction"],
                        tgt,
                    ));
                }
            }
        }
        "vite" | "react" | "vue" => {
            need("npm");
            let template = match (gen.id.as_str(), ts) {
                ("vite", _) => "vanilla-ts",
                ("react", true) => "react-ts",
                ("react", false) => "react",
                ("vue", true) => "vue-ts",
                _ => "vue",
            };
            steps.push(run(
                "npm",
                &[
                    "create",
                    "vite@latest",
                    n,
                    "--",
                    "--template",
                    template,
                    "--no-interactive",
                ],
                loc,
            ));
            if install {
                steps.push(run("npm", &["install"], tgt));
            }
        }
        "angular" => {
            need("npx");
            let mut args = vec![
                "-y",
                "@angular/cli@latest",
                "new",
                n,
                "--defaults",
                "--skip-git",
            ];
            if !install {
                args.push("--skip-install");
            }
            steps.push(run("npx", &args, loc));
        }
        "svelte" => {
            need("npx");
            let mut args = vec!["-y", "sv", "create", n, "--template", "minimal"];
            let types: &[&str] = if ts {
                &["--types", "ts"]
            } else {
                &["--no-types"]
            };
            args.extend(types);
            args.push("--no-add-ons");
            if install {
                args.extend(["--install", "npm"]);
            } else {
                args.push("--no-install");
            }
            steps.push(run("npx", &args, loc));
        }
        "ruby-gem" => {
            need("bundle");
            let test = format!("--test={}", req.string(&gen, "testFramework"));
            let linter = if req.bool(&gen, "rubocop") {
                "--linter=rubocop"
            } else {
                "--linter=none"
            };
            let exe = if req.bool(&gen, "exe") {
                "--exe"
            } else {
                "--no-exe"
            };
            steps.push(run(
                "bundle",
                &[
                    "gem",
                    n,
                    &test,
                    linter,
                    exe,
                    "--no-coc",
                    "--no-mit",
                    "--no-changelog",
                    "--no-ci",
                    "--no-git",
                ],
                loc,
            ));
        }
        "dotnet" => {
            need("dotnet");
            let template = match req.string(&gen, "otherTemplate").trim() {
                "" => req.string(&gen, "template"),
                t => t.to_string(),
            };
            let language = req.string(&gen, "language");
            let framework = req.string(&gen, "framework");
            let ext = match language.as_str() {
                "F#" => "fsproj",
                "VB" => "vbproj",
                _ => "csproj",
            };
            let solution = req.bool(&gen, "solution");
            let out_dir = if solution {
                format!("src/{n}")
            } else {
                n.to_string()
            };
            let mut args: Vec<String> = vec![
                "new".into(),
                template.clone(),
                "-n".into(),
                n.into(),
                "-o".into(),
                out_dir.clone(),
            ];
            if !language.is_empty() && language != "C#" {
                args.extend(["-lang".into(), language.clone()]);
            }
            if !framework.is_empty() {
                args.extend(["--framework".into(), framework]);
            }
            let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
            if solution {
                steps.push(run("dotnet", &["new", "sln", "-n", n, "-o", n], loc));
                steps.push(run("dotnet", &args_ref, tgt));
                let proj = format!("{out_dir}/{n}.{ext}");
                steps.push(run("dotnet", &["sln", "add", &proj], tgt));
            } else {
                steps.push(run("dotnet", &args_ref, loc));
            }
        }
        template => {
            let sample = gen.options.iter().any(|o| o.id == "sample") && req.bool(&gen, "sample")
                || !gen.options.iter().any(|o| o.id == "sample");
            steps.push(Step::Template {
                template: template.to_string(),
                options: req.effective_options(&gen),
                sample,
            });
        }
    }
    if req.bool(&gen, "git") {
        need("git");
        steps.push(Step::GitInit);
    }

    let preview = steps.iter().map(|s| s.display(n)).collect();
    let mut files = Vec::new();
    for step in &steps {
        if let Step::Template {
            template,
            options,
            sample,
        } = step
        {
            let vars = crate::templates::TemplateVars::new(n, options);
            let rendered = crate::templates::render_template(template, &vars, *sample)?;
            files.extend(
                rendered
                    .files
                    .iter()
                    .map(|(p, _)| p.to_string_lossy().replace('\\', "/")),
            );
        }
    }
    let fallback = if missing.iter().any(|m| m != "git") {
        gen.fallback.clone()
    } else {
        None
    };
    Ok(Plan {
        target,
        steps,
        preview,
        files,
        missing_tools: missing,
        fallback,
        error,
    })
}

fn is_empty_dir(path: &Path) -> bool {
    std::fs::read_dir(path)
        .map(|mut it| it.next().is_none())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(generator: &str, options: &[(&str, Value)]) -> CreateRequest {
        CreateRequest {
            generator: generator.into(),
            location: "/projects".into(),
            name: "demo".into(),
            options: options
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        }
    }

    fn all_tools(_: &str) -> bool {
        true
    }

    #[test]
    fn generator_list_is_grouped_and_templates_exist() {
        let gens = generators();
        let groups: Vec<_> = gens.iter().map(|g| g.group.as_str()).collect();
        let mut dedup = groups.clone();
        dedup.dedup();
        assert_eq!(dedup, vec!["PHP", "Web", "Lua", "Ruby", ".NET", "Other"]);
        for g in gens.iter().filter(|g| g.tools.is_empty()) {
            assert!(
                crate::templates::template(&g.id).is_some(),
                "{} has no template",
                g.id
            );
        }
        for g in &gens {
            if let Some(f) = &g.fallback {
                assert!(find_generator(f).unwrap().tools.is_empty());
            }
        }
    }

    #[test]
    fn dotnet_projects() {
        let p = plan(&req("dotnet", &[]), &all_tools).unwrap();
        assert_eq!(
            p.preview,
            vec![
                "dotnet new sln -n demo -o demo",
                "dotnet new console -n demo -o src/demo",
                "dotnet sln add src/demo/demo.csproj",
            ]
        );
        let p = plan(
            &req(
                "dotnet",
                &[
                    ("template", json!("classlib")),
                    ("language", json!("F#")),
                    ("framework", json!("net8.0")),
                    ("solution", json!(false)),
                ],
            ),
            &all_tools,
        )
        .unwrap();
        assert_eq!(
            p.preview,
            vec!["dotnet new classlib -n demo -o demo -lang F# --framework net8.0"]
        );
        let other = plan(
            &req(
                "dotnet",
                &[("otherTemplate", json!("grpc")), ("solution", json!(false))],
            ),
            &all_tools,
        )
        .unwrap();
        assert!(other.preview[0].starts_with("dotnet new grpc "));
        let missing = plan(&req("dotnet", &[]), &|t| t != "dotnet").unwrap();
        assert_eq!(missing.missing_tools, vec!["dotnet"]);
    }

    #[test]
    fn laravel_starter_kits() {
        let p = plan(&req("laravel", &[]), &all_tools).unwrap();
        assert_eq!(
            p.preview,
            vec!["composer create-project laravel/laravel demo --no-interaction"]
        );
        assert_eq!(p.target, PathBuf::from("/projects/demo"));
        let p = plan(
            &req(
                "laravel",
                &[("starterKit", json!("vue")), ("git", json!(true))],
            ),
            &all_tools,
        )
        .unwrap();
        assert_eq!(
            p.preview,
            vec![
                "composer create-project laravel/vue-starter-kit demo --no-interaction",
                "git init"
            ]
        );
    }

    #[test]
    fn missing_composer_offers_offline_template() {
        let p = plan(&req("laravel", &[]), &|t| t != "composer").unwrap();
        assert_eq!(p.missing_tools, vec!["composer"]);
        assert_eq!(p.fallback.as_deref(), Some("empty-php"));
    }

    #[test]
    fn symfony_prefers_symfony_cli_and_falls_back_to_composer() {
        let p = plan(&req("symfony", &[]), &all_tools).unwrap();
        assert_eq!(p.preview, vec!["symfony new demo --no-git --webapp"]);
        let p = plan(&req("symfony", &[]), &|t| t != "symfony").unwrap();
        assert_eq!(
            p.preview,
            vec![
                "composer create-project symfony/skeleton demo --no-interaction",
                "composer require webapp --no-interaction"
            ]
        );
        assert!(p.missing_tools.is_empty());
        let p = plan(&req("symfony", &[("variant", json!("skeleton"))]), &|t| {
            t != "symfony"
        })
        .unwrap();
        assert_eq!(p.steps.len(), 1);
    }

    #[test]
    fn web_generators() {
        let p = plan(&req("react", &[("typescript", json!(false))]), &all_tools).unwrap();
        assert_eq!(
            p.preview,
            vec![
                "npm create vite@latest demo -- --template react --no-interactive",
                "npm install"
            ]
        );
        let p = plan(&req("angular", &[("install", json!(false))]), &all_tools).unwrap();
        assert_eq!(
            p.preview,
            vec!["npx -y @angular/cli@latest new demo --defaults --skip-git --skip-install"]
        );
        let p = plan(&req("svelte", &[]), &all_tools).unwrap();
        assert_eq!(
            p.preview,
            vec!["npx -y sv create demo --template minimal --types ts --no-add-ons --install npm"]
        );
        let p = plan(&req("vue", &[]), &|_| false).unwrap();
        assert_eq!(p.fallback.as_deref(), Some("empty-web"));
    }

    #[test]
    fn template_generators_need_no_tools() {
        let p = plan(&req("love-game", &[("sample", json!(false))]), &|_| false).unwrap();
        assert!(p.missing_tools.is_empty());
        assert_eq!(
            p.steps,
            vec![Step::Template {
                template: "love-game".into(),
                options: [
                    ("git".to_string(), json!(false)),
                    ("sample".to_string(), json!(false))
                ]
                .into_iter()
                .collect(),
                sample: false,
            }]
        );
        // Without a "sample" option the template is written in full.
        let p = plan(&req("wordpress-plugin", &[]), &|_| false).unwrap();
        assert!(matches!(&p.steps[0], Step::Template { sample: true, .. }));
        assert_eq!(p.preview, vec!["Create demo/ from the built-in template"]);
        assert!(p.files.contains(&"includes/class-demo.php".to_string()));
    }

    #[test]
    fn ruby_generators() {
        let p = plan(
            &req("ruby-gem", &[("testFramework", json!("rspec"))]),
            &all_tools,
        )
        .unwrap();
        assert_eq!(
            p.preview[0],
            "bundle gem demo --test=rspec --linter=rubocop --no-exe --no-coc --no-mit --no-changelog --no-ci --no-git"
        );
        let p = plan(&req("ruby-gem", &[]), &|t| t == "git").unwrap();
        assert_eq!(p.missing_tools, ["bundle"]);
        assert_eq!(p.fallback.as_deref(), Some("ruby-script"));
        let p = plan(&req("ruby-script", &[]), &all_tools).unwrap();
        assert!(
            p.files.contains(&"lib/demo.rb".to_string()),
            "{:?}",
            p.files
        );
        assert!(p.files.contains(&"test/greeter_test.rb".to_string()));
        assert!(p.files.contains(&"bin/demo".to_string()));
    }

    #[test]
    fn git_needs_git() {
        let p = plan(&req("empty", &[("git", json!(true))]), &|t| t != "git").unwrap();
        assert_eq!(p.missing_tools, vec!["git"]);
        assert_eq!(p.fallback, None);
    }

    #[test]
    fn validates_name_and_location() {
        assert!(validate_name("my-app").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("..").is_err());

        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("taken")).unwrap();
        std::fs::write(dir.path().join("taken/file"), "").unwrap();
        std::fs::create_dir(dir.path().join("empty")).unwrap();
        let mut r = req("empty", &[]);
        r.location = dir.path().display().to_string();
        r.name = "taken".into();
        assert!(plan(&r, &all_tools)
            .unwrap()
            .error
            .unwrap()
            .contains("not empty"));
        r.name = "empty".into();
        assert_eq!(plan(&r, &all_tools).unwrap().error, None);
        r.location = " ".into();
        assert_eq!(
            plan(&r, &all_tools).unwrap().error.as_deref(),
            Some("Choose a location")
        );
    }

    #[test]
    fn expands_home() {
        std::env::set_var("HOME", "/home/test");
        assert_eq!(
            expand_home("~/Projects"),
            PathBuf::from("/home/test/Projects")
        );
        assert_eq!(expand_home("/abs"), PathBuf::from("/abs"));
    }
}
