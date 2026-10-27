/**
 * Browser-mode demo data and tiny interpreters for the Redis and MongoDB data sources of the
 * database mock: a Redis keyspace (sessions, users, queues, a leaderboard, a stream) with a
 * command console, and a MongoDB `shop` database (users, orders, products) with the shell
 * calls the console understands (`find`, `aggregate`, `countDocuments`, …). Deterministic so
 * screenshots stay stable. Never touches the project.
 */
import type { Cell, ColumnMeta, RedisKeys, RedisValue, SchemaInfo, TableInfo } from "@/ipc/db";

export interface RawResult {
  columns: ColumnMeta[];
  rows: Cell[][];
  rowsAffected: number;
  documents?: unknown[];
}

// ---- Redis -------------------------------------------------------------------------------

type RVal =
  | { type: "string"; v: string }
  | { type: "list"; v: string[] }
  | { type: "hash"; v: Record<string, string> }
  | { type: "set"; v: string[] }
  | { type: "zset"; v: [string, number][] }
  | { type: "stream"; v: { id: string; fields: Record<string, string> }[] };

interface RKey {
  value: RVal;
  ttlMs: number;
}

export type RedisDemo = Map<number, Map<string, RKey>>;

export function createRedisDemo(): RedisDemo {
  const db0 = new Map<string, RKey>([
    ["session:9f2c1a7e", { value: { type: "string", v: JSON.stringify({ userId: 1, csrf: "Xq81fZ", locale: "en", cart: [3, 7] }) }, ttlMs: 1_740_000 }],
    ["session:b71e04d2", { value: { type: "string", v: JSON.stringify({ userId: 7, csrf: "pL02kd", locale: "de", cart: [] }) }, ttlMs: 712_000 }],
    ["user:1", { value: { type: "hash", v: { name: "Ada Lovelace", email: "ada.lovelace@example.com", plan: "pro", logins: "128" } }, ttlMs: -1 }],
    ["user:2", { value: { type: "hash", v: { name: "Alan Turing", email: "alan.turing@example.com", plan: "free", logins: "54" } }, ttlMs: -1 }],
    ["user:7", { value: { type: "hash", v: { name: "Barbara Liskov", email: "barbara.liskov@example.com", plan: "team", logins: "312" } }, ttlMs: -1 }],
    ["user:1:roles", { value: { type: "set", v: ["admin", "billing", "editor"] }, ttlMs: -1 }],
    [
      "queue:emails",
      {
        value: {
          type: "list",
          v: [
            JSON.stringify({ job: "OrderShipped", to: "ada.lovelace@example.com", order: 118 }),
            JSON.stringify({ job: "PasswordReset", to: "alan.turing@example.com" }),
            JSON.stringify({ job: "Invoice", to: "grace.hopper@example.com", order: 117 }),
            JSON.stringify({ job: "Welcome", to: "tim.bernerslee@example.com" }),
          ],
        },
        ttlMs: -1,
      },
    ],
    [
      "leaderboard:weekly",
      {
        value: {
          type: "zset",
          v: [
            ["barbara.liskov", 9120],
            ["ada.lovelace", 8875.5],
            ["grace.hopper", 8410],
            ["linus.torvalds", 7203],
            ["alan.turing", 6990],
          ],
        },
        ttlMs: 518_400_000,
      },
    ],
    ["cache:home:en", { value: { type: "string", v: "<!doctype html><html lang=\"en\"><head><title>Forge Shop</title></head><body>…</body></html>" }, ttlMs: 241_000 }],
    ["rate:203.0.113.7", { value: { type: "string", v: "42" }, ttlMs: 38_000 }],
    ["feature:flags", { value: { type: "hash", v: { new_checkout: "on", dark_mode: "on", gift_cards: "off" } }, ttlMs: -1 }],
    [
      "stream:orders",
      {
        value: {
          type: "stream",
          v: [
            { id: "1736155800000-0", fields: { order: "116", status: "paid" } },
            { id: "1736155860000-0", fields: { order: "117", status: "paid" } },
            { id: "1736155920000-0", fields: { order: "116", status: "shipped" } },
          ],
        },
        ttlMs: -1,
      },
    ],
  ]);
  const db1 = new Map<string, RKey>([
    ["cache:product:1", { value: { type: "string", v: JSON.stringify({ id: 1, name: "Forge Hoodie", price: 59 }) }, ttlMs: 3_400_000 }],
    ["cache:product:5", { value: { type: "string", v: JSON.stringify({ id: 5, name: "Mechanical Keyboard", price: 149 }) }, ttlMs: 3_100_000 }],
  ]);
  return new Map([
    [0, db0],
    [1, db1],
  ]);
}

