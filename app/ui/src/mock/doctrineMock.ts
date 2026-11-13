/**
 * Browser-mode Doctrine backend over the Symfony demo project: entity metadata as
 * `phpforge-orm-doctrine` reports it. DQL completion, navigation, diagnostics and highlighting
 * run the Rust `phpforge-dql` crate (WebAssembly) over these mappings, like the desktop app.
 */
import { registerMock } from "@/ipc";
import type { DoctrineAssociation, DoctrineEntity, DoctrineField, DoctrineGutterMark, DqlCompletion, DqlDiagnostic, DqlHighlight } from "@/ipc/doctrine";
import type { SourceLocation } from "@/ipc/symfony";
import { callWasm, forgeWasm, type ForgeWasm } from "@/wasm";
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

const short = (fqn: string) => fqn.split("\\").pop()!;

/** The demo mappings as a `phpforge_dql::model::Snapshot`. */
export const doctrineSnapshot = () => ({
  entities: demoEntities,
  repositories: Object.entries(repoEntity).map(([path, name]) => ({
    class: `App\\Repository\\${name}Repository`,
    entity: E + name,
    path,
    line: lineOf(path, "class "),
  })),
});

/** Run a `dql_*` bridge function for a PHP file of the demo project. */
async function dql<T>(fn: (w: ForgeWasm) => (json: string) => string, args: Record<string, unknown>, empty: T): Promise<T> {
  if (!isSymfonyDemo()) return empty;
  const wasm = await forgeWasm();
  return callWasm<T>(fn(wasm), { path: String(args.path), content: String(args.content), offset: Number(args.offset ?? 0), snapshot: doctrineSnapshot() });
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
registerMock("doctrine_complete", (a) => dql<DqlCompletion | null>((w) => w.dql_complete, a, null));
registerMock("doctrine_goto", (a) => dql<SourceLocation[]>((w) => w.dql_goto, a, []));
registerMock("doctrine_diagnostics", (a) => dql<DqlDiagnostic[]>((w) => w.dql_diagnostics, a, []));
registerMock("doctrine_highlight", (a) => dql<DqlHighlight>((w) => w.dql_highlight, a, { injected: [], spans: [] }));
registerMock("doctrine_gutter", ({ path }) => (isSymfonyDemo() ? gutter(String(path)) : []));
