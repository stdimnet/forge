/**
 * Browser-mode Symfony translations, forms and events over the Symfony demo project
 * (`?project=symfony`). Simplified versions of `phpforge-fw-symfony`'s `translations`,
 * `forms` and `events` modules; catalogues are read from (and the "Create translation key"
 * fix writes to) the in-memory demo files, never a real project.
 */
import { registerMock } from "@/ipc";
import type {
  EventListener,
  SourceLocation,
  SymfonyCompletionItem,
  SymfonyGutterMark,
  TranslationAnalysis,
  TranslationFile,
  TranslationFold,
  TranslationProblem,
  TranslationRow,
  TranslationTable,
} from "@/ipc/symfony";
import { mockFiles } from "./workspaceMock";

const DEFAULT_LOCALE = "en";
const ENABLED = ["en", "de", "fr"];

interface Entry {
  key: string;
  value: string;
  domain: string;
  locale: string;
  location: SourceLocation;
}

const lineOfOffset = (text: string, offset: number) => text.slice(0, offset).split("\n").length - 1;

const unquote = (v: string) => {
  const t = v.trim();
  if (t.length >= 2 && ((t[0] === "'" && t.endsWith("'")) || (t[0] === '"' && t.endsWith('"')))) return t.slice(1, -1).replace(/''/g, "'");
  return t;
};

