/** Browser-mode PHPForge backend over the demo project (see phpMockParser.ts). */
import { emitMock, registerMock } from "@/ipc";
import { decodeInterpreters, findInterpreter, levelForVersion } from "@/features/php/interpreters";
import type {
  FieldTarget,
  FileStructure,
  GenerateKind,
  GenerateOptions,
  GenerateTargets,
  Hover,
  MethodTarget,
  StructureNode,
  SymbolHit,
  TextEdit,
} from "@/features/php/api";
import { demoFiles } from "./demoProject";
import { mockSettingValue } from "./settingsMock";
import { mockFiles } from "./workspaceMock";
import { classAt, lineOf, memberType, parsePhp, type MockClass, type MockMember } from "./phpMockParser";
import { callWasmSync, forgeWasm, type ForgeWasm } from "@/wasm";
import type { FilterHit } from "@/ipc/search";


/** Stand-ins for vendor classes the demo extends, so Override Methods has content. */
const vendorStubs: Record<string, { methods: { name: string; sig: string; visibility: "public" | "protected"; isStatic?: boolean; abstract?: boolean }[] }> = {
  "Illuminate\\Database\\Eloquent\\Model": {
    methods: [
      { name: "save", sig: "save(array $options = []): bool", visibility: "public" },
      { name: "delete", sig: "delete(): ?bool", visibility: "public" },
      { name: "fresh", sig: "fresh(array|string $with = []): ?static", visibility: "public" },
      { name: "toArray", sig: "toArray(): array", visibility: "public" },
      { name: "boot", sig: "boot(): void", visibility: "protected", isStatic: true },
    ],
  },
  "Illuminate\\Foundation\\Auth\\User": {
    methods: [
      { name: "getAuthIdentifierName", sig: "getAuthIdentifierName(): string", visibility: "public" },
      { name: "getAuthPassword", sig: "getAuthPassword(): string", visibility: "public" },
      { name: "save", sig: "save(array $options = []): bool", visibility: "public" },
    ],
  },
};

function source(path: unknown, content: unknown): string {
  if (typeof content === "string") return content;
  const s = demoFiles[String(path)];
  if (s === undefined) throw new Error(`not found: ${path}`);
  return s;
}

const phpFiles = () => Object.keys(demoFiles).filter((p) => p.endsWith(".php") && !p.endsWith(".blade.php"));

function resolveName(name: string, file: ReturnType<typeof parsePhp>): string {
  if (name.startsWith("\\")) return name.slice(1);
  const [head, ...rest] = name.split("\\");
  const imported = file.uses.get(head);
  if (imported) return [imported, ...rest].join("\\");
  return file.namespace ? `${file.namespace}\\${name}` : name;
}

function findClass(fqn: string): { path: string; cls: MockClass; src: string } | undefined {
  for (const path of phpFiles()) {
    const src = demoFiles[path];
    const cls = parsePhp(src).classes.find((c) => c.fqn.toLowerCase() === fqn.toLowerCase());
    if (cls) return { path, cls, src };
  }
  return undefined;
}

// ---------------------------------------------------------------- indexing + search

let indexing: Promise<unknown> | null = null;
registerMock("php_index_workspace", () => {
  const files = phpFiles();
  indexing ??= new Promise((resolve) => {
    let done = 0;
    const tick = () => {
      done = Math.min(files.length, done + 1);
      emitMock("php://index-progress", { done, total: files.length });
      if (done < files.length) setTimeout(tick, 70);
      else
        resolve({
          files: files.length,
          parsed: files.length,
          cached: 0,
          classes: files.reduce((n, p) => n + parsePhp(demoFiles[p]).classes.length, 0),
          functions: 0,
          durationMs: 40,
        });
    };
    setTimeout(tick, 70);
  }).finally(() => (indexing = null));
  return indexing;
});

/** Match positions from the shared Rust matcher (`forge-fuzzy`), or null. */
function fuzzy(wasm: ForgeWasm, query: string, name: string, exact = false): number[] | null {
  const [hit] = callWasmSync<FilterHit[]>(wasm.fuzzy_filter, { query, candidates: [name], exact });
  return hit ? hit.positions : null;
}