function glob(pattern: string, s: string): boolean {
  const re = new RegExp(`^${pattern.replace(/[.+^${}()|\\]/g, "\\$&").replace(/\*/g, ".*").replace(/\?/g, ".")}$`);
  return re.test(s);
}

export const redisDbNumber = (schema: string) => Number(schema.replace(/^db/, "")) || 0;

export function redisSchemas(demo: RedisDemo, current: number) {
  const dbs = [...demo.keys()].filter((n) => (demo.get(n)?.size ?? 0) > 0 || n === current).sort();
  return dbs.map((n) => ({ name: `db${n}`, current: n === current }));
}

export function redisKeys(demo: RedisDemo, schema: string, pattern: string, limit = 1000): RedisKeys {
  const db = redisDbNumber(schema);
  const p = pattern.trim() || "*";
  const all = [...(demo.get(db) ?? new Map<string, RKey>()).entries()].filter(([k]) => glob(p, k)).sort(([a], [b]) => a.localeCompare(b));
  return {
    db,
    pattern: p,
    keys: all.slice(0, limit).map(([key, k]) => ({ key, type: k.value.type, ttlMs: k.ttlMs })),
    truncated: all.length > limit,
  };
}

export function redisValue(demo: RedisDemo, schema: string, key: string): RedisValue {
  const k = demo.get(redisDbNumber(schema))?.get(key);
  if (!k) throw new Error(`key ${key} does not exist`);
  const v = k.value;
  const value =
    v.type === "zset"
      ? v.v.map(([member, score]) => ({ member, score }))
      : v.type === "set"
        ? [...v.v].sort()
        : v.v;
  const length = v.type === "string" ? new TextEncoder().encode(v.v).length : v.type === "hash" ? Object.keys(v.v).length : v.v.length;
  return { key, type: v.type, ttlMs: k.ttlMs, length, value, truncated: false };
}

const words = (line: string): string[] => {
  const out: string[] = [];
  const re = /"((?:[^"\\]|\\.)*)"|'([^']*)'|(\S+)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(line))) out.push(m[1] !== undefined ? m[1].replace(/\\(.)/g, "$1") : (m[2] ?? m[3]));
  return out;
};

const one = (v: Cell): RawResult => ({ columns: [{ name: "value", typeName: "redis" }], rows: [[v]], rowsAffected: 0 });
const list = (items: Cell[]): RawResult => ({
  columns: [
    { name: "#", typeName: "redis" },
    { name: "value", typeName: "redis" },
  ],
  rows: items.map((v, i) => [i + 1, v]),
  rowsAffected: 0,
});
const pairs = (items: [Cell, Cell][]): RawResult => ({
  columns: [
    { name: "field", typeName: "redis" },
    { name: "value", typeName: "redis" },
  ],
  rows: items.map(([a, b]) => [a, b]),
  rowsAffected: 0,
});

