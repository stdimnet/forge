/**
 * Browser-mode Symfony backend over the Symfony demo project (`?project=symfony`). Data
 * mirrors what `phpforge-fw-symfony` reports for those files; completion and navigation
 * use simplified versions of its context rules. Console runs are simulated.
 */
import { emitMock, registerMock } from "@/ipc";
import {
  CONSOLE_EVENT,
  type ConsoleCommand,
  type SourceLocation,
  type SymfonyCompletionItem,
  type SymfonyCompletionKind,
  type SymfonyGutterMark,
  type SymfonyInfo,
  type SymfonyParameter,
  type SymfonyRoute,
  type SymfonyService,
  type SymfonyTemplate,
} from "@/ipc/symfony";
import { isSymfonyDemo, symfonyDemoFiles, symfonyDemoInfo, symfonyDemoName } from "./demoSymfonyProject";
import { setSetting } from "./settingsMock";

const files = symfonyDemoFiles;

/** 0-based line of the first occurrence of `needle` in a demo file. */
export function lineOf(path: string, needle: string): number {
  const src = files[path] ?? "";
  const i = src.indexOf(needle);
  return i < 0 ? 0 : src.slice(0, i).split("\n").length - 1;
}

const at = (path: string, needle: string): SourceLocation => ({ path, line: lineOf(path, needle) });

const OC = "src/Controller/OrderController.php";
const PC = "src/Controller/ProductController.php";

const routes: SymfonyRoute[] = [
  { name: "admin_dashboard", path: "/admin", methods: [], controller: "App\\Controller\\Admin\\DashboardController::index", source: "attribute", location: at("src/Controller/Admin/DashboardController.php", "function index") },
  { name: "health", path: "/health", methods: ["GET"], controller: "App\\Controller\\HealthController::check", source: "yaml", location: at("config/routes.yaml", "health:") },
  { name: "order_index", path: "/orders", methods: ["GET"], controller: "App\\Controller\\OrderController::index", source: "attribute", location: at(OC, "function index") },
  { name: "order_show", path: "/orders/{id}", methods: ["GET"], controller: "App\\Controller\\OrderController::show", source: "attribute", location: at(OC, "function show") },
  { name: "order_pay", path: "/orders/{id}/pay", methods: ["POST"], controller: "App\\Controller\\OrderController::pay", source: "attribute", location: at(OC, "function pay") },
  { name: "product_list", path: "/products", methods: ["GET"], controller: "App\\Controller\\ProductController::list", source: "attribute", location: at(PC, "function list") },
  { name: "product_show", path: "/products/{slug}", methods: ["GET"], controller: "App\\Controller\\ProductController::show", source: "attribute", location: at(PC, "function show") },
  { name: "_preview_error", path: "/_error/{code}.{_format}", methods: [], controller: "error_controller::preview", source: "yaml" },
  { name: "_wdt", path: "/_wdt/{token}", methods: [], controller: "web_profiler.controller.profiler::toolbarAction", source: "yaml" },
  { name: "_profiler", path: "/_profiler/{token}", methods: [], controller: "web_profiler.controller.profiler::panelAction", source: "yaml" },
];

const appClass = (fqn: string, path: string): SymfonyService => ({
  id: fqn,
  class: fqn,
  public: false,
  autowire: true,
  autoconfigure: true,
  source: "resource",
  location: at(path, `class ${fqn.split("\\").pop()}`),
});

const vendor = (id: string, cls: string, extra: Partial<SymfonyService> = {}): SymfonyService => ({
  id,
  class: cls,
  public: false,
  autowire: false,
  autoconfigure: false,
  source: "container",
  ...extra,
});

