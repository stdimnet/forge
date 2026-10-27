/**
 * The demo `shop` database of the browser mock: a Laravel-style MySQL schema with users,
 * products, orders and order items. Rows are generated deterministically so screenshots
 * stay stable.
 */
import type { Cell, RoutineInfo, TableInfo } from "@/ipc/db";

export interface DemoTable {
  info: TableInfo;
  rows: Cell[][];
}

export interface DemoDatabase {
  tables: Record<string, DemoTable>;
  routines: RoutineInfo[];
}

/** Small deterministic PRNG (mulberry32). */
function rng(seed: number) {
  let a = seed;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const pad = (n: number) => String(n).padStart(2, "0");

function stamp(dayOffset: number, minutes: number): string {
  const d = new Date(Date.UTC(2025, 0, 6) + dayOffset * 86400000 + minutes * 60000);
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`;
}

const FIRST = ["Ada", "Alan", "Grace", "Linus", "Margaret", "Dennis", "Barbara", "Ken", "Radia", "Tim", "Frances", "John", "Hedy", "Donald", "Katherine", "Guido", "Sophie", "Bjarne", "Anita", "James", "Shafi", "Edsger", "Jean", "Niklaus"];
const LAST = ["Lovelace", "Turing", "Hopper", "Torvalds", "Hamilton", "Ritchie", "Liskov", "Thompson", "Perlman", "Berners-Lee", "Allen", "McCarthy", "Lamarr", "Knuth", "Johnson", "van Rossum", "Wilson", "Stroustrup", "Borg", "Gosling", "Goldwasser", "Dijkstra", "Sammet", "Wirth"];

const PRODUCTS: [string, string, number][] = [
  ["Forge Hoodie", "Apparel", 59.0],
  ["Anvil Coffee Mug", "Kitchen", 14.5],
  ["Ember T-Shirt", "Apparel", 24.99],
  ["Tongs Bottle Opener", "Kitchen", 9.9],
  ["Mechanical Keyboard", "Electronics", 149.0],
  ["USB-C Dock", "Electronics", 89.0],
  ["Laravel Up & Running", "Books", 39.99],
  ["Symfony 7 Recipes", "Books", 34.0],
  ["Refactoring (2nd ed.)", "Books", 47.25],
  ["Desk Mat XL", "Accessories", 29.0],
  ["Cable Organizer", "Accessories", 12.0],
  ["Standing Desk Frame", "Furniture", 389.0],
  ["Monitor Arm", "Furniture", 119.0],
  ["Noise-Cancelling Headphones", "Electronics", 279.0],
  ["Sticker Pack", "Accessories", 4.5],
  ["Hammer Pen", "Accessories", 7.9],
  ["Clean Code", "Books", 41.0],
  ["Ergonomic Mouse", "Electronics", 69.0],
  ["Forge Beanie", "Apparel", 19.0],
  ["Spark Notebook", "Accessories", 11.5],
  ["Webcam 4K", "Electronics", 129.0],
  ["Lamp", "Furniture", 49.0],
  ["Debugging Teddy Bear", "Accessories", 22.0],
  ["Coffee Beans 1kg", "Kitchen", 26.0],
];

/** A 16×16 PNG (an ember on a dark background) for the avatar column's image preview. */
const AVATAR_PNG =
  "89504e470d0a1a0a0000000d494844520000001000000010080200000090916836000000924944415478dab552bb0a853014eb776474f22fdd1c1c0497d2416e11a182f88ff118900b72ef70142143d32487f411aaaa76213c0d700517b008b658f137c04d8e19cce047c8a2b6b9e157c084091cc1044621894e47e61a389acc92cd3780bd30888e92ce6ee13b3e6ba4993ab0153ad12469b9048a4a470d366b23b4a25152791a7057f21eda7dad771ecefd35defaad3b33fb0af498a8d9fb0000000049454e44ae426082";

const STATUSES = ["delivered", "delivered", "shipped", "paid", "pending", "cancelled"];

const col = (name: string, dataType: string, opts: Partial<TableInfo["columns"][number]> = {}) => ({
  name,
  dataType,
  nullable: false,
  autoIncrement: false,
  ...opts,
});

const timestamps = () => [col("created_at", "timestamp", { nullable: true }), col("updated_at", "timestamp", { nullable: true })];

export function createDemoDatabase(): DemoDatabase {
  const r = rng(42);
  const pick = <T,>(xs: T[]) => xs[Math.floor(r() * xs.length)];

  const users: Cell[][] = [];
  for (let i = 1; i <= 48; i++) {
    // The first users are famous pairs; later ones mix first and last names.
    const first = FIRST[(i - 1) % FIRST.length];
    const last = i <= LAST.length ? LAST[i - 1] : LAST[(i * 5 + 3) % LAST.length];
    const suffix = i <= LAST.length ? "" : String(i);
    const created = stamp(i * 3, Math.floor(r() * 1400));
    users.push([
      i,
      `${first} ${last}`,
      `${first.toLowerCase()}.${last.toLowerCase().replace(/[^a-z]/g, "")}${suffix}@example.com`,
      i % 5 === 0 ? null : stamp(i * 3, 90),
      "$2y$12$" + Math.floor(r() * 1e16).toString(36).padEnd(11, "x") + "Qe7rJb1aM9cW",
      i <= 2 ? "admin" : "customer",
      (Math.round(r() * 50000) / 100).toFixed(2),
      i % 4 === 1 ? { $hex: AVATAR_PNG } : null,
      i % 3 === 0 ? null : Math.floor(r() * 1e12).toString(36),
      created,
      i % 6 === 0 ? stamp(i * 3 + 20, 300) : created,
    ]);
  }

  const products: Cell[][] = PRODUCTS.map(([name, category, price], i) => [
    i + 1,
    `${category.slice(0, 3).toUpperCase()}-${String(1000 + i * 17)}`,
    name,
    price.toFixed(2),
    Math.floor(r() * 240),
    category,
    i !== 14,
    i % 3 === 0 ? null : `${name} from the Forge merch store.`,
    stamp(i, 60),
    stamp(i + 30, 120),
  ]);

  const orders: Cell[][] = [];
  const items: Cell[][] = [];
  let itemId = 1;
  for (let i = 1; i <= 120; i++) {
    const user = 1 + Math.floor(r() * 40);
    const status = pick(STATUSES);
    const day = 20 + Math.floor(i * 2.3);
    const lines = 1 + Math.floor(r() * 3);
    let total = 0;
    for (let l = 0; l < lines; l++) {
      const p = Math.floor(r() * PRODUCTS.length);
      const qty = 1 + Math.floor(r() * 3);
      const price = PRODUCTS[p][2];
      total += qty * price;
      items.push([itemId++, i, p + 1, qty, price.toFixed(2)]);
    }
    orders.push([
      i,
      user,
      `SO-2025-${String(i).padStart(5, "0")}`,
      status,
      total.toFixed(2),
      "EUR",
      stamp(day, Math.floor(r() * 1400)),
      status === "shipped" || status === "delivered" ? stamp(day + 2, 600) : null,
      i % 9 === 0 ? "Gift wrap, please" : null,
    ]);
  }

  const migrations: Cell[][] = [
    "0001_01_01_000000_create_users_table",
    "0001_01_01_000001_create_cache_table",
    "0001_01_01_000002_create_jobs_table",
    "2025_01_10_120000_create_products_table",
    "2025_01_10_120100_create_orders_table",
    "2025_01_10_120200_create_order_items_table",
    "2025_02_03_091500_add_role_to_users_table",
  ].map((m, i) => [i + 1, m, i < 3 ? 1 : i < 6 ? 2 : 3]);

  const audit: Cell[][] = Array.from({ length: 12 }, (_, i) => [
    stamp(60 + i, i * 37),
    i % 2 ? "ada.lovelace@example.com" : "system",
    pick(["order.created", "order.shipped", "user.login", "product.updated"]),
    JSON.stringify({
      id: 1 + i * 3,
      ip: `10.0.0.${10 + i}`,
      agent: i % 3 ? "Forge/1.0 (Linux)" : "curl/8.7",
      tags: ["shop", i % 2 ? "web" : "cli"],
      meta: { attempt: (i % 3) + 1, ok: i % 5 !== 0, ms: 12 + ((i * 7) % 40) },
    }),
  ]);

  const tables: Record<string, DemoTable> = {
    audit_log: {
      info: {
        name: "audit_log",
        kind: "table",
        comment: "Append-only activity log",
        columns: [col("occurred_at", "timestamp"), col("actor", "varchar(255)"), col("action", "varchar(64)"), col("payload", "json", { nullable: true })],
        primaryKey: [],
        foreignKeys: [],
        indexes: [{ name: "audit_log_occurred_at_index", columns: ["occurred_at"], unique: false, primary: false }],
        rowEstimate: audit.length,
      },
      rows: audit,
    },
    migrations: {
      info: {
        name: "migrations",
        kind: "table",
        columns: [col("id", "int unsigned", { autoIncrement: true }), col("migration", "varchar(255)"), col("batch", "int")],
        primaryKey: ["id"],
        foreignKeys: [],
        indexes: [{ name: "PRIMARY", columns: ["id"], unique: true, primary: true }],
        rowEstimate: migrations.length,
      },
      rows: migrations,
    },
    order_items: {
      info: {
        name: "order_items",
        kind: "table",
        columns: [
          col("id", "bigint unsigned", { autoIncrement: true }),
          col("order_id", "bigint unsigned"),
          col("product_id", "bigint unsigned"),
          col("quantity", "int unsigned", { default: "1" }),
          col("unit_price", "decimal(10,2)"),
        ],
        primaryKey: ["id"],
        foreignKeys: [
          { name: "order_items_order_id_foreign", columns: ["order_id"], refSchema: "shop", refTable: "orders", refColumns: ["id"], onDelete: "CASCADE", onUpdate: "NO ACTION" },
          { name: "order_items_product_id_foreign", columns: ["product_id"], refSchema: "shop", refTable: "products", refColumns: ["id"], onDelete: "RESTRICT", onUpdate: "NO ACTION" },
        ],
        indexes: [
          { name: "PRIMARY", columns: ["id"], unique: true, primary: true },
          { name: "order_items_order_id_foreign", columns: ["order_id"], unique: false, primary: false },
          { name: "order_items_product_id_foreign", columns: ["product_id"], unique: false, primary: false },
        ],
        rowEstimate: items.length,
      },
      rows: items,
    },
    orders: {
      info: {
        name: "orders",
        kind: "table",
        columns: [
          col("id", "bigint unsigned", { autoIncrement: true }),
          col("user_id", "bigint unsigned"),
          col("number", "varchar(20)"),
          col("status", "enum('pending','paid','shipped','delivered','cancelled')", { default: "pending" }),
          col("total", "decimal(10,2)", { default: "0.00" }),
          col("currency", "char(3)", { default: "EUR" }),
          col("placed_at", "timestamp"),
          col("shipped_at", "timestamp", { nullable: true }),
          col("notes", "text", { nullable: true }),
        ],
        primaryKey: ["id"],
        foreignKeys: [{ name: "orders_user_id_foreign", columns: ["user_id"], refSchema: "shop", refTable: "users", refColumns: ["id"], onDelete: "CASCADE", onUpdate: "NO ACTION" }],
        indexes: [
          { name: "PRIMARY", columns: ["id"], unique: true, primary: true },
          { name: "orders_number_unique", columns: ["number"], unique: true, primary: false },
          { name: "orders_user_id_status_index", columns: ["user_id", "status"], unique: false, primary: false },
        ],
        rowEstimate: orders.length,
      },
      rows: orders,
    },
    products: {
      info: {
        name: "products",
        kind: "table",
        columns: [
          col("id", "bigint unsigned", { autoIncrement: true }),
          col("sku", "varchar(32)"),
          col("name", "varchar(255)"),
          col("price", "decimal(10,2)"),
          col("stock", "int unsigned", { default: "0" }),
          col("category", "varchar(64)"),
          col("is_active", "tinyint(1)", { default: "1" }),
          col("description", "text", { nullable: true }),
          ...timestamps(),
        ],
        primaryKey: ["id"],
        foreignKeys: [],
        indexes: [
          { name: "PRIMARY", columns: ["id"], unique: true, primary: true },
          { name: "products_sku_unique", columns: ["sku"], unique: true, primary: false },
          { name: "products_category_index", columns: ["category"], unique: false, primary: false },
        ],
        rowEstimate: products.length,
      },
      rows: products,
    },
    users: {
      info: {
        name: "users",
        kind: "table",
        columns: [
          col("id", "bigint unsigned", { autoIncrement: true }),
          col("name", "varchar(255)"),
          col("email", "varchar(255)"),
          col("email_verified_at", "timestamp", { nullable: true }),
          col("password", "varchar(255)"),
          col("role", "enum('customer','admin')", { default: "customer" }),
          col("balance", "decimal(10,2)", { default: "0.00" }),
          col("avatar", "blob", { nullable: true }),
          col("remember_token", "varchar(100)", { nullable: true }),
          ...timestamps(),
        ],
        primaryKey: ["id"],
        foreignKeys: [],
        indexes: [
          { name: "PRIMARY", columns: ["id"], unique: true, primary: true },
          { name: "users_email_unique", columns: ["email"], unique: true, primary: false },
        ],
        rowEstimate: users.length,
      },
      rows: users,
    },
    customer_totals: {
      info: {
        name: "customer_totals",
        kind: "view",
        columns: [col("user_id", "bigint unsigned"), col("name", "varchar(255)"), col("orders", "bigint"), col("spent", "decimal(32,2)", { nullable: true })],
        primaryKey: [],
        foreignKeys: [],
        indexes: [],
      },
      rows: [],
    },
  };

  // The view is computed once from the generated data.
  const spent = new Map<number, { n: number; total: number }>();
  for (const o of orders) {
    const u = o[1] as number;
    const e = spent.get(u) ?? { n: 0, total: 0 };
    e.n++;
    e.total += Number(o[4]);
    spent.set(u, e);
  }
  tables.customer_totals.rows = [...spent.entries()]
    .sort((a, b) => b[1].total - a[1].total)
    .map(([u, e]) => [u, users[u - 1][1], e.n, e.total.toFixed(2)]);

  return {
    tables,
    routines: [
      { name: "close_stale_orders", kind: "procedure" },
      { name: "order_total", kind: "function", returns: "decimal(10,2)" },
    ],
  };
}

/** MySQL result type name of a column type (`varchar(255)` → `VARCHAR`). */
export function resultTypeName(dataType: string): string {
  const t = dataType.toLowerCase();
  if (t.startsWith("tinyint(1)")) return "BOOLEAN";
  const base = t.split(/[ (]/)[0].toUpperCase();
  const unsigned = t.includes("unsigned") ? " UNSIGNED" : "";
  if (base === "ENUM") return "ENUM";
  return base + unsigned;
}
