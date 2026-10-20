/**
 * Browser-mode Doctrine backend over the Symfony demo project: entity metadata as
 * `phpforge-orm-doctrine` reports it, and a compact port of its DQL completion and
 * navigation rules for `createQuery()` and QueryBuilder strings.
 */
import { registerMock } from "@/ipc";
import type { DoctrineAssociation, DoctrineEntity, DoctrineField, DoctrineGutterMark, DqlItem } from "@/ipc/doctrine";
import type { SourceLocation } from "@/ipc/symfony";
import { DQL_FUNCTIONS, findDqlSites, sourceToDql, dqlToSource, tokenizeDql, type DqlMode, type DqlToken } from "@/features/doctrine/dql";
import { isSymfonyDemo } from "./demoSymfonyProject";
import { lineOf } from "./symfonyMock";

const E = "App\\Entity\\";

function field(path: string, name: string, type: string, extra: Partial<DoctrineField> = {}): DoctrineField {
  return { name, type, line: lineOf(path, `$${name}`), ...extra };
}

function assoc(path: string, name: string, kind: DoctrineAssociation["kind"], target: string, extra: Partial<DoctrineAssociation> = {}): DoctrineAssociation {
  return { name, kind, target: E + target, line: lineOf(path, `$${name}`), ...extra };
}

function entity(name: string, extra: Partial<DoctrineEntity>, fields: (p: string) => DoctrineField[], assocs: (p: string) => DoctrineAssociation[] = () => []): DoctrineEntity {
  const path = `src/Entity/${name}.php`;
  return {
    class: E + name,
    name,
    path,
    line: lineOf(path, `class ${name}`),
    source: "attribute",
    fields: fields(path),
    associations: assocs(path),
    ...extra,
  };
}

const id = (p: string) => field(p, "id", "integer", { id: true, generated: true });

export const demoEntities: DoctrineEntity[] = [
  entity("Address", { embeddable: true }, (p) => [
    field(p, "street", "string", { length: 120 }),
    field(p, "city", "string", { length: 80 }),
    field(p, "country", "string", { length: 2 }),
  ]),
  entity(
    "Customer",
    { table: "customers", repositoryClass: "App\\Repository\\CustomerRepository" },
    (p) => [id(p), field(p, "email", "string", { length: 180, unique: true }), field(p, "name", "string", { length: 120 })],
    (p) => [assoc(p, "orders", "oneToMany", "Order", { mappedBy: "customer" })],
  ),
  entity(
    "Order",
    { table: "orders", repositoryClass: "App\\Repository\\OrderRepository" },
    (p) => [
      id(p),
      field(p, "status", "string", { length: 32 }),
      field(p, "total", "decimal"),
      field(p, "createdAt", "datetime_immutable"),
      field(p, "paidAt", "datetime_immutable", { nullable: true }),
      field(p, "shippingAddress", E + "Address", { embedded: true }),
    ],
    (p) => [
      assoc(p, "customer", "manyToOne", "Customer", { inversedBy: "orders" }),
      assoc(p, "items", "oneToMany", "OrderItem", { mappedBy: "order" }),
    ],
  ),
  entity(
    "OrderItem",
    { table: "order_items" },
    (p) => [id(p), field(p, "quantity", "integer"), field(p, "price", "decimal")],
    (p) => [assoc(p, "order", "manyToOne", "Order", { inversedBy: "items" }), assoc(p, "product", "manyToOne", "Product")],
  ),
  entity(
    "Product",
    { table: "products", repositoryClass: "App\\Repository\\ProductRepository", source: "annotation" },
    (p) => [
      id(p),
      field(p, "name", "string", { length: 255 }),
      field(p, "slug", "string", { length: 255, unique: true }),
      field(p, "price", "decimal"),
      field(p, "available", "boolean"),
    ],
  ),
];

const byName = (name: string) => {
  const n = name.replace(/\\\\/g, "\\").replace(/^\\/, "");
  const short = n.split(/[\\:]/).pop()!.toLowerCase();
  return demoEntities.find((e) => e.class.toLowerCase() === n.toLowerCase()) ?? demoEntities.find((e) => e.name.toLowerCase() === short);
};