/** Nested YAML mappings flattened to dotted keys (enough for the demo catalogues). */
export function parseYamlCatalogue(text: string): { key: string; value: string; line: number }[] {
  const out: { key: string; value: string; line: number }[] = [];
  const stack: { indent: number; key: string }[] = [];
  text.split("\n").forEach((line, i) => {
    const m = /^(\s*)(['"]?)([^'":#][^:]*?)\2:(?:\s+(.*))?$/.exec(line);
    if (!m || /^\s*#/.test(line)) return;
    const indent = m[1].length;
    while (stack.length && stack[stack.length - 1].indent >= indent) stack.pop();
    const path = [...stack.map((s) => s.key), m[3]].join(".");
    const raw = m[4]?.trim();
    const value = raw && /^['"]/.test(raw) ? raw : raw?.replace(/\s+#.*$/, "");
    if (value === undefined || value === "") stack.push({ indent, key: m[3] });
    else out.push({ key: path, value: unquote(value), line: i });
  });
  return out;
}

function parseXliff(text: string): { key: string; value: string; line: number }[] {
  const out: { key: string; value: string; line: number }[] = [];
  for (const m of text.matchAll(/<trans-unit[^>]*?(?:resname="([^"]*)")?[^>]*>\s*<source>([^<]*)<\/source>\s*(?:<target>([^<]*)<\/target>)?/g)) {
    out.push({ key: m[1] || m[2], value: m[3] ?? m[2], line: lineOfOffset(text, m.index ?? 0) });
  }
  return out;
}

function catalogues(): { files: TranslationFile[]; entries: Entry[] } {
  const files: TranslationFile[] = [];
  const entries: Entry[] = [];
  for (const path of Object.keys(mockFiles).sort()) {
    const m = /^translations\/([\w-]+?)(\+intl-icu)?\.([\w-]+)\.(ya?ml|xlf|xliff)$/.exec(path);
    if (!m) continue;
    const [, domain, icu, locale, ext] = m;
    const format = ext.startsWith("y") ? "yaml" : "xliff";
    files.push({ path, domain, locale, format, icu: !!icu });
    const parsed = format === "yaml" ? parseYamlCatalogue(mockFiles[path]) : parseXliff(mockFiles[path]);
    for (const e of parsed) entries.push({ key: e.key, value: e.value, domain, locale, location: { path, line: e.line } });
  }
  return { files, entries };
}

const domainLocales = (files: TranslationFile[], domain: string) => [...new Set(files.filter((f) => f.domain === domain).map((f) => f.locale))].sort();

export function translationTable(): TranslationTable {
  const { files, entries } = catalogues();
  const rows = new Map<string, TranslationRow>();
  for (const e of entries) {
    const id = `${e.domain}\u0000${e.key}`;
    const row = rows.get(id) ?? { domain: e.domain, key: e.key, values: {}, locations: {} };
    row.values[e.locale] = e.value;
    row.locations[e.locale] = e.location;
    rows.set(id, row);
  }
  const list = [...rows.values()].sort((a, b) => a.domain.localeCompare(b.domain) || a.key.localeCompare(b.key));
  for (const r of list) {
    const missing = domainLocales(files, r.domain).filter((l) => !(l in r.values));
    if (missing.length) r.missing = missing;
  }
  const locales = [...new Set([...files.map((f) => f.locale), ...ENABLED])].sort((a, b) => (a === DEFAULT_LOCALE ? -1 : b === DEFAULT_LOCALE ? 1 : a.localeCompare(b)));
  return { defaultLocale: DEFAULT_LOCALE, locales, domains: [...new Set(files.map((f) => f.domain))].sort(), files, rows: list };
}

// ---------------------------------------------------------------- references

interface Ref {
  kind: "key" | "domain";
  /** Content range. */
  from: number;
  to: number;
  /** Literal range (quotes included). */
  litFrom: number;
  litTo: number;
  value: string;
  domain: string;
}

function references(path: string, text: string): Ref[] {
  const out: Ref[] = [];
  const twig = path.endsWith(".twig");
  const php = path.endsWith(".php") && !path.endsWith(".blade.php");
  if (!twig && !php) return out;
  const defaultDomain = (at: number) => /trans_default_domain\s+['"]([\w.-]+)['"]/.exec(text.slice(0, at))?.[1] ?? "messages";
  const push = (m: RegExpMatchArray, keyGroup: number, domain?: string) => {
    const quoteAt = (m.index ?? 0) + m[0].indexOf(m[keyGroup - 1] + m[keyGroup]);
    const from = quoteAt + 1;
    out.push({ kind: "key", from, to: from + m[keyGroup].length, litFrom: quoteAt, litTo: from + m[keyGroup].length + 1, value: m[keyGroup], domain: domain ?? defaultDomain(quoteAt) });
  };
  if (twig) {
    for (const m of text.matchAll(/(['"])([^'"\n]+)\1\s*\|\s*trans\b(\s*\((?:\{[^}]*\}|\[[^\]]*\]|[^,)]*)\s*(?:,\s*['"]([\w.-]+)['"])?)?/g)) push(m, 2, m[4]);
    for (const m of text.matchAll(/\bt\(\s*(['"])([^'"\n]+)\1/g)) push(m, 2);
    for (const m of text.matchAll(/\{%-?\s*trans(?:\s+[^%]*?from\s+['"]([\w.-]+)['"])?[^%]*%\}([^{]+)\{%-?\s*endtrans/g)) {
      const body = m[2];
      const lead = body.length - body.trimStart().length;
      const from = (m.index ?? 0) + m[0].indexOf(body) + lead;
      const key = body.trim();
      out.push({ kind: "key", from, to: from + key.length, litFrom: from, litTo: from + key.length, value: key, domain: m[1] ?? defaultDomain(from) });
    }
  } else {
    for (const m of text.matchAll(/->trans\(\s*(['"])([^'"\n]+)\1(?:\s*,\s*(?:\[[^\]]*\]|\$\w+)\s*,\s*['"]([\w.-]+)['"])?/g)) push(m, 2, m[3]);
    for (const m of text.matchAll(/(?<![\w>$])t\(\s*(['"])([^'"\n]+)\1/g)) push(m, 2);
  }
  return out.sort((a, b) => a.from - b.from);
}

export function translationAnalysis(path: string, text: string): TranslationAnalysis {
  const table = translationTable();
  const problems: TranslationProblem[] = [];
  const folds: TranslationFold[] = [];
  for (const r of references(path, text)) {
    if (r.kind !== "key" || /[${}#]/.test(r.value)) continue;
    const row = table.rows.find((x) => x.domain === r.domain && x.key === r.value);
    const wanted = domainLocales(table.files, r.domain);
    if (!row) {
      problems.push({
        from: r.from,
        to: r.to,
        severity: "warning",
        message: `Missing translation key '${r.value}' in domain '${r.domain}'`,
        domain: r.domain,
        key: r.value,
        locales: wanted.length ? wanted : [DEFAULT_LOCALE],
      });
      continue;
    }
    if (row.missing?.length) {
      problems.push({
        from: r.from,
        to: r.to,
        severity: "weak",
        message: `Translation '${r.value}' is missing for locale${row.missing.length === 1 ? "" : "s"} ${row.missing.join(", ")}`,
        domain: r.domain,
        key: r.value,
        locales: row.missing,
      });
    }
    const value = row.values[DEFAULT_LOCALE] ?? Object.values(row.values)[0];
    if (value !== undefined) folds.push({ from: r.litFrom, to: r.litTo, text: value.length > 60 ? `${value.slice(0, 59)}…` : value, key: r.value });
  }
  return { problems, folds };
}

/** Append `key` to the demo catalogues (flat keys, which Symfony accepts as well). */
export function createTranslation(domain: string, key: string, locales: string[]): SourceLocation[] {
  const out: SourceLocation[] = [];
  for (const locale of locales) {
    const path = Object.keys(mockFiles).find((p) => p === `translations/${domain}.${locale}.yaml`) ?? `translations/${domain}.${locale}.yaml`;
    const current = mockFiles[path] ?? "";
    const text = current && !current.endsWith("\n") ? `${current}\n` : current;
    mockFiles[path] = `${text}${key}: '${key.replace(/'/g, "''")}'\n`;
    out.push({ path, line: mockFiles[path].split("\n").length - 2 });
  }
  return out;
}

// ---------------------------------------------------------------- events

const at = (path: string, needle: string): SourceLocation => {
  const src = mockFiles[path] ?? "";
  const i = src.indexOf(needle);
  return { path, line: i < 0 ? 0 : lineOfOffset(src, i) };
};

const SUB = "src/EventSubscriber/LocaleSubscriber.php";
const MAIL = "src/EventListener/OrderMailListener.php";

export function eventListeners(): EventListener[] {
  return [
    { event: "App\\Event\\OrderPlacedEvent", eventClass: "App\\Event\\OrderPlacedEvent", class: "App\\EventListener\\OrderMailListener", method: "onOrderPlaced", priority: 10, source: "attribute", location: at(MAIL, "function onOrderPlaced"), declaredAt: at(MAIL, "#[AsEventListener(priority") },
    { event: "App\\Event\\OrderPlacedEvent", eventClass: "App\\Event\\OrderPlacedEvent", class: "App\\EventSubscriber\\LocaleSubscriber", method: "onOrderPlaced", priority: 0, source: "subscriber", location: at(SUB, "function onOrderPlaced"), declaredAt: at(SUB, "OrderPlacedEvent::class =>") },
    { event: "kernel.exception", eventClass: "Symfony\\Component\\HttpKernel\\Event\\ExceptionEvent", class: "App\\EventListener\\OrderMailListener", method: "onException", priority: 0, source: "attribute", location: at(MAIL, "function onException"), declaredAt: at(MAIL, "#[AsEventListener(event:") },
    { event: "kernel.request", eventClass: "Symfony\\Component\\HttpKernel\\Event\\RequestEvent", class: "App\\EventSubscriber\\LocaleSubscriber", method: "onKernelRequest", priority: 20, source: "subscriber", location: at(SUB, "function onKernelRequest"), declaredAt: at(SUB, "KernelEvents::REQUEST") },
  ];
}

const KERNEL_EVENTS: [string, string][] = [
  ["kernel.request", "RequestEvent"],
  ["kernel.controller", "ControllerEvent"],
  ["kernel.controller_arguments", "ControllerArgumentsEvent"],
  ["kernel.view", "ViewEvent"],
  ["kernel.response", "ResponseEvent"],
  ["kernel.finish_request", "FinishRequestEvent"],
  ["kernel.terminate", "TerminateEvent"],
  ["kernel.exception", "ExceptionEvent"],
  ["console.command", "ConsoleCommandEvent"],
  ["console.error", "ConsoleErrorEvent"],
  ["console.terminate", "ConsoleTerminateEvent"],
];

export function eventGutter(path: string, content: string): SymfonyGutterMark[] {
  const out: SymfonyGutterMark[] = [];
  const listeners = eventListeners();
  const lines = content.split("\n");
  for (const l of listeners.filter((x) => x.location?.path === path)) {
    const line = lines.findIndex((t) => t.includes(`function ${l.method}(`));
    if (line < 0) continue;
    const short = l.event.includes("\\") ? l.event.split("\\").pop() : l.event;
    const target = l.eventClass === "App\\Event\\OrderPlacedEvent" ? at("src/Event/OrderPlacedEvent.php", "class OrderPlacedEvent") : undefined;
    out.push({ line, kind: "listener", title: `Listens to ${short}${l.priority ? ` (priority ${l.priority})` : ""}`, targets: target ? [target] : [] });
  }
  if (path === "src/Event/OrderPlacedEvent.php") {
    const line = lines.findIndex((t) => t.includes("class OrderPlacedEvent"));
    const of = listeners.filter((l) => l.event === "App\\Event\\OrderPlacedEvent");
    out.push({
      line,
      kind: "event",
      title: `${of.length} listeners of OrderPlacedEvent: ${of.map((l) => `${l.class.split("\\").pop()}::${l.method}`).join(", ")}`,
      targets: of.flatMap((l) => (l.location ? [l.location] : [])),
    });
  }
  return out;
}

// ---------------------------------------------------------------- completion

const FORM_OPTIONS: Record<string, string[]> = {
  FormType: ["attr", "constraints", "data", "disabled", "empty_data", "help", "label", "label_attr", "mapped", "required", "row_attr", "translation_domain"],
  TextType: [],
  CheckboxType: ["false_values", "value"],
  MoneyType: ["currency", "divisor", "grouping", "html5", "input", "rounding_mode", "scale"],
  ChoiceType: ["choices", "choice_label", "expanded", "multiple", "placeholder", "preferred_choices"],
};

/** Completion of translation keys, domains, event names and form options; `null` elsewhere. */
export function extrasComplete(path: string, text: string, offset: number): { from: number; items: SymfonyCompletionItem[] } | null {
  const lineStart = text.lastIndexOf("\n", offset - 1) + 1;
  const before = text.slice(lineStart, offset);
  let m: RegExpExecArray | null;
  const table = () => translationTable();
  const keys = (domain: string, prefix: string) => ({
    from: offset - prefix.length,
    items: table()
      .rows.filter((r) => r.domain === domain && r.key.includes(prefix))
      .map((r): SymfonyCompletionItem => ({ label: r.key, kind: "translation", detail: r.values[DEFAULT_LOCALE] ?? Object.values(r.values)[0] ?? "", info: domain })),
  });
  if (path.endsWith(".twig")) {
    if ((m = /\{\{[^}]*?(['"])([\w.-]*)$/.exec(before)) && /^[\w.-]*['"]\s*\|\s*trans\b/.test(text.slice(offset))) return keys("messages", m[2]);
    if ((m = /\{%-?\s*trans\s*%\}([\w.-]*)$/.exec(before))) return keys("messages", m[1]);
    if ((m = /\|\s*trans\(\s*(?:\{[^}]*\}|\[[^\]]*\])\s*,\s*['"]([\w.-]*)$/.exec(before))) {
      const prefix = m[1];
      return { from: offset - prefix.length, items: table().domains.filter((d) => d.includes(prefix)).map((d) => ({ label: d, kind: "domain" as const })) };
    }
    return null;
  }
  if (path.endsWith(".php")) {
    if ((m = /->trans\(\s*['"]([\w.-]*)$/.exec(before)) || (m = /(?<![\w>$])t\(\s*['"]([\w.-]*)$/.exec(before))) return keys("messages", m[1]);
    if ((m = /AsEventListener\((?:event:\s*)?['"]([\w.]*)$/.exec(before)) || (m = /addListener\(\s*['"]([\w.]*)$/.exec(before))) {
      const prefix = m[1];
      return {
        from: offset - prefix.length,
        items: KERNEL_EVENTS.filter(([n]) => n.includes(prefix)).map(([label, detail]): SymfonyCompletionItem => ({ label, kind: "event", detail })),
      };
    }
    if ((m = /->add\(\s*'[\w.]*'\s*,\s*(\w+)::class\s*,\s*\[(?:[^\]]*,)?\s*'(\w*)$/.exec(before))) {
      const [, type, prefix] = m;
      const own = FORM_OPTIONS[type] ?? [];
      const items = [...own.map((o) => [o, type]), ...FORM_OPTIONS.FormType.map((o) => [o, "FormType"])]
        .filter(([o]) => o.includes(prefix))
        .map(([label, detail]): SymfonyCompletionItem => ({ label, kind: "formOption", detail }));
      return { from: offset - prefix.length, items };
    }
    if ((m = /->add\(\s*'(\w*)$/.exec(before)) && /data_class/.test(text)) {
      const prefix = m[1];
      const fields: [string, string][] = [["id", "int"], ["name", "string"], ["slug", "string"], ["price", "string"], ["available", "bool"]];
      return {
        from: offset - prefix.length,
        items: fields.filter(([n]) => n.includes(prefix)).map(([label, detail]): SymfonyCompletionItem => ({ label, kind: "formField", detail, info: "App\\Entity\\Product" })),
      };
    }
  }
  return null;
}

/** Go to Declaration of a translation key (every locale) or of a form type. */
export function extrasGoto(path: string, text: string, offset: number): SourceLocation[] | null {
  const r = references(path, text).find((x) => x.from <= offset && offset <= x.to);
  if (r) {
    const row = translationTable().rows.find((x) => x.domain === r.domain && x.key === r.value);
    return row ? Object.values(row.locations) : [];
  }
  const word = /(\w+)$/.exec(text.slice(0, offset))?.[1] ?? "";
  const rest = /^\w*/.exec(text.slice(offset))?.[0] ?? "";
  if (`${word}${rest}` === "ProductType" && text.slice(offset + rest.length).startsWith("::class")) return [at("src/Form/ProductType.php", "class ProductType")];
  return null;
}

registerMock("symfony_translations", () => translationTable());
registerMock("symfony_events", () => eventListeners());
registerMock("symfony_translation_analysis", ({ path, content }) => translationAnalysis(String(path), String(content)));
registerMock("symfony_create_translation", ({ domain, key, locales }) => createTranslation(String(domain), String(key), (locales as string[]) ?? []));