registerMock("php_search_symbols", async ({ query, kind, limit, exact }) => {
  const wasm = await forgeWasm();
  const hits: SymbolHit[] = [];
  const q = String(query ?? "");
  for (const path of phpFiles()) {
    const src = demoFiles[path];
    for (const c of parsePhp(src).classes) {
      const add = (name: string, fqn: string, k: SymbolHit["kind"], container: string, offset: number) => {
        const positions = fuzzy(wasm, q, name, exact === true);
        if (!positions) return;
        const score = 100 - name.length - (positions[positions.length - 1] ?? 0) + (positions[0] === 0 ? 20 : 0);
        hits.push({ name, fqn, kind: k, container, path, offset, line: lineOf(src, offset), column: 0, score, positions });
      };
      add(c.name, c.fqn, c.kind, c.namespace, c.offset);
      if (kind === "symbol") for (const m of c.members) add(m.name, `${c.fqn}::${m.name}`, m.kind, c.fqn, m.offset);
    }
  }
  return hits.sort((a, b) => b.score - a.score).slice(0, Number(limit ?? 100));
});

// ---------------------------------------------------------------- structure

function detail(m: MockMember): string {
  const t = memberType(m);
  if (m.kind === "method") return `(${m.params ?? ""})${m.type ? `: ${m.type}` : ""}`;
  if (m.kind === "property") return t ? `: ${t}` : "";
  return m.value ? ` = ${m.value}` : "";
}

function memberNode(src: string, m: MockMember): StructureNode {
  return {
    name: m.name,
    kind: m.kind,
    detail: detail(m),
    visibility: m.kind === "enumCase" ? undefined : m.visibility,
    modifiers: m.modifiers,
    offset: m.offset,
    line: lineOf(src, m.offset),
    deprecated: m.doc?.includes("@deprecated") || undefined,
  };
}

registerMock("php_file_structure", ({ path, content, inherited }): FileStructure => {
  const src = source(path, content);
  const file = parsePhp(src);
  const nodes = file.classes.map((c): StructureNode => {
    const children = c.members.map((m) => memberNode(src, m));
    if (inherited && c.extends) {
      const parentFqn = resolveName(c.extends, file);
      const own = new Set(c.members.map((m) => m.name.toLowerCase()));
      const parent = findClass(parentFqn);
      if (parent) {
        for (const m of parent.cls.members)
          if (!own.has(m.name.toLowerCase()) && m.visibility !== "private")
            children.push({ ...memberNode(parent.src, m), inheritedFrom: parent.cls.name, path: parent.path });
      }
      for (const m of vendorStubs[parentFqn]?.methods ?? [])
        if (!own.has(m.name.toLowerCase()))
          children.push({
            name: m.name,
            kind: "method",
            detail: m.sig.slice(m.name.length),
            visibility: m.visibility,
            modifiers: { isStatic: m.isStatic },
            offset: 0,
            line: 0,
            inheritedFrom: parentFqn.split("\\").pop(),
            path: `vendor/${parentFqn.replace(/\\/g, "/")}.php`,
          });
    }
    return {
      name: c.name,
      kind: c.kind,
      detail: c.extends ? ` extends ${c.extends.split("\\").pop()}` : "",
      modifiers: c.modifiers,
      offset: c.offset,
      line: lineOf(src, c.offset),
      deprecated: c.doc?.includes("@deprecated") || undefined,
      children,
    };
  });
  return { path: String(path), nodes };
});

// ---------------------------------------------------------------- hover