/** Run one Redis console line against the demo keyspace (database `db` of the session). */
export function redisCommand(demo: RedisDemo, session: { db: number }, line: string): RawResult {
  const [name = "", ...args] = words(line);
  const cmd = name.toUpperCase();
  let db = demo.get(session.db);
  if (!db) {
    db = new Map();
    demo.set(session.db, db);
  }
  const key = args[0] ?? "";
  const k = db.get(key);
  const wrong = () => {
    throw new Error("WRONGTYPE Operation against a key holding the wrong kind of value");
  };
  switch (cmd) {
    case "PING":
      return one("PONG");
    case "SELECT":
      session.db = Number(args[0]) || 0;
      return one("OK");
    case "DBSIZE":
      return one(db.size);
    case "INFO":
      return one(`# Server\nredis_version:7.2.4\n# Keyspace\n${[...demo.entries()].map(([n, m]) => `db${n}:keys=${m.size}`).join("\n")}`);
    case "KEYS":
      return list(redisKeys(demo, `db${session.db}`, key || "*").keys.map((x) => x.key));
    case "SCAN":
      return list(redisKeys(demo, `db${session.db}`, args[args.findIndex((a) => a.toUpperCase() === "MATCH") + 1] ?? "*").keys.map((x) => x.key));
    case "TYPE":
      return one(k?.value.type ?? "none");
    case "TTL":
      return one(!k ? -2 : k.ttlMs < 0 ? -1 : Math.round(k.ttlMs / 1000));
    case "PTTL":
      return one(!k ? -2 : k.ttlMs);
    case "EXISTS":
      return one(args.filter((a) => db!.has(a)).length);
    case "GET":
      if (k && k.value.type !== "string") wrong();
      return one(k ? (k.value.v as string) : null);
    case "SET":
      db.set(key, { value: { type: "string", v: args[1] ?? "" }, ttlMs: -1 });
      return one("OK");
    case "INCR": {
      const n = Number(k?.value.type === "string" ? k.value.v : 0) + 1;
      db.set(key, { value: { type: "string", v: String(n) }, ttlMs: k?.ttlMs ?? -1 });
      return one(n);
    }
    case "EXPIRE":
      if (k) k.ttlMs = Number(args[1]) * 1000;
      return one(k ? 1 : 0);
    case "DEL":
      return one(args.filter((a) => db!.delete(a)).length);
    case "FLUSHDB": {
      const n = db.size;
      db.clear();
      return { ...one("OK"), rowsAffected: n };
    }
    case "HGETALL":
      if (k && k.value.type !== "hash") wrong();
      return pairs(k ? Object.entries(k.value.v as Record<string, string>) : []);
    case "HGET":
      if (k && k.value.type !== "hash") wrong();
      return one(k ? ((k.value.v as Record<string, string>)[args[1]] ?? null) : null);
    case "HSET": {
      const h = k?.value.type === "hash" ? k.value.v : {};
      for (let i = 1; i + 1 < args.length; i += 2) h[args[i]] = args[i + 1];
      db.set(key, { value: { type: "hash", v: h }, ttlMs: k?.ttlMs ?? -1 });
      return one(Math.floor((args.length - 1) / 2));
    }
    case "LRANGE":
      if (k && k.value.type !== "list") wrong();
      return list(k ? (k.value.v as string[]) : []);
    case "LLEN":
      return one(k?.value.type === "list" ? k.value.v.length : 0);
    case "SMEMBERS":
      if (k && k.value.type !== "set") wrong();
      return list(k ? [...(k.value.v as string[])].sort() : []);
    case "ZRANGE":
      if (k && k.value.type !== "zset") wrong();
      return args.some((a) => a.toUpperCase() === "WITHSCORES")
        ? pairs(k ? (k.value.v as [string, number][]).map(([m, s]) => [m, s]) : [])
        : list(k ? (k.value.v as [string, number][]).map(([m]) => m) : []);
    default:
      throw new Error(`ERR unknown command '${name}', with args beginning with: ${args.slice(0, 2).map((a) => `'${a}'`).join(" ")}`);
  }
}

// ---- MongoDB -----------------------------------------------------------------------------

type Doc = Record<string, unknown>;
export type MongoDemo = Map<string, Map<string, Doc[]>>;

const oid = (n: number) => ({ $oid: `65a0f0c2e4b0a1b2c3d4${n.toString(16).padStart(4, "0")}` });