const services: SymfonyService[] = [
  { ...appClass("App\\Command\\ImportProductsCommand", "src/Command/ImportProductsCommand.php"), tags: ["console.command"] },
  { ...appClass("App\\Command\\PurgeOrdersCommand", "src/Command/PurgeOrdersCommand.php"), tags: ["console.command"] },
  { ...appClass("App\\Controller\\Admin\\DashboardController", "src/Controller/Admin/DashboardController.php"), tags: ["controller.service_arguments"] },
  { ...appClass("App\\Controller\\HealthController", "src/Controller/HealthController.php"), tags: ["controller.service_arguments"] },
  { ...appClass("App\\Controller\\OrderController", OC), tags: ["controller.service_arguments"] },
  { ...appClass("App\\Controller\\ProductController", PC), tags: ["controller.service_arguments"] },
  { ...appClass("App\\Repository\\CustomerRepository", "src/Repository/CustomerRepository.php"), tags: ["doctrine.repository_service"] },
  { ...appClass("App\\Repository\\OrderRepository", "src/Repository/OrderRepository.php"), tags: ["doctrine.repository_service"] },
  { ...appClass("App\\Repository\\ProductRepository", "src/Repository/ProductRepository.php"), tags: ["doctrine.repository_service"] },
  appClass("App\\Service\\Checkout", "src/Service/Checkout.php"),
  appClass("App\\Service\\InvoiceMailer", "src/Service/InvoiceMailer.php"),
  { ...appClass("App\\Service\\PaymentGateway", "src/Service/PaymentGateway.php"), source: "yaml" as const, location: at("config/services.yaml", "App\\Service\\PaymentGateway:") },
  { ...appClass("App\\Twig\\AppExtension", "src/Twig/AppExtension.php"), tags: ["twig.extension"] },
  { id: "app.invoice_mailer", class: "App\\Service\\InvoiceMailer", public: true, autowire: true, autoconfigure: true, source: "yaml" as const, location: at("config/services.yaml", "app.invoice_mailer:") },
  vendor("cache.app", "Symfony\\Component\\Cache\\Adapter\\FilesystemAdapter", { public: true, tags: ["cache.pool"] }),
  vendor("doctrine", "Doctrine\\Bundle\\DoctrineBundle\\Registry", { public: true }),
  vendor("doctrine.orm.default_entity_manager", "Doctrine\\ORM\\EntityManager", { public: true }),
  vendor("doctrine.orm.entity_manager", "Doctrine\\ORM\\EntityManagerInterface", { alias: "doctrine.orm.default_entity_manager", public: true }),
  vendor("event_dispatcher", "Symfony\\Component\\EventDispatcher\\EventDispatcher", { public: true }),
  vendor("form.factory", "Symfony\\Component\\Form\\FormFactory", { public: true }),
  vendor("logger", "Monolog\\Logger", { tags: ["monolog.logger"] }),
  vendor("mailer.mailer", "Symfony\\Component\\Mailer\\Mailer"),
  vendor("request_stack", "Symfony\\Component\\HttpFoundation\\RequestStack", { public: true }),
  vendor("router", "Symfony\\Bundle\\FrameworkBundle\\Routing\\Router", { public: true, tags: ["monolog.logger"] }),
  vendor("security.authorization_checker", "Symfony\\Component\\Security\\Core\\Authorization\\AuthorizationChecker", { public: true }),
  vendor("serializer", "Symfony\\Component\\Serializer\\Serializer", { public: true }),
  vendor("translator", "Symfony\\Component\\Translation\\DataCollectorTranslator", { public: true }),
  vendor("twig", "Twig\\Environment", { public: true }),
  vendor("validator", "Symfony\\Component\\Validator\\Validator\\TraceableValidator", { public: true }),
].sort((a, b) => a.id.localeCompare(b.id));

const parameters: SymfonyParameter[] = [
  { name: "app.currency", value: "EUR", location: at("config/services.yaml", "app.currency:") },
  { name: "app.invoice_dir", value: "%kernel.project_dir%/var/invoices", location: at("config/services.yaml", "app.invoice_dir:") },
  { name: "app.supported_locales", value: "[en, de, fr]", location: at("config/services.yaml", "app.supported_locales:") },
  { name: "kernel.cache_dir", value: "%kernel.project_dir%/var/cache/dev" },
  { name: "kernel.charset", value: "UTF-8" },
  { name: "kernel.debug", value: "true" },
  { name: "kernel.default_locale", value: "en" },
  { name: "kernel.environment", value: "dev" },
  { name: "kernel.logs_dir", value: "%kernel.project_dir%/var/log" },
  { name: "kernel.project_dir", value: "/home/dev/Projects/symfony-api" },
  { name: "kernel.secret", value: "%env(APP_SECRET)%" },
];