function docMarkdown(doc?: string): string {
  if (!doc) return "";
  const lines = doc
    .replace(/^\/\*\*|\*\/$/g, "")
    .split("\n")
    .map((l) => l.replace(/^\s*\*\s?/, "").trimEnd());
  const text: string[] = [];
  const tags: string[] = [];
  const annotations: string[] = [];
  for (const l of lines) {
    const m = /^@([\w\\-]+)\s*(.*)$/.exec(l.trim());
    if (!m) {
      if (!tags.length) text.push(l);
      continue;
    }
    if (/^[A-Z]|\\/.test(m[1])) annotations.push(`- \`@${m[1]}${m[2]}\``);
    else if (m[1] === "property-read" || m[1] === "property") {
      const [type, name] = m[2].split(/\s+/);
      tags.push(`- \`${type} ${name}\`${m[1] === "property-read" ? " *(read-only)*" : ""}`);
    } else if (m[1] === "var") tags.push(`**Type:** \`${m[2].split(/\s+/)[0]}\``);
    else if (m[1] === "return") tags.push(`**Returns:** \`${m[2]}\``);
    else if (m[1] === "deprecated") tags.unshift(`**Deprecated:** ${m[2]}`);
  }
  const props = tags.filter((t) => t.startsWith("- "));
  const rest = tags.filter((t) => !t.startsWith("- "));
  return [
    text.join("\n").trim(),
    ...rest,
    props.length ? `**Properties:**\n${props.join("\n")}` : "",
    annotations.length ? `**Annotations:**\n${annotations.join("\n")}` : "",
  ]
    .filter(Boolean)
    .join("\n\n");
}

function classSignature(c: MockClass): string {
  const mods = [c.modifiers.isFinal && "final", c.modifiers.isAbstract && "abstract"].filter(Boolean).join(" ");
  const ext = c.extends ? ` extends ${c.extends.split("\\").pop()}` : "";
  const impl = c.implements.length ? ` implements ${c.implements.join(", ")}` : "";
  return `${mods ? `${mods} ` : ""}${c.kind} ${c.name}${ext}${impl}`;
}

function memberSignature(m: MockMember): string {
  const mods = [m.modifiers.isAbstract && "abstract", m.modifiers.isFinal && "final", m.visibility, m.modifiers.isStatic && "static", m.modifiers.isReadonly && "readonly"]
    .filter(Boolean)
    .join(" ");
  if (m.kind === "method") return `${mods} function ${m.name}(${m.params ?? ""})${m.type ? `: ${m.type}` : ""}`;
  if (m.kind === "property") return `${mods} ${m.type ? `${m.type} ` : ""}$${m.name}${m.value ? ` = ${m.value}` : ""}`;
  if (m.kind === "enumCase") return `case ${m.name}${m.value ? ` = ${m.value}` : ""}`;
  return `${mods} const ${m.name} = ${m.value}`;
}

const code = (attrs: string[], sig: string) => "```php\n" + attrs.map((a) => `#[${a}]\n`).join("") + sig + "\n```";

registerMock("php_hover", ({ path, offset, content }): Hover | null => {
  const src = source(path, content);
  const pos = Number(offset);
  const isWord = (ch: string | undefined) => !!ch && /[\w\\$]/.test(ch);
  let start = pos;
  let end = pos;
  while (start > 0 && isWord(src[start - 1])) start--;
  while (end < src.length && isWord(src[end])) end++;
  if (start === end) return null;
  const word = src.slice(start, end);
  const file = parsePhp(src);
  for (const c of file.classes) {
    if (c.offset === start)
      return { start, end, markdown: [`\`${c.namespace}\``, code(c.attributes, classSignature(c)), docMarkdown(c.doc)].filter(Boolean).join("\n\n") };
    for (const m of c.members)
      if (m.offset === start)
        return { start, end, markdown: [`\`${c.fqn}\``, code(m.attributes, memberSignature(m)), docMarkdown(m.doc)].filter(Boolean).join("\n\n") };
  }
  const before = src.slice(0, start);
  const member = /(\$this|self|static|(\w+))\s*(->|::)\s*$/.exec(before);
  if (member) {
    const owner = member[2] ? findClass(resolveName(member[2], file))?.cls : classAt(file, start);
    const m = owner?.members.find((x) => x.name === word.replace(/^\$/, ""));
    if (owner && m) return { start, end, markdown: [`\`${owner.fqn}\``, code(m.attributes, memberSignature(m)), docMarkdown(m.doc)].filter(Boolean).join("\n\n") };
    return null;
  }
  if (word.startsWith("$")) return null;
  const target = findClass(resolveName(word, file));
  if (!target) return null;
  const c = target.cls;
  return { start, end, markdown: [`\`${c.namespace}\``, code(c.attributes, classSignature(c)), docMarkdown(c.doc)].filter(Boolean).join("\n\n") };
});

// ---------------------------------------------------------------- generate