export function createMongoDemo(): MongoDemo {
  const names = ["Ada Lovelace", "Alan Turing", "Grace Hopper", "Linus Torvalds", "Margaret Hamilton", "Dennis Ritchie", "Barbara Liskov", "Ken Thompson", "Radia Perlman", "Tim Berners-Lee", "Frances Allen", "John McCarthy"];
  const cities: [string, string][] = [
    ["London", "GB"],
    ["Berlin", "DE"],
    ["New York", "US"],
    ["Helsinki", "FI"],
    ["Boston", "US"],
    ["Zurich", "CH"],
  ];
  const users: Doc[] = names.map((name, i) => ({
    _id: oid(i + 1),
    name,
    email: `${name.toLowerCase().replace(/[^a-z ]/g, "").replace(" ", ".")}@example.com`,
    age: 28 + ((i * 7) % 30),
    roles: i < 2 ? ["admin", "customer"] : ["customer"],
    address: { city: cities[i % cities.length][0], country: cities[i % cities.length][1] },
    createdAt: { $date: `2025-01-${String(6 + i).padStart(2, "0")}T09:${String(10 + i * 3).padStart(2, "0")}:00Z` },
  }));
  const statuses = ["delivered", "shipped", "paid", "pending", "cancelled"];
  const orders: Doc[] = Array.from({ length: 24 }, (_, i) => ({
    _id: oid(100 + i),
    userId: oid(1 + ((i * 5) % names.length)),
    status: statuses[(i * 3) % statuses.length],
    total: Math.round((19.9 + ((i * 37.3) % 280)) * 100) / 100,
    items: [{ sku: `APP-${1000 + ((i * 17) % 200)}`, qty: 1 + (i % 3) }],
    placedAt: { $date: `2025-02-${String(1 + (i % 27)).padStart(2, "0")}T1${i % 10}:00:00Z` },
  }));
  const products: Doc[] = [
    ["Forge Hoodie", "Apparel", 59],
    ["Anvil Coffee Mug", "Kitchen", 14.5],
    ["Mechanical Keyboard", "Electronics", 149],
    ["USB-C Dock", "Electronics", 89],
    ["Clean Code", "Books", 41],
    ["Desk Mat XL", "Accessories", 29],
  ].map(([name, category, price], i) => ({ _id: oid(200 + i), name, category, price, tags: [String(category).toLowerCase()], stock: (i * 23) % 90 }));
  return new Map([
    [
      "shop",
      new Map<string, Doc[]>([
        ["orders", orders],
        ["products", products],
        ["users", users],
      ]),
    ],
    ["admin", new Map<string, Doc[]>([["system.version", [{ _id: "featureCompatibilityVersion", version: "7.0" }]]])],
  ]);
}

const typeOf = (v: unknown): string => {
  if (v === null) return "null";
  if (Array.isArray(v)) return "array";
  if (typeof v === "object") {
    const o = v as Doc;
    if ("$oid" in o) return "objectId";
    if ("$date" in o) return "date";
    return "object";
  }
  if (typeof v === "number") return Number.isInteger(v) ? "int" : "double";
  return typeof v === "boolean" ? "bool" : "string";
};

export function mongoSchema(demo: MongoDemo, db: string): SchemaInfo {
  const colls = demo.get(db) ?? new Map<string, Doc[]>();
  const tables: TableInfo[] = [...colls.entries()]
    .filter(([n]) => !n.startsWith("system."))
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([name, docs]) => {
      const fields: TableInfo["columns"] = [];
      for (const d of docs.slice(0, 20))
        for (const [k, v] of Object.entries(d)) {
          const t = typeOf(v);
          const f = fields.find((x) => x.name === k);
          if (!f) fields.push({ name: k, dataType: t, nullable: k !== "_id", autoIncrement: false });
          else if (!f.dataType.split("|").includes(t)) f.dataType += `|${t}`;
        }
      return { name, kind: "collection", columns: fields, primaryKey: ["_id"], foreignKeys: [], indexes: [{ name: "_id_", columns: ["_id"], unique: true, primary: true }], rowEstimate: docs.length };
    });
  return { name: db, tables, routines: [] };
}

