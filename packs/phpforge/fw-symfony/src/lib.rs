//! PHPForge Symfony plugin: everything the IDE knows about a Symfony application.
//!
//! - [`container`] reads `config/services.yaml` (explicit services, aliases, `_defaults` and
//!   autowired `resource:` directories) and, when present, the compiled container XML in
//!   `var/cache/<env>/`, which also lists the services and parameters of all bundles.
//! - [`routes`] collects routes from `#[Route]` attributes, legacy `@Route` annotations and
//!   YAML route files.
//! - [`twig`] lists templates, Twig tags/filters/functions/tests (core, Symfony bridge and
//!   project extensions) and which controller renders which template.
//! - [`console`] finds console commands and builds `bin/console` command lines.
//! - [`complete`] answers completion and Go to Declaration for route names, service ids,
//!   templates, parameters and Twig syntax; [`gutter`] computes the editor gutter marks.
//!
//! [`scan`] builds a [`Snapshot`] of a project once; the IDE caches it and rescans when files
//! change.

pub mod complete;
pub mod console;
pub mod container;
pub mod gutter;
pub mod routes;
pub mod strings;
pub mod twig;

use std::path::{Path, PathBuf};

use phpforge_lang_php::parser;
use phpforge_lang_php::FileIndex;
use serde::{Deserialize, Serialize};

pub use complete::{CompletionItem, CompletionKind, CompletionList};
pub use console::ConsoleCommand;
pub use container::{Parameter, Service, ServiceSource};
pub use routes::{Route, RouteSource};
pub use twig::{Template, TwigSymbol, TwigSymbolKind};

/// A position in a project file. `path` is relative to the project root with `/`
/// separators; `line` and `column` are 0-based (column in bytes).
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

/// Plugin settings (`symfony.*`), with the defaults of a standard Symfony Flex project.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// Application sources, scanned for controllers, commands and Twig extensions.
    pub app_directory: String,
    pub templates_directory: String,
    pub console_path: String,
    /// Kernel environment whose compiled container is read (`var/cache/<env>`).
    pub environment: String,
    /// Explicit container XML; empty means "find it in var/cache".
    pub container_xml: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            app_directory: "src".into(),
            templates_directory: "templates".into(),
            console_path: "bin/console".into(),
            environment: "dev".into(),
            container_xml: String::new(),
        }
    }
}

/// Summary for the status bar and the tool window header.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymfonyInfo {
    /// `7.2.3`, from composer.lock (or the composer.json constraint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub environment: String,
    pub console_path: String,
    /// Whether `bin/console` exists.
    pub has_console: bool,
    /// Compiled container used for services and parameters, relative to the root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_xml: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_class: Option<String>,
    pub routes: usize,
    pub services: usize,
    pub templates: usize,
    pub commands: usize,
}

/// One indexed PHP file of the application.
#[derive(Debug, Clone)]
pub struct PhpFile {
    pub path: String,
    pub source: String,
    pub index: FileIndex,
}

/// Everything known about a Symfony project at one point in time.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub root: PathBuf,
    pub config: Config,
    pub info: SymfonyInfo,
    pub services: Vec<Service>,
    pub parameters: Vec<Parameter>,
    pub routes: Vec<Route>,
    pub templates: Vec<Template>,
    pub twig: Vec<TwigSymbol>,
    /// Template name -> controller actions that render it.
    pub renders: Vec<twig::RenderRef>,
    pub commands: Vec<ConsoleCommand>,
    pub php: Vec<PhpFile>,
}

impl Snapshot {
    pub fn route(&self, name: &str) -> Option<&Route> {
        self.routes.iter().find(|r| r.name == name)
    }

    pub fn service(&self, id: &str) -> Option<&Service> {
        let id = id.trim_start_matches('@');
        self.services
            .iter()
            .find(|s| s.id == id)
            .or_else(|| self.services.iter().find(|s| s.id.eq_ignore_ascii_case(id)))
    }

    pub fn parameter(&self, name: &str) -> Option<&Parameter> {
        self.parameters
            .iter()
            .find(|p| p.name == name.trim_matches('%'))
    }

    pub fn template(&self, name: &str) -> Option<&Template> {
        self.templates.iter().find(|t| t.name == name)
    }

    /// Declaration of a class in the application sources.
    pub fn class_location(&self, fqn: &str) -> Option<Location> {
        let fqn = fqn.trim_start_matches('\\');
        self.php.iter().find_map(|f| {
            f.index
                .classes
                .iter()
                .find(|c| c.fqn.eq_ignore_ascii_case(fqn))
                .map(|c| Location::new(&f.path, c.name_range.line, c.name_range.column))
        })
    }