const studly = (s: string) => s.split("_").filter(Boolean).map((p) => p[0].toUpperCase() + p.slice(1)).join("");
const isBool = (t?: string) => !!t && /^\??bool(ean)?$/i.test(t);
export const getterName = (f: string, t?: string) =>
  isBool(t) ? (/^(is|has|can|should)[A-Z_]/.test(f) ? f : `is${studly(f)}`) : `get${studly(f)}`;
export const setterName = (f: string) => `set${studly(f)}`;

function fieldTargets(src: string, c: MockClass): FieldTarget[] {
  const ctor = c.members.find((m) => m.kind === "method" && m.name === "__construct");
  const ctorBody = ctor ? src.slice(ctor.start, ctor.end) : "";
  const has = (n: string) => c.members.some((m) => m.kind === "method" && m.name.toLowerCase() === n.toLowerCase());
  return c.members
    .filter((m) => m.kind === "property")
    .map((m) => ({
      name: m.name,
      typeHint: memberType(m),
      visibility: m.visibility,
      isStatic: !!m.modifiers.isStatic,
      isReadonly: !!m.modifiers.isReadonly,
      promoted: !!m.promoted,
      hasGetter: has(getterName(m.name, memberType(m))),
      hasSetter: has(setterName(m.name)),
      inConstructor: !!m.promoted || ctorBody.includes(`$this->${m.name} =`),
    }));
}

function methodTargets(file: ReturnType<typeof parsePhp>, c: MockClass): MethodTarget[] {
  if (!c.extends) return [];
  const fqn = resolveName(c.extends, file);
  const own = new Set(c.members.map((m) => m.name.toLowerCase()));
  const out: MethodTarget[] = [];
  const parent = findClass(fqn);
  for (const m of parent?.cls.members ?? [])
    if (m.kind === "method" && m.visibility !== "private" && !own.has(m.name.toLowerCase()))
      out.push({ id: `${fqn}::${m.name}`, name: m.name, owner: fqn, ownerKind: "class", signature: `${m.name}(${m.params ?? ""})${m.type ? `: ${m.type}` : ""}`, visibility: m.visibility, isStatic: !!m.modifiers.isStatic });
  for (const m of vendorStubs[fqn]?.methods ?? [])
    if (!own.has(m.name.toLowerCase()))
      out.push({ id: `${fqn}::${m.name}`, name: m.name, owner: fqn, ownerKind: "class", signature: m.sig, visibility: m.visibility, isStatic: !!m.isStatic });
  return out;
}

registerMock("php_generate_targets", ({ path, offset, content }): GenerateTargets => {
  const src = source(path, content);
  const file = parsePhp(src);
  const pos = Number(offset);
  const c = classAt(file, pos);
  const docTarget = c?.members.find((m) => m.start <= pos && pos <= m.end && !m.doc && !m.promoted);
  if (!c) return { fields: [], hasConstructor: false, hasToString: false, overrideMethods: [], implementMethods: [] };
  return {
    className: c.name,
    classKind: c.kind,
    fields: c.kind === "interface" ? [] : fieldTargets(src, c),
    hasConstructor: c.members.some((m) => m.name === "__construct"),
    hasToString: c.members.some((m) => m.name === "__toString"),
    overrideMethods: methodTargets(file, c),
    implementMethods: [],
    phpDocTarget: docTarget?.name,
  };
});

function indentOf(src: string, offset: number): string {
  const ls = src.lastIndexOf("\n", offset - 1) + 1;
  return /^[ \t]*/.exec(src.slice(ls))![0];
}

function indentBlock(block: string, indent: string): string {
  return block
    .split("\n")
    .map((l) => {
      if (!l.trim()) return "";
      const spaces = /^ */.exec(l)![0].length;
      return indent + "    ".repeat(Math.floor(spaces / 4)) + " ".repeat(spaces % 4) + l.slice(spaces);
    })
    .join("\n");
}

function memberIndent(src: string, c: MockClass): string {
  const first = c.members.find((m) => !m.promoted);
  const classIndent = indentOf(src, c.start);
  return first ? indentOf(src, first.start) : `${classIndent}    `;
}