/** Shell syntax to JSON (unquoted keys, single quotes, ObjectId(), ISODate()). */
export function relaxedJson(text: string): unknown {
  const t = text
    .replace(/ObjectId\(\s*["']([0-9a-f]+)["']\s*\)/g, '{"$oid":"$1"}')
    .replace(/(?:ISODate|new Date)\(\s*["']([^"']+)["']\s*\)/g, '{"$date":"$1"}')
    .replace(/'([^'\\]*)'/g, (_m, s: string) => JSON.stringify(s))
    .replace(/([{,]\s*)([A-Za-z_$][\w$.]*)\s*:/g, '$1"$2":')
    .replace(/,\s*([}\]])/g, "$1");
  return JSON.parse(t);
}

const get = (d: Doc, path: string): unknown => path.split(".").reduce<unknown>((o, k) => (o && typeof o === "object" ? (o as Doc)[k] : undefined), d);
const scalar = (v: unknown): unknown => (v && typeof v === "object" && "$oid" in (v as Doc) ? (v as Doc).$oid : v && typeof v === "object" && "$date" in (v as Doc) ? (v as Doc).$date : v);

function matches(d: Doc, filter: Doc): boolean {
  return Object.entries(filter).every(([k, cond]) => {
    const v = scalar(get(d, k));
    if (cond && typeof cond === "object" && !Array.isArray(cond) && !("$oid" in (cond as Doc))) {
      return Object.entries(cond as Doc).every(([op, x]) => {
        const want = scalar(x) as number;
        switch (op) {
          case "$gt":
            return (v as number) > want;
          case "$gte":
            return (v as number) >= want;
          case "$lt":
            return (v as number) < want;
          case "$lte":
            return (v as number) <= want;
          case "$ne":
            return v !== want;
          case "$in":
            return (x as unknown[]).map(scalar).includes(v);
          case "$regex":
            return new RegExp(String(x), String((cond as Doc).$options ?? "")).test(String(v));
          case "$options":
            return true;
          default:
            return false;
        }
      });
    }
    const want = scalar(cond);
    return Array.isArray(v) ? v.includes(want) : v === want;
  });
}

const cellOf = (v: unknown): Cell => {
  if (v === undefined || v === null) return null;
  if (typeof v === "object") {
    const o = v as Doc;
    if ("$oid" in o) return `ObjectId("${o.$oid}")`;
    if ("$date" in o) return String(o.$date);
    return JSON.stringify(v);
  }
  return v as Cell;
};

export function documentsResult(docs: Doc[]): RawResult {
  const cols: string[] = [];
  for (const d of docs) for (const k of Object.keys(d)) if (!cols.includes(k)) cols.push(k);
  const i = cols.indexOf("_id");
  if (i > 0) cols.unshift(...cols.splice(i, 1));
  return {
    columns: cols.map((name) => ({ name, typeName: "bson" })),
    rows: docs.map((d) => cols.map((c) => cellOf(d[c]))),
    rowsAffected: 0,
    documents: docs,
  };
}

const names = (name: string, xs: string[]): RawResult => ({ columns: [{ name, typeName: "string" }], rows: xs.map((x) => [x]), rowsAffected: 0 });
const single = (name: string, v: Cell, affected = 0): RawResult => ({ columns: [{ name, typeName: "bson" }], rows: [[v]], rowsAffected: affected });

function aggregate(docs: Doc[], pipeline: Doc[]): Doc[] {
  let out = docs;
  for (const stage of pipeline) {
    const [op, arg] = Object.entries(stage)[0] ?? [];
    if (op === "$match") out = out.filter((d) => matches(d, arg as Doc));
    else if (op === "$limit") out = out.slice(0, Number(arg));
    else if (op === "$sort") {
      const [[k, dir]] = Object.entries(arg as Doc);
      out = [...out].sort((a, b) => (scalar(get(a, k)) as number) > (scalar(get(b, k)) as number) ? Number(dir) : -Number(dir));
    } else if (op === "$group") {
      const spec = arg as Doc;
      const groups = new Map<string, Doc>();
      for (const d of out) {
        const id = typeof spec._id === "string" && spec._id.startsWith("$") ? scalar(get(d, spec._id.slice(1))) : spec._id;
        const key = JSON.stringify(id);
        const g = groups.get(key) ?? { _id: id };
        for (const [field, acc] of Object.entries(spec)) {
          if (field === "_id") continue;
          const [aop, expr] = Object.entries(acc as Doc)[0];
          const val = typeof expr === "string" && expr.startsWith("$") ? Number(scalar(get(d, expr.slice(1)))) : Number(expr);
          if (aop === "$sum") g[field] = Math.round(((g[field] as number) ?? 0) * 100 + val * 100) / 100;
          else if (aop === "$max") g[field] = Math.max((g[field] as number) ?? -Infinity, val);
        }
        groups.set(key, g);
      }
      out = [...groups.values()];
    }
  }
  return out;
}

/** Run one MongoDB console call against the demo databases (`session.db` is the current one). */
export function mongoCommand(demo: MongoDemo, session: { db: string }, line: string): RawResult {
  const text = line.trim().replace(/;$/, "");
  if (/^show\s+(dbs|databases)$/.test(text)) return names("database", [...demo.keys()].sort());
  if (/^show\s+collections$/.test(text)) return names("collection", [...(demo.get(session.db)?.keys() ?? [])].sort());
  const use = /^use\s+(\S+)$/.exec(text);
  if (use) {
    session.db = use[1];
    if (!demo.has(use[1])) demo.set(use[1], new Map());
    return single("result", `switched to db ${use[1]}`);
  }
  const m = /^db\.(?:getCollection\(\s*["']([^"']+)["']\s*\)|([\w.]+?))\.(\w+)\(([\s\S]*?)\)((?:\s*\.\w+\([^)]*\))*)$/.exec(text);
  if (!m) throw new Error("expected db.<collection>.<method>(…), show dbs, show collections or use <db>");
  const [, quoted, plain, method, argText, chainText] = m;
  const coll = quoted ?? plain;
  const db = demo.get(session.db) ?? new Map<string, Doc[]>();
  demo.set(session.db, db);
  const docs = db.get(coll) ?? [];
  const args = argText.trim() ? (relaxedJson(`[${argText}]`) as unknown[]) : [];
  const chain = [...chainText.matchAll(/\.(\w+)\(([^)]*)\)/g)].map(([, n, a]) => [n, a.trim() ? relaxedJson(a) : undefined] as const);
  switch (method) {
    case "find":
    case "findOne": {
      let out = docs.filter((d) => matches(d, (args[0] as Doc) ?? {}));
      let limit = method === "findOne" ? 1 : 1000;
      for (const [n, a] of chain) {
        if (n === "sort") out = aggregate(out, [{ $sort: a as Doc }]);
        else if (n === "limit") limit = Number(a);
        else if (n === "skip") out = out.slice(Number(a));
      }
      return documentsResult(out.slice(0, limit));
    }
    case "aggregate":
      return documentsResult(aggregate(docs, (args[0] as Doc[]) ?? []));
    case "countDocuments":
    case "count":
      return single("count", docs.filter((d) => matches(d, (args[0] as Doc) ?? {})).length);
    case "estimatedDocumentCount":
      return single("count", docs.length);
    case "distinct":
      return names(String(args[0]), [...new Set(docs.map((d) => String(scalar(get(d, String(args[0]))))))].sort());
    case "insertOne":
      db.set(coll, [...docs, { _id: oid(900 + docs.length), ...(args[0] as Doc) }]);
      return single("result", "1 document(s) inserted", 1);
    case "deleteMany":
    case "deleteOne": {
      const hit = docs.filter((d) => matches(d, (args[0] as Doc) ?? {}));
      const gone = method === "deleteOne" ? hit.slice(0, 1) : hit;
      db.set(coll, docs.filter((d) => !gone.includes(d)));
      return single("result", `${gone.length} document(s) deleted`, gone.length);
    }
    case "updateMany":
    case "updateOne": {
      const hit = docs.filter((d) => matches(d, (args[0] as Doc) ?? {}));
      const set = ((args[1] as Doc)?.$set ?? {}) as Doc;
      const changed = method === "updateOne" ? hit.slice(0, 1) : hit;
      for (const d of changed) Object.assign(d, set);
      return single("result", `${changed.length} document(s) modified`, changed.length);
    }
    case "getIndexes":
      return documentsResult([{ v: 2, key: { _id: 1 }, name: "_id_" }]);
    default:
      throw new Error(`${method}() is not supported`);
  }
}