const repoEntity: Record<string, string> = {
  "src/Repository/OrderRepository.php": "Order",
  "src/Repository/ProductRepository.php": "Product",
  "src/Repository/CustomerRepository.php": "Customer",
};

/** Aliases the QueryBuilder chain declares (`createQueryBuilder('o')`, `join('o.customer', 'c')`). */
function presetAliases(path: string, src: string): Map<string, DoctrineEntity> {
  const out = new Map<string, DoctrineEntity>();
  const own = repoEntity[path];
  for (const m of src.matchAll(/\$this->createQueryBuilder\(\s*'(\w+)'/g)) if (own) out.set(m[1], byName(own)!);
  for (const m of src.matchAll(/->from\(\s*([\w\\]+)::class\s*,\s*'(\w+)'/g)) {
    const e = byName(m[1]);
    if (e) out.set(m[2], e);
  }
  for (let pass = 0; pass < 2; pass++) {
    for (const m of src.matchAll(/->(?:join|innerJoin|leftJoin)\(\s*'(\w+)\.(\w+)'\s*,\s*'(\w+)'/g)) {
      const t = out.get(m[1])?.associations.find((a) => a.name === m[2]);
      const e = t && byName(t.target);
      if (e) out.set(m[3], e);
    }
  }
  return out;
}

/** Aliases declared in a DQL text (`FROM App\Entity\Order o JOIN o.customer c`). */
function dqlAliases(tokens: DqlToken[], into: Map<string, DoctrineEntity>) {
  const sig = tokens.filter((t) => t.kind !== "whitespace");
  sig.forEach((t, i) => {
    if (t.kind !== "keyword" || !["FROM", "JOIN", "UPDATE"].includes(t.text.toUpperCase())) return;
    const target = sig[i + 1];
    let k = i + 2;
    if (sig[k]?.text.toUpperCase() === "AS") k++;
    const alias = sig[k];
    if (target?.kind !== "identifier" || alias?.kind !== "identifier") return;
    const e = target.text.includes(".") && !target.text.includes("\\") ? resolvePath(into, target.text) : byName(target.text);
    if (e) into.set(alias.text, e);
  });
}

function resolvePath(aliases: Map<string, DoctrineEntity>, path: string): DoctrineEntity | undefined {
  const [first, ...rest] = path.split(".");
  let e = aliases.get(first);
  for (const part of rest) {
    if (!e) return undefined;
    const a = e.associations.find((x) => x.name === part);
    const f = e.fields.find((x) => x.name === part && x.embedded);
    e = a ? byName(a.target) : f ? byName(f.type) : undefined;
  }
  return e;
}

const short = (fqn: string) => fqn.split("\\").pop()!;

function members(e: DoctrineEntity): DqlItem[] {
  return [
    ...e.fields.map((f): DqlItem => ({ label: f.name, kind: f.embedded ? "embedded" : "field", detail: f.embedded ? short(f.type) : f.type })),
    ...e.associations.map((a): DqlItem => ({
      label: a.name,
      kind: "association",
      detail: a.kind === "oneToMany" || a.kind === "manyToMany" ? `Collection<${short(a.target)}>` : short(a.target),
    })),
  ];
}

const entityItems = (): DqlItem[] =>
  demoEntities.filter((e) => !e.embeddable).map((e) => ({ label: e.name, kind: "entity", detail: e.class, apply: e.class }));

const KEYWORDS = ["SELECT", "FROM", "WHERE", "JOIN", "LEFT JOIN", "INNER JOIN", "WITH", "AND", "OR", "NOT", "IN", "IS NULL", "IS NOT NULL", "LIKE", "BETWEEN", "MEMBER OF", "INSTANCE OF", "ORDER BY", "GROUP BY", "HAVING", "ASC", "DESC", "AS", "DISTINCT", "NEW", "CASE", "WHEN", "THEN", "ELSE", "END"];
const kw = (words: string[]): DqlItem[] => words.map((label) => ({ label, kind: "keyword" }));

/** Port of `phpforge_orm_doctrine::dql::complete` for the demo entities. */
export function completeDql(dql: string, cursor: number, mode: DqlMode, preset: Map<string, DoctrineEntity>): { from: number; items: DqlItem[] } {
  const aliases = new Map(preset);
  dqlAliases(tokenizeDql(dql), aliases);
  const tokens = tokenizeDql(dql.slice(0, cursor));
  const last = tokens[tokens.length - 1];
  if (last && last.end === cursor && (last.kind === "parameter" || (last.kind === "string" && (last.text.length < 2 || !last.text.endsWith(last.text[0]))))) {
    return { from: cursor, items: [] };
  }
  const wordTok = last && last.end === cursor && ["identifier", "keyword", "function"].includes(last.kind) ? last : undefined;
  const word = wordTok?.text ?? "";
  const wordStart = wordTok?.start ?? cursor;
  const prev = tokens.filter((t) => t.kind !== "whitespace" && t.end <= wordStart);
  const kwAt = (n: number) => {
    const t = prev[prev.length - 1 - n];
    return t?.kind === "keyword" ? t.text.toUpperCase() : "";
  };
  const p0 = prev.length ? kwAt(0) : mode === "entity" ? "FROM" : mode === "join" ? "JOIN" : "";
  const finish = (from: number, prefix: string, items: DqlItem[]) => {
    const p = prefix.toLowerCase();
    return { from, items: items.filter((i) => !p || i.label.toLowerCase().startsWith(p) || (i.kind === "entity" && i.detail?.toLowerCase().startsWith(p))) };
  };
  const dot = word.lastIndexOf(".");
  if (dot >= 0) {
    const e = resolvePath(aliases, word.slice(0, dot));
    let items = e ? members(e) : [];
    if (p0 === "JOIN") items = items.filter((i) => i.kind === "association");
    return finish(wordStart + dot + 1, word.slice(dot + 1), items);
  }
  let items: DqlItem[] = [];
  if (p0 === "FROM" || p0 === "UPDATE") items = entityItems();
  else if (p0 === "JOIN") {
    for (const [a, e] of aliases) for (const x of e.associations) items.push({ label: `${a}.${x.name}`, kind: "association", detail: short(x.target) });
    items.push(...entityItems());
  } else if (p0 === "ORDER" || p0 === "GROUP") items = kw(["BY"]);
  else if (p0 === "LEFT" || p0 === "INNER") items = kw(["JOIN"]);
  else if (p0 === "IS") items = kw(["NULL", "NOT NULL", "EMPTY", "NOT EMPTY"]);
  else {
    const lastTok = prev[prev.length - 1];
    const before = prev[prev.length - 2]?.text.toUpperCase();
    const declared = lastTok?.kind === "identifier" && ["FROM", "JOIN", "UPDATE", ","].includes(before ?? "") ? (lastTok.text.includes(".") ? resolvePath(aliases, lastTok.text) : byName(lastTok.text)) : undefined;
    if (declared) {
      const n = declared.name;
      items = [
        { label: n[0].toLowerCase(), kind: "alias", detail: n },
        { label: n[0].toLowerCase() + n.slice(1), kind: "alias", detail: n },
        ...kw(["AS", "INDEX BY", "WITH"]),
      ];
    } else {
      items = [...[...aliases].reverse().map(([a, e]): DqlItem => ({ label: a, kind: "alias", detail: e.name }))];
      items.push(...DQL_FUNCTIONS.map(([label, detail]): DqlItem => ({ label, kind: "function", detail })));
      items.push(...kw(KEYWORDS));
    }
  }
  return finish(wordStart, word, items);
}

function siteAt(path: string, src: string, offset: number) {
  const site = findDqlSites(src).find((s) => sourceToDql(s, offset) != null);
  if (!site) return null;
  const preset = site.mode === "full" ? new Map<string, DoctrineEntity>() : presetAliases(path, src);
  return { site, preset, cursor: sourceToDql(site, offset)! };
}

function complete(path: string, src: string, offset: number) {
  const s = siteAt(path, src, offset);
  if (!s) return null;
  const c = completeDql(s.site.dql, s.cursor, s.site.mode, s.preset);
  return { from: dqlToSource(s.site, c.from) ?? offset, items: c.items };
}

function gotoDecl(path: string, src: string, offset: number): SourceLocation[] {
  const s = siteAt(path, src, offset);
  if (!s) return [];
  const aliases = new Map(s.preset);
  const tokens = tokenizeDql(s.site.dql);
  dqlAliases(tokens, aliases);
  const tok = tokens.find((t) => t.kind === "identifier" && t.start <= s.cursor && s.cursor <= t.end);
  if (!tok) return [];
  if (!tok.text.includes(".")) {
    const e = byName(tok.text) ?? aliases.get(tok.text);
    return e ? [{ path: e.path, line: e.line }] : [];
  }
  const rel = s.cursor - tok.start;
  const end = tok.text.indexOf(".", rel);
  const parts = tok.text.slice(0, end < 0 ? undefined : end).split(".");
  const owner = parts.length === 1 ? undefined : resolvePath(aliases, parts.slice(0, -1).join("."));
  if (!owner) {
    const e = aliases.get(parts[0]);
    return e ? [{ path: e.path, line: e.line }] : [];
  }
  const name = parts[parts.length - 1];
  const line = owner.fields.find((f) => f.name === name)?.line ?? owner.associations.find((a) => a.name === name)?.line;
  return line == null ? [] : [{ path: owner.path, line }];
}

function gutter(path: string): DoctrineGutterMark[] {
  const e = demoEntities.find((x) => x.path === path);
  if (e?.repositoryClass) {
    const repoPath = `src/Repository/${short(e.repositoryClass)}.php`;
    return [
      {
        line: e.line,
        kind: "entity",
        title: `Doctrine entity (table ${e.table}). Go to repository ${short(e.repositoryClass)}`,
        targets: [{ path: repoPath, line: lineOf(repoPath, "class ") }],
      },
    ];
  }
  const owned = repoEntity[path] && byName(repoEntity[path]);
  if (owned) {
    return [{ line: lineOf(path, "class "), kind: "repository", title: `Repository of ${owned.name}. Go to entity`, targets: [{ path: owned.path, line: owned.line }] }];
  }
  return [];
}

registerMock("doctrine_entities", () => (isSymfonyDemo() ? structuredClone(demoEntities) : []));
registerMock("doctrine_complete", ({ path, content, offset }) => complete(String(path), String(content), Number(offset)));
registerMock("doctrine_goto", ({ path, content, offset }) => gotoDecl(String(path), String(content), Number(offset)));
/** Unknown fields in `alias.field` paths, like `phpforge_orm_doctrine::dql::diagnostics`. */
export function mockDqlDiagnostics(path: string, src: string): { start: number; end: number; message: string }[] {
  const out: { start: number; end: number; message: string }[] = [];
  for (const site of findDqlSites(src)) {
    const aliases = site.mode === "full" ? new Map<string, DoctrineEntity>() : presetAliases(path, src);
    const tokens = tokenizeDql(site.dql);
    dqlAliases(tokens, aliases);
    if (!aliases.size) continue;
    for (const t of tokens) {
      if (t.kind !== "identifier" || !t.text.includes(".") || t.text.includes("\\")) continue;
      const parts = t.text.split(".");
      let e = aliases.get(parts[0]);
      if (!e) continue;
      let offset = parts[0].length + 1;
      for (const part of parts.slice(1)) {
        if (!part) break;
        const a = e.associations.find((x) => x.name === part);
        const f = e.fields.find((x) => x.name === part);
        if (!a && !f) {
          const start = dqlToSource(site, t.start + offset);
          if (start != null) out.push({ start, end: start + part.length, message: `Field '${part}' not found in ${e.name}` });
          break;
        }
        const next = a ? byName(a.target) : f?.embedded ? byName(f.type) : undefined;
        if (!next) break;
        e = next;
        offset += part.length + 1;
      }
    }
  }
  return out;
}

registerMock("doctrine_diagnostics", ({ path, content }) => (isSymfonyDemo() ? mockDqlDiagnostics(String(path), String(content)) : []));
registerMock("doctrine_gutter", ({ path }) => (isSymfonyDemo() ? gutter(String(path)) : []));