const templates: SymfonyTemplate[] = Object.keys(files)
  .filter((p) => p.startsWith("templates/") && p.endsWith(".twig"))
  .sort()
  .map((path) => ({ name: path.slice("templates/".length), path }));

const commands: ConsoleCommand[] = [
  { name: "app:orders:purge", description: "Delete abandoned orders older than 30 days", class: "App\\Command\\PurgeOrdersCommand", location: at("src/Command/PurgeOrdersCommand.php", "class PurgeOrdersCommand") },
  { name: "app:products:import", description: "Import products from a CSV file", aliases: ["app:import"], class: "App\\Command\\ImportProductsCommand", location: at("src/Command/ImportProductsCommand.php", "class ImportProductsCommand") },
  ...(
    [
      ["about", "Display information about the current project"],
      ["assets:install", "Install bundle's web assets under a public directory"],
      ["cache:clear", "Clear the cache"],
      ["cache:pool:clear", "Clear cache pools"],
      ["cache:warmup", "Warm up an empty cache"],
      ["debug:autowiring", "List classes/interfaces you can use for autowiring"],
      ["debug:config", "Dump the current configuration for an extension"],
      ["debug:container", "Display current services for an application"],
      ["debug:router", "Display current routes for an application"],
      ["debug:twig", "Show a list of twig functions, filters, globals and tests"],
      ["doctrine:database:create", "Creates the configured database"],
      ["doctrine:migrations:diff", "Generate a migration by comparing your current database to your mapping information"],
      ["doctrine:migrations:migrate", "Execute a migration to a specified version or the latest available version"],
      ["doctrine:schema:validate", "Validate the mapping files"],
      ["lint:container", "Ensure that arguments injected into services match type declarations"],
      ["lint:twig", "Lint a Twig template and outputs encountered errors"],
      ["lint:yaml", "Lint a YAML file and outputs encountered errors"],
      ["make:command", "Create a new console command class"],
      ["make:controller", "Create a new controller class"],
      ["make:entity", "Create or update a Doctrine entity class, and optionally an API Platform resource"],
      ["make:migration", "Create a new migration based on database changes"],
      ["messenger:consume", "Consume messages"],
      ["router:match", "Help debug routes by simulating a path info match"],
      ["secrets:set", "Set a secret in the vault"],
    ] as const
  ).map(([name, description]) => ({ name, description, builtin: true })),
];

interface TwigSym {
  name: string;
  kind: "tag" | "filter" | "function" | "test" | "variable";
  signature?: string;
  provider: string;
  location?: SourceLocation;
}

const tw = (kind: TwigSym["kind"], provider: string, list: string[]): TwigSym[] =>
  list.map((entry) => {
    const [name, signature] = entry.split("|");
    return { name, kind, signature, provider };
  });