function insertAtEnd(src: string, c: MockClass, blocks: string[]): TextEdit {
  const body = indentBlock(blocks.join("\n\n"), memberIndent(src, c));
  const inner = src.slice(c.open + 1, c.close);
  if (!inner.trim()) return { start: c.open + 1, end: c.close, newText: `\n${body}\n${indentOf(src, c.start)}` };
  const at = c.open + 1 + inner.trimEnd().length;
  return { start: at, end: at, newText: `\n\n${body}` };
}

const docblock = (lines: string[]) => `/**\n${lines.map((l) => ` * ${l}\n`).join("")} */\n`;

function getter(m: MockMember): string {
  const t = memberType(m);
  const doc = !m.type && t ? docblock([`@return ${t}`]) : "";
  const ret = m.type ? `: ${m.type}` : "";
  const name = getterName(m.name, t);
  return m.modifiers.isStatic
    ? `${doc}public static function ${name}()${ret}\n{\n    return self::$${m.name};\n}`
    : `${doc}public function ${name}()${ret}\n{\n    return $this->${m.name};\n}`;
}

function setter(m: MockMember, fluent: boolean): string {
  const t = memberType(m);
  fluent = fluent && !m.modifiers.isStatic;
  const doc = !m.type && t ? docblock([`@param ${t} $${m.name}`, ...(fluent ? ["@return $this"] : [])]) : "";
  const param = m.type ? `${m.type} $${m.name}` : `$${m.name}`;
  const name = setterName(m.name);
  if (m.modifiers.isStatic) return `${doc}public static function ${name}(${param}): void\n{\n    self::$${m.name} = $${m.name};\n}`;
  if (fluent) return `${doc}public function ${name}(${param}): static\n{\n    $this->${m.name} = $${m.name};\n    return $this;\n}`;
  return `${doc}public function ${name}(${param}): void\n{\n    $this->${m.name} = $${m.name};\n}`;
}

export function mockGenerate(src: string, offset: number, kind: GenerateKind, opts: GenerateOptions): TextEdit[] {
  const file = parsePhp(src);
  const c = classAt(file, offset);
  if (kind === "phpDoc") {
    const m = c?.members.find((x) => x.start <= offset && offset <= x.end && !x.promoted);
    if (!m) throw new Error("Place the caret on a function or field");
    if (m.doc) throw new Error("PHPDoc comment already exists");
    const lines =
      m.kind === "method"
        ? [
            ...(m.params ? m.params.split(",").map((p) => p.trim().replace(/\s*=.*$/, "")).filter(Boolean).map((p) => (p.includes("$") && p.split(/\s+/).length > 1 ? `@param ${p.split(/\s+/).slice(-2).join(" ")}` : `@param mixed ${p}`)) : []),
            ...(m.type === "void" || m.name === "__construct" ? [] : [`@return ${m.type ?? "mixed"}`]),
          ]
        : [`@var ${m.type ?? "mixed"}`];
    const indent = indentOf(src, m.start);
    const ls = src.lastIndexOf("\n", m.start - 1) + 1;
    return [{ start: ls, end: ls, newText: docblock(lines).split("\n").filter(Boolean).map((l) => indent + l + "\n").join("") }];
  }
  if (!c) throw new Error("Place the caret inside a class");
  const props = c.members.filter((m) => m.kind === "property" && (opts.fields ?? []).includes(m.name));
  const has = (n: string) => c.members.some((m) => m.kind === "method" && m.name.toLowerCase() === n.toLowerCase());
  switch (kind) {
    case "getters":
    case "setters":
    case "gettersAndSetters": {
      const blocks: string[] = [];
      for (const m of props) {
        if (kind !== "setters" && !has(getterName(m.name, memberType(m)))) blocks.push(getter(m));
        if (kind !== "getters" && !m.modifiers.isReadonly && !has(setterName(m.name))) blocks.push(setter(m, !!opts.fluent));
      }
      if (!blocks.length) throw new Error("No fields selected");
      return [insertAtEnd(src, c, blocks)];
    }
    case "constructor": {
      if (has("__construct")) throw new Error(`Class '${c.name}' already has a constructor`);
      const fields = props.filter((m) => !m.modifiers.isStatic && !m.promoted);
      const params = fields.map((m) => (m.type ? `${m.type} $${m.name}` : `$${m.name}`)).join(", ");
      const assigns = fields.map((m) => `    $this->${m.name} = $${m.name};\n`).join("");
      const block = `public function __construct(${params})\n{\n${assigns}}`;
      const lastField = c.members.filter((m) => !m.promoted && m.kind !== "method").pop();
      const body = indentBlock(block, memberIndent(src, c));
      if (lastField) return [{ start: lastField.end, end: lastField.end, newText: `\n\n${body}` }];
      return [insertAtEnd(src, c, [block])];
    }
    case "toString": {
      if (has("__toString")) throw new Error(`Class '${c.name}' already has a __toString() method`);
      const fields = opts.fields?.length ? props : c.members.filter((m) => m.kind === "property" && !m.modifiers.isStatic);
      let body = `    return '${c.name}{'`;
      fields.forEach((f, i) => (body += ` .\n        '${i ? ", " : ""}${f.name}=' . $this->${f.name}`));
      body += " .\n        '}';";
      return [insertAtEnd(src, c, [`public function __toString(): string\n{\n${body}\n}`])];
    }
    case "overrideMethods":
    case "implementMethods": {
      const targets = methodTargets(file, c).filter((t) => (opts.methods ?? []).some((id) => id === t.id || id === t.name));
      if (!targets.length) throw new Error("No methods selected");
      const blocks = targets.map((t) => {
        const args = [...t.signature.matchAll(/(\.\.\.)?\$(\w+)/g)].map((m) => `${m[1] ?? ""}$${m[2]}`).join(", ");
        const ret = /\):\s*(.+)$/.exec(t.signature)?.[1];
        const call = `parent::${t.name}(${args})`;
        const body = kind === "implementMethods" ? `    // TODO: Implement ${t.name}() method.` : ret === "void" ? `    ${call};` : `    return ${call};`;
        return `${t.visibility}${t.isStatic ? " static" : ""} function ${t.signature}\n{\n${body}\n}`;
      });
      return [insertAtEnd(src, c, blocks)];
    }
  }
  return [];
}