    /// Declaration of `Class::method` (controller actions), else of the class.
    pub fn method_location(&self, controller: &str) -> Option<Location> {
        let (class, method) = controller
            .split_once("::")
            .unwrap_or((controller, "__invoke"));
        let class = class.trim_start_matches('\\');
        for f in &self.php {
            for c in &f.index.classes {
                if !c.fqn.eq_ignore_ascii_case(class) {
                    continue;
                }
                if let Some(m) = c.method(method) {
                    return Some(Location::new(
                        &f.path,
                        m.name_range.line,
                        m.name_range.column,
                    ));
                }
                return Some(Location::new(
                    &f.path,
                    c.name_range.line,
                    c.name_range.column,
                ));
            }
        }
        None
    }
}

/// Is this a Symfony project (has the framework bundle, a kernel or `bin/console`)?
pub fn is_symfony_project(root: &Path) -> bool {
    framework_version(root).is_some()
        || root.join("config/bundles.php").is_file()
        || (root.join("bin/console").is_file() && root.join("src/Kernel.php").is_file())
}

/// Installed Symfony version from composer.lock, else the composer.json constraint.
pub fn framework_version(root: &Path) -> Option<String> {
    const PACKAGES: [&str; 3] = [
        "symfony/framework-bundle",
        "symfony/http-kernel",
        "symfony/symfony",
    ];
    if let Ok(text) = std::fs::read_to_string(root.join("composer.lock")) {
        if let Ok(lock) = serde_json::from_str::<serde_json::Value>(&text) {
            for key in ["packages", "packages-dev"] {
                let Some(list) = lock.get(key).and_then(|v| v.as_array()) else {
                    continue;
                };
                for name in PACKAGES {
                    if let Some(v) = list
                        .iter()
                        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
                        .and_then(|p| p.get("version"))
                        .and_then(|v| v.as_str())
                    {
                        return Some(v.trim_start_matches('v').to_string());
                    }
                }
            }
        }
    }
    let text = std::fs::read_to_string(root.join("composer.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    for key in ["require", "require-dev"] {
        for name in PACKAGES {
            if let Some(c) = json
                .get(key)
                .and_then(|r| r.get(name))
                .and_then(|v| v.as_str())
            {
                let v: String = c
                    .trim_start_matches(['^', '~', '>', '=', 'v', ' '])
                    .chars()
                    .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
                    .collect();
                let v = v.trim_end_matches('.').to_string();
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// Relative `/`-separated path of `path` below `root`.
pub(crate) fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 0-based line and byte column of `offset` in `text`.
pub fn line_col(text: &str, offset: usize) -> (u32, u32) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32;
    let col = before.rfind('\n').map(|i| offset - i - 1).unwrap_or(offset) as u32;
    (line, col)
}

/// Directories never scanned.
pub(crate) fn skip_dir(name: &str) -> bool {
    matches!(
        name,
        "vendor" | "node_modules" | "var" | ".git" | ".idea" | ".forge"
    )
}

/// PHP files of the application directory, indexed.
pub fn index_php_files(root: &Path, dir: &str) -> Vec<PhpFile> {
    let base = root.join(dir);
    if !base.is_dir() {
        return Vec::new();
    }
    let mut parser = parser::new_parser();
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(&base)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| !(e.file_type().is_dir() && skip_dir(&e.file_name().to_string_lossy())))
        .flatten()
    {
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|e| e.to_str()) != Some("php")
        {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let path = rel_path(root, entry.path());
        let index = parser::index_with(&mut parser, &path, &source);
        out.push(PhpFile {
            path,
            source,
            index,
        });
    }
    out
}

/// Scan a project: container, routes, templates, Twig symbols and console commands.
pub fn scan(root: &Path, config: &Config) -> Snapshot {
    let php = index_php_files(root, &config.app_directory);
    let container_xml = container::find_container_xml(root, config);
    let (mut services, mut parameters) = container::from_yaml(root, &php);
    if let Some(xml) = &container_xml {
        if let Ok(text) = std::fs::read_to_string(root.join(xml)) {
            let (s, p) = container::parse_container_xml(&text);
            container::merge(&mut services, s, &mut parameters, p);
        }
    }
    container::add_kernel_parameters(&mut parameters);
    services.sort_by(|a, b| a.id.cmp(&b.id));
    parameters.sort_by(|a, b| a.name.cmp(&b.name));

    let mut routes = routes::from_php(&php);
    routes::from_yaml_files(root, &mut routes);
    routes.sort_by(|a, b| a.path.cmp(&b.path).then(a.name.cmp(&b.name)));

    let templates = twig::templates(root, &config.templates_directory);
    let mut twig_symbols = twig::builtin_symbols();
    twig_symbols.extend(twig::project_extensions(&php));
    let renders = twig::render_refs(&php);
    let commands = console::commands(&php);

    let kernel_class = php
        .iter()
        .flat_map(|f| f.index.classes.iter())
        .find(|c| c.extends.iter().any(|e| e.ends_with("HttpKernel\\Kernel")) || c.name == "Kernel")
        .map(|c| c.fqn.clone());
    let info = SymfonyInfo {
        version: framework_version(root),
        environment: config.environment.clone(),
        console_path: config.console_path.clone(),
        has_console: root.join(&config.console_path).is_file(),
        container_xml,
        kernel_class,
        routes: routes.len(),
        services: services.len(),
        templates: templates.len(),
        commands: commands.len(),
    };
    Snapshot {
        root: root.to_path_buf(),
        config: config.clone(),
        info,
        services,
        parameters,
        routes,
        templates,
        twig: twig_symbols,
        renders,
        commands,
        php,
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    //! A small Symfony project on disk, shared by the module tests.
    use std::fs;
    use std::path::Path;

    pub const CONTROLLER: &str = r#"<?php

namespace App\Controller;

use App\Entity\Order;
use App\Service\Checkout;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Attribute\Route;

#[Route('/orders', name: 'order_')]
final class OrderController extends AbstractController
{
    #[Route('', name: 'index', methods: ['GET'])]
    public function index(): Response
    {
        return $this->render('order/index.html.twig', ['orders' => []]);
    }

    #[Route('/{id}', name: 'show', methods: ['GET'], requirements: ['id' => '\d+'])]
    public function show(Order $order): Response
    {
        return $this->render('order/show.html.twig', ['order' => $order]);
    }

    #[Route('/{id}/pay', name: 'pay', methods: ['POST'])]
    public function pay(Order $order, Checkout $checkout): Response
    {
        $checkout->pay($order);
        return $this->redirectToRoute('order_show', ['id' => $order->getId()]);
    }
}
"#;

    pub const LEGACY_CONTROLLER: &str = r#"<?php

namespace App\Controller;

use Symfony\Component\Routing\Annotation\Route;

class LegacyController
{
    /**
     * @Route("/legacy/report", name="legacy_report", methods={"GET","HEAD"})
     */
    public function report()
    {
    }
}
"#;

    pub const COMMAND: &str = r#"<?php

namespace App\Command;

use Symfony\Component\Console\Attribute\AsCommand;
use Symfony\Component\Console\Command\Command;

#[AsCommand(name: 'app:orders:purge', description: 'Delete abandoned orders')]
final class PurgeOrdersCommand extends Command
{
}
"#;

    pub const OLD_COMMAND: &str = r#"<?php

namespace App\Command;

use Symfony\Component\Console\Command\Command;

class ImportCommand extends Command
{
    protected static $defaultName = 'app:import';

    protected function configure(): void
    {
        $this->setDescription('Import products from a CSV file')->setAliases(['app:imp']);
    }
}
"#;

    pub const CHECKOUT: &str = r#"<?php

namespace App\Service;

class Checkout
{
    public function __construct(private string $currency) {}
}
"#;

    pub const MAILER: &str = r#"<?php

namespace App\Service;

class Mailer {}
"#;

    pub const TWIG_EXT: &str = r#"<?php

namespace App\Twig;

use Twig\Extension\AbstractExtension;
use Twig\TwigFilter;
use Twig\TwigFunction;

class AppExtension extends AbstractExtension
{
    public function getFilters(): array
    {
        return [
            new TwigFilter('price', [$this, 'formatPrice']),
        ];
    }

    public function getFunctions(): array
    {
        return [new TwigFunction('cart_count', [$this, 'cartCount'])];
    }
}
"#;

    pub const ENTITY: &str = r#"<?php

namespace App\Entity;

class Order {}
"#;

    pub const SERVICES: &str = r#"parameters:
    app.currency: 'EUR'
    app.supported_locales: ['en', 'de']

services:
    _defaults:
        autowire: true
        autoconfigure: true

    App\:
        resource: '../src/'
        exclude:
            - '../src/DependencyInjection/'
            - '../src/Entity/'
            - '../src/Kernel.php'

    App\Service\Checkout:
        arguments:
            $currency: '%app.currency%'

    app.mailer:
        class: App\Service\Mailer
        public: true

    mailer.default: '@app.mailer'
"#;

    pub const ROUTES: &str = r#"controllers:
    resource:
        path: ../src/Controller/
        namespace: App\Controller
    type: attribute

home:
    path: /
    controller: Symfony\Bundle\FrameworkBundle\Controller\TemplateController
    defaults:
        template: home.html.twig

api_health:
    path: /api/health
    controller: App\Controller\HealthController::check
    methods: [GET]
"#;

    pub const CONTAINER_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<container xmlns="http://symfony.com/schema/dic/services">
  <parameters>
    <parameter key="kernel.project_dir">/app</parameter>
    <parameter key="kernel.environment">dev</parameter>
    <parameter key="app.currency">EUR</parameter>
    <parameter key="locales" type="collection">
      <parameter>en</parameter>
    </parameter>
  </parameters>
  <services>
    <service id="service_container" class="Symfony\Component\DependencyInjection\ContainerInterface" public="true" synthetic="true"/>
    <service id="router" class="Symfony\Bundle\FrameworkBundle\Routing\Router" public="true">
      <tag name="monolog.logger" channel="router"/>
    </service>
    <service id="twig" class="Twig\Environment" public="true"/>
    <service id=".abstract.instanceof.App\Service\Checkout" class="App\Service\Checkout" abstract="true"/>
    <service id="App\Service\Checkout" class="App\Service\Checkout" autowire="true"/>
    <service id="Twig\Environment" alias="twig"/>
  </services>
</container>
"#;

    pub fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    /// Write the fixture project and return its temp dir.
    pub fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        write(
            r,
            "composer.lock",
            r#"{"packages":[{"name":"symfony/framework-bundle","version":"v7.2.3"}]}"#,
        );
        write(r, "bin/console", "#!/usr/bin/env php\n<?php\n");
        write(r, "config/bundles.php", "<?php return [];\n");
        write(r, "config/services.yaml", SERVICES);
        write(r, "config/routes.yaml", ROUTES);
        write(r, "src/Kernel.php", "<?php\nnamespace App;\nuse Symfony\\Component\\HttpKernel\\Kernel as BaseKernel;\nclass Kernel extends BaseKernel {}\n");
        write(r, "src/Controller/OrderController.php", CONTROLLER);
        write(r, "src/Controller/LegacyController.php", LEGACY_CONTROLLER);
        write(r, "src/Command/PurgeOrdersCommand.php", COMMAND);
        write(r, "src/Command/ImportCommand.php", OLD_COMMAND);
        write(r, "src/Service/Checkout.php", CHECKOUT);
        write(r, "src/Service/Mailer.php", MAILER);
        write(r, "src/Twig/AppExtension.php", TWIG_EXT);
        write(r, "src/Entity/Order.php", ENTITY);
        write(r, "templates/base.html.twig", "<html>{% block body %}{% endblock %}</html>\n");
        write(r, "templates/order/index.html.twig", "{% extends 'base.html.twig' %}\n");
        write(r, "templates/order/show.html.twig", "{% extends 'base.html.twig' %}\n{% block body %}{{ order.total|price }}{% endblock %}\n");
        dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_version_and_scans_everything() {
        let dir = fixture::project();
        assert!(is_symfony_project(dir.path()));
        let snap = scan(dir.path(), &Config::default());
        assert_eq!(snap.info.version.as_deref(), Some("7.2.3"));
        assert!(snap.info.has_console);
        assert_eq!(snap.info.kernel_class.as_deref(), Some("App\\Kernel"));
        assert!(snap.route("order_show").is_some());
        assert!(snap.service("app.mailer").is_some());
        assert!(snap.template("order/show.html.twig").is_some());
        assert!(snap.commands.iter().any(|c| c.name == "app:orders:purge"));
        assert_eq!(
            snap.method_location("App\\Controller\\OrderController::show")
                .unwrap()
                .path,
            "src/Controller/OrderController.php"
        );
    }

    #[test]
    fn version_from_composer_json_constraint() {
        let dir = tempfile::tempdir().unwrap();
        fixture::write(
            dir.path(),
            "composer.json",
            r#"{"require":{"symfony/framework-bundle":"^6.4"}}"#,
        );
        assert_eq!(framework_version(dir.path()).as_deref(), Some("6.4"));
    }

    #[test]
    fn line_col_counts_bytes() {
        assert_eq!(line_col("ab\ncd", 4), (1, 1));
        assert_eq!(line_col("ab", 1), (0, 1));
    }
}