const twig: TwigSym[] = [
  ...tw("tag", "Twig", ["apply|apply filter", "autoescape|autoescape 'html'", "block|block name", "cache|cache 'key'", "deprecated|deprecated 'message'", "do|do expression", "embed|embed 'template'", "extends|extends 'template'", "for|for item in items", "from|from 'template' import macro", "if|if condition", "else", "elseif|elseif condition", "import|import 'template' as name", "include|include 'template'", "macro|macro name(args)", "set|set name = value", "use|use 'template'", "verbatim", "with|with {key: value}", "endblock", "endfor", "endif", "endapply", "endembed", "endmacro", "endset", "endwith"]),
  ...tw("tag", "Symfony", ["form_theme|form_theme form 'theme.html.twig'", "trans|trans with {params} from 'domain'", "trans_default_domain|trans_default_domain 'domain'", "stopwatch|stopwatch 'event'", "dump|dump variable"]),
  ...tw("filter", "Twig", ["abs", "batch|batch(size, fill = null, preserve_keys = true)", "capitalize", "column|column(name, index = null)", "date|date(format = null, timezone = null)", "date_modify|date_modify(modifier)", "default|default(value = '')", "escape|escape(strategy = 'html', charset = null)", "e|e(strategy = 'html')", "filter|filter(arrow)", "first", "format|format(...values)", "format_currency|format_currency(currency, attrs = [], locale = null)", "join|join(glue = '', and = null)", "json_encode|json_encode(options = null)", "keys", "last", "length", "lower", "map|map(arrow)", "merge|merge(array)", "nl2br", "number_format|number_format(decimal = 0, decimal_point = '.', thousand_sep = ',')", "raw", "reduce|reduce(arrow, initial = null)", "replace|replace(from)", "reverse|reverse(preserve_keys = false)", "round|round(precision = 0, method = 'common')", "slice|slice(start, length = null, preserve_keys = false)", "slug|slug(separator = '-', locale = null)", "sort|sort(arrow = null)", "split|split(delimiter, limit = null)", "striptags|striptags(allowable_tags = null)", "title", "trim|trim(character_mask = null, side = 'both')", "upper", "url_encode"]),
  ...tw("filter", "Symfony", ["trans|trans(arguments = [], domain = null, locale = null)", "humanize", "yaml_encode|yaml_encode(inline = 0, dumpObjects = 0)", "serialize|serialize(format = 'json', context = [])", "sanitize_html|sanitize_html(sanitizer = null)"]),
  ...tw("function", "Twig", ["attribute|attribute(object, method, arguments = [])", "block|block(name, template = null)", "constant|constant(name, object = null)", "cycle|cycle(values, position)", "date|date(date = null, timezone = null)", "dump|dump(...vars)", "enum|enum(class)", "include|include(template, variables = [], with_context = true)", "max|max(...values)", "min|min(...values)", "parent|parent()", "random|random(values = null, max = null)", "range|range(low, high, step = 1)", "source|source(name, ignore_missing = false)"]),
  ...tw("function", "Symfony", ["path|path(name, parameters = [], relative = false)", "url|url(name, parameters = [], schemeRelative = false)", "asset|asset(path, packageName = null)", "absolute_url|absolute_url(path)", "csrf_token|csrf_token(tokenId)", "is_granted|is_granted(role, object = null, field = null)", "logout_path|logout_path(key = null)", "render|render(uri, options = [])", "controller|controller(controller, attributes = [], query = [])", "form|form(view, variables = [])", "form_start|form_start(view, variables = [])", "form_end|form_end(view, variables = [])", "form_widget|form_widget(view, variables = [])", "form_row|form_row(view, variables = [])", "form_label|form_label(view, label = null, variables = [])", "form_errors|form_errors(view)", "form_help|form_help(view)", "form_rest|form_rest(view, variables = [])", "t|t(message, parameters = [], domain = null)", "importmap|importmap(entryPoint = 'app')"]),
  ...tw("test", "Twig", ["constant", "defined", "divisible by", "empty", "even", "iterable", "mapping", "null", "odd", "same as", "sequence"]),
  { name: "app", kind: "variable", signature: "AppVariable (user, request, session, flashes, environment, debug, current_route)", provider: "Symfony" },
  { name: "price", kind: "filter", provider: "App\\Twig\\AppExtension", location: at("src/Twig/AppExtension.php", "'price'") },
  { name: "cart_count", kind: "function", provider: "App\\Twig\\AppExtension", location: at("src/Twig/AppExtension.php", "'cart_count'") },
];

/** Controller actions rendering each template (`render('x.html.twig')`). */
function renders(): { template: string; controller: string; location: SourceLocation }[] {
  const out: { template: string; controller: string; location: SourceLocation }[] = [];
  for (const [path, src] of Object.entries(files)) {
    if (!path.startsWith("src/Controller/")) continue;
    const cls = /namespace ([\w\\]+);[\s\S]*?class (\w+)/.exec(src);
    for (const m of src.matchAll(/->render\(\s*'([^']+\.twig)'/g)) {
      const before = src.slice(0, m.index);
      const method = [...before.matchAll(/function (\w+)/g)].pop()?.[1] ?? "__invoke";
      const line = before.split("\n").length - 1;
      out.push({ template: m[1], controller: `${cls?.[1]}\\${cls?.[2]}::${method}`, location: { path, line } });
    }
  }
  return out;
}

function methodLocation(controller: string): SourceLocation | undefined {
  const [cls, method] = controller.split("::");
  const short = cls.split("\\").pop()!;
  const path = Object.keys(files).find((p) => p.endsWith(`/${short}.php`));
  if (!path) return undefined;
  return at(path, method ? `function ${method}(` : `class ${short}`);
}