registerMock("php_generate", ({ path, offset, kind, options, content }) =>
  mockGenerate(source(path, content), Number(offset), kind as GenerateKind, (options ?? {}) as GenerateOptions),
);

// The blank line pass needs the tree-sitter PHP parser, which the WebAssembly build does not
// include: browser mode leaves the text as the (mock) language server formatted it.
registerMock("php_tidy_blank_lines", () => null);

// Composer: the demo's composer.json requires PHP ^8.3 (the desktop app resolves the
// constraint in `phpforge_lang_php::composer`, which needs no more than this here).
registerMock("php_language_level", () => {
  const json = mockFiles[String(mockSettingValue("php.composer.json") ?? "composer.json")];
  const sync = mockSettingValue("php.composer.syncWithComposerJson") !== false;
  const required = json?.match(/"php"\s*:\s*"[^"\d]*(\d+\.\d+)/)?.[1];
  if (sync && required) return { level: required, fromComposer: true };
  const level = String(mockSettingValue("php.languageLevel") ?? "8.3");
  if (level === "interpreter") {
    const list = decodeInterpreters(mockSettingValue("php.interpreters") ?? [], mockSettingValue("php.projectInterpreters") ?? []);
    const version = findInterpreter(list, String(mockSettingValue("php.interpreter") ?? "php"))?.version ?? "";
    const fromInterpreter = levelForVersion(version, ["7.4", "8.0", "8.1", "8.2", "8.3", "8.4"]);
    return { level: fromInterpreter ?? "8.3", fromComposer: false, fromInterpreter: !!fromInterpreter };
  }
  return { level, fromComposer: false };
});
registerMock("php_composer_outdated", async () => [
  { name: "laravel/framework", version: "v12.1.0", latest: "v12.3.0", status: "semver-safe-update" },
  { name: "phpunit/phpunit", version: "11.4.0", latest: "12.0.2", status: "update-possible" },
]);

// php-lsp is built in.
const PHP_LSP = { command: "/usr/bin/forge", args: ["--php-lsp", "--stdio"], version: "0.1.0 (7011d4c)" };
registerMock("php_detect_servers", () => ({ phpLsp: PHP_LSP }));