const info = (): SymfonyInfo => ({
  version: "7.2.3",
  environment: "dev",
  consolePath: "bin/console",
  hasConsole: true,
  containerXml: "var/cache/dev/App_KernelDevDebugContainer.xml",
  kernelClass: "App\\Kernel",
  routes: routes.length,
  services: services.length,
  templates: templates.length,
  commands: commands.filter((c) => !c.builtin).length,
});

// ---------------------------------------------------------------- reference at cursor

type RefKind = "route" | "service" | "template" | "parameter" | "tag" | "filter" | "function" | "test";

interface Ref {
  kind: RefKind;
  from: number;
  prefix: string;
  value: string;
}

/** Simplified version of `phpforge_fw_symfony::complete::reference_at`. */
export function referenceAt(path: string, text: string, offset: number): Ref | null {
  const lineStart = text.lastIndexOf("\n", offset - 1) + 1;
  const lineEnd = text.indexOf("\n", offset) < 0 ? text.length : text.indexOf("\n", offset);
  const before = text.slice(lineStart, offset);
  const after = text.slice(offset, lineEnd);
  const rest = (re: RegExp) => re.exec(after)?.[0] ?? "";
  const ref = (kind: RefKind, prefix: string, tail = rest(/^[\w.\-/@\\]*/)): Ref => ({
    kind,
    from: offset - prefix.length,
    prefix,
    value: prefix + tail,
  });
  let m: RegExpExecArray | null;
  if (path.endsWith(".twig")) {
    const open = Math.max(before.lastIndexOf("{{"), before.lastIndexOf("{%"));
    if (open < 0 || /(\}\}|%\})/.test(before.slice(open))) return null;
    const inner = before.slice(open);
    if ((m = /\b(?:path|url)\(\s*['"]([\w.]*)$/.exec(inner))) return ref("route", m[1]);
    if ((m = /(?:\{%-?\s*(?:extends|include|embed|import|from|use)\s+|\binclude\(\s*|\bsource\(\s*)['"]([^'"]*)$/.exec(inner))) return ref("template", m[1]);
    if (/['"][^'"]*$/.test(inner.replace(/'[^']*'|"[^"]*"/g, ""))) return null;
    if ((m = /\|\s*(\w*)$/.exec(inner))) return ref("filter", m[1], rest(/^\w*/));
    if ((m = /^\{%-?\s*(\w*)$/.exec(inner))) return ref("tag", m[1], rest(/^\w*/));
    if ((m = /\bis\s+(?:not\s+)?(\w*)$/.exec(inner))) return ref("test", m[1], rest(/^\w*/));
    if ((m = /(?:^\{\{-?\s*|[(,\[{:=+~?]\s*|\s(?:and|or|not|in|if)\s+)(\w*)$/.exec(inner))) return ref("function", m[1], rest(/^\w*/));
    return null;
  }
  if (path.endsWith(".yaml") || path.endsWith(".yml")) {
    if ((m = /['"]@([\w.\\]*)$/.exec(before))) return ref("service", m[1]);
    if ((m = /['"][^'"]*%([\w.]*)$/.exec(before))) return ref("parameter", m[1], rest(/^[\w.]*/));
    return null;
  }
  if (path.endsWith(".php")) {
    if ((m = /->(?:generateUrl|redirectToRoute)\(\s*['"]([\w.]*)$/.exec(before))) return ref("route", m[1]);
    if ((m = /->(?:render|renderView|renderBlock)\(\s*['"]([^'"]*)$/.exec(before))) return ref("template", m[1]);
    if ((m = /Autowire\(\s*service:\s*['"]([\w.\\]*)$/.exec(before))) return ref("service", m[1]);
    if ((m = /container->get\(\s*['"]([\w.\\]*)$/.exec(before))) return ref("service", m[1]);
    if ((m = /getParameter\(\s*['"]([\w.]*)$/.exec(before))) return ref("parameter", m[1], rest(/^[\w.]*/));
    if ((m = /Autowire\(\s*(?:param:\s*)?['"]%([\w.]*)$/.exec(before))) return ref("parameter", m[1], rest(/^[\w.]*/));
  }
  return null;
}

function fuzzy(name: string, prefix: string): boolean {
  if (!prefix) return true;
  const n = name.toLowerCase();
  const p = prefix.toLowerCase();
  if (n.includes(p)) return true;
  let i = 0;
  for (const c of n) if (c === p[i]) i++;
  return i === p.length && n[0] === p[0];
}

function complete(path: string, text: string, offset: number) {
  const r = referenceAt(path, text, offset);
  if (!r) return null;
  const items: SymfonyCompletionItem[] = [];
  const add = (label: string, kind: SymfonyCompletionKind, detail = "", info = "", apply?: string) => {
    if (fuzzy(label, r.prefix)) items.push({ label, kind, detail, info, apply });
  };
  switch (r.kind) {
    case "route":
      for (const x of routes) add(x.name, "route", `${x.methods?.length ? x.methods.join("|") : "ANY"} ${x.path}`, x.controller ?? "");
      break;
    case "service":
      for (const s of services) add(s.id, "service", s.alias ? `alias for ${s.alias}` : s.class !== s.id ? (s.class ?? "") : "", s.public ? "public" : "");
      break;
    case "template":
      for (const t of templates) add(t.name, "template", t.path);
      break;
    case "parameter": {
      const closing = text[r.from - 1] === "%" && text[offset] !== "%";
      for (const p of parameters) add(p.name, "parameter", p.value, "", closing ? `${p.name}%` : undefined);
      break;
    }
    default:
      for (const s of twig) {
        if (s.kind === r.kind || (r.kind === "function" && s.kind === "variable")) {
          add(s.name, s.kind, s.signature ?? s.provider, s.provider);
        }
      }
  }
  const p = r.prefix.toLowerCase();
  items.sort((a, b) => Number(!a.label.toLowerCase().startsWith(p)) - Number(!b.label.toLowerCase().startsWith(p)) || a.label.localeCompare(b.label));
  return { from: r.from, items };
}

function gotoTargets(path: string, text: string, offset: number): SourceLocation[] {
  const r = referenceAt(path, text, offset);
  if (!r) return [];
  let loc: SourceLocation | undefined;
  switch (r.kind) {
    case "route": {
      const route = routes.find((x) => x.name === r.value);
      loc = (route?.controller && methodLocation(route.controller)) || route?.location;
      break;
    }
    case "service": {
      let s = services.find((x) => x.id === r.value);
      if (s?.alias) s = services.find((x) => x.id === s!.alias) ?? s;
      loc = s?.class ? (methodLocation(s.class) ?? s.location) : s?.location;
      break;
    }
    case "template": {
      const t = templates.find((x) => x.name === r.value);
      loc = t && { path: t.path, line: 0 };
      break;
    }
    case "parameter":
      loc = parameters.find((x) => x.name === r.value)?.location;
      break;
    default:
      loc = twig.find((s) => s.kind === r.kind && s.name === r.value)?.location;
  }
  return loc ? [loc] : [];
}

function gutter(path: string, content: string): SymfonyGutterMark[] {
  const out: SymfonyGutterMark[] = [];
  const lines = content.split("\n");
  if (path.endsWith(".php")) {
    lines.forEach((line, i) => {
      if (/^\s*#\[Route\(/.test(line) && !/class /.test(lines[i + 1] ?? "") && !/^\s*#\[/.test(lines[i + 1] ?? "")) {
        const fn = /function (\w+)/.exec(lines.slice(i, i + 3).join("\n"))?.[1];
        const route = routes.find((r) => r.controller?.endsWith(`::${fn}`) && r.location?.path === path);
        if (route) {
          const methodLine = i + lines.slice(i, i + 3).findIndex((l) => l.includes(`function ${fn}`));
          out.push({ line: methodLine, kind: "route", title: `${route.methods?.length ? route.methods.join("|") : "ANY"} ${route.path} (${route.name})` });
        }
      }
      const tpl = /->render\(\s*'([^']+\.twig)'/.exec(line);
      if (tpl) {
        const t = templates.find((x) => x.name === tpl[1]);
        out.push({ line: i, kind: "template", title: `Go to template ${tpl[1]}`, targets: t ? [{ path: t.path, line: 0 }] : [] });
      }
      const cmd = /#\[AsCommand\(name: '([^']+)'/.exec(line);
      if (cmd) {
        const classLine = lines.findIndex((l) => /\bclass \w+/.test(l));
        out.push({ line: classLine, kind: "command", title: `Run '${cmd[1]}'`, command: cmd[1] });
      }
    });
  } else if (path.endsWith(".twig")) {
    const t = templates.find((x) => x.path === path);
    const refs = t ? renders().filter((r) => r.template === t.name) : [];
    if (refs.length) {
      out.push({
        line: 0,
        kind: "controller",
        title: `Rendered by ${refs.map((r) => r.controller.split("\\").pop()).join(", ")}`,
        targets: refs.map((r) => methodLocation(r.controller) ?? r.location),
      });
    }
  }
  return out.sort((a, b) => a.line - b.line);
}

// ---------------------------------------------------------------- console (simulated)

const consoleOutput: Record<string, string[]> = {
  "cache:clear": [
    "",
    " // Clearing the cache for the dev environment with debug true",
    "",
    " [OK] Cache for the \"dev\" environment (debug=true) was successfully cleared.",
    "",
  ],
  "app:orders:purge": ["Looking for orders abandoned before 2026-09-20…", "Purged 12 abandoned orders."],
  "debug:router": [
    " ----------------- -------- -------- ------ ------------------------- ",
    "  Name              Method   Scheme   Host   Path                     ",
    " ----------------- -------- -------- ------ ------------------------- ",
    ...routes.map((r) => `  ${r.name.padEnd(17)} ${(r.methods?.join("|") || "ANY").padEnd(8)} ANY      ANY    ${r.path}`),
    " ----------------- -------- -------- ------ ------------------------- ",
  ],
};

const timers = new Map<string, ReturnType<typeof setTimeout>[]>();

if (isSymfonyDemo()) {
  registerMock("project_detect", () => structuredClone(symfonyDemoInfo));
  // Like the Laravel demo, announce the framework only with `?announce`.
  setSetting(
    "project.announcedFrameworks",
    new URLSearchParams(location.search).has("announce") ? null : [`~/Projects/${symfonyDemoName}#symfony`, `~/Projects/${symfonyDemoName}#doctrine`],
    "user",
  );
}

registerMock("symfony_detect", () => (isSymfonyDemo() ? info() : null));
registerMock("symfony_refresh", () => info());
registerMock("symfony_routes", () => structuredClone(routes));
registerMock("symfony_services", () => structuredClone(services));
registerMock("symfony_parameters", () => structuredClone(parameters));
registerMock("symfony_templates", () => structuredClone(templates));
registerMock("symfony_console_commands", () => structuredClone(commands));
registerMock("symfony_complete", ({ path, content, offset }) => complete(String(path), String(content), Number(offset)));
registerMock("symfony_goto", ({ path, content, offset }) => gotoTargets(String(path), String(content), Number(offset)));
registerMock("symfony_gutter", ({ path, content }) => (isSymfonyDemo() ? gutter(String(path), String(content)) : []));
registerMock("symfony_console_run", ({ command, args, runId }) => {
  const id = String(runId);
  const name = String(command);
  const line = ["php", "bin/console", name, String(args ?? "")].filter(Boolean).join(" ");
  const out = consoleOutput[name] ?? [`Running ${name}…`, "", " [OK] Done."];
  const list: ReturnType<typeof setTimeout>[] = [];
  emitMock(CONSOLE_EVENT, { type: "started", runId: id, commandLine: line });
  out.forEach((text, i) => list.push(setTimeout(() => emitMock(CONSOLE_EVENT, { type: "output", runId: id, text, stderr: false }), 120 * (i + 1))));
  list.push(setTimeout(() => emitMock(CONSOLE_EVENT, { type: "exit", runId: id, code: 0 }), 120 * (out.length + 1)));
  timers.set(id, list);
});
registerMock("symfony_console_cancel", ({ runId }) => {
  const id = String(runId);
  timers.get(id)?.forEach(clearTimeout);
  timers.delete(id);
  emitMock(CONSOLE_EVENT, { type: "exit", runId: id, code: 130 });
});

export { complete as mockSymfonyComplete, gotoTargets as mockSymfonyGoto, gutter as mockSymfonyGutter };
