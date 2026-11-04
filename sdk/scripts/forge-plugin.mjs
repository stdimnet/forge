#!/usr/bin/env node
/**
 * The Forge plugin SDK command line (`pnpm plugin <command>` in app/ui):
 *
 *   new <folder> [--id acme.my-plugin] [--name "My Plugin"] [--publisher Acme]
 *       scaffold a TypeScript plugin project from sdk/templates/basic
 *   build <folder>    bundle src/index.ts into dist/plugin.js (esbuild, IIFE `__forgePlugin`)
 *   pack <folder>     build, then write <id>-<version>.forgeplugin and print its SHA-256
 */
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, renameSync, statSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { pack } from "./pack.mjs";

const sdk = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repo = resolve(sdk, "..");

function option(args, name, fallback) {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] ? args.splice(i, 2)[1] : fallback;
}

const slug = (s) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || "plugin";

function forgeVersion() {
  try {
    return JSON.parse(readFileSync(join(repo, "app/ui/package.json"), "utf8")).version;
  } catch {
    return "0.1.0";
  }
}

function fill(dir, vars) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) fill(p, vars);
    else writeFileSync(p, readFileSync(p, "utf8").replace(/\{\{(\w+)\}\}/g, (m, k) => vars[k] ?? m));
  }
}

/** Scaffold a plugin project; returns its folder. */
export function scaffold(target, opts = {}) {
  const dir = resolve(target);
  if (existsSync(dir) && readdirSync(dir).length) throw new Error(`${dir} is not empty`);
  const name = opts.name ?? basename(dir).replace(/[-_]+/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
  const publisher = opts.publisher ?? "me";
  const id = opts.id ?? `${slug(publisher)}.${slug(name)}`;
  if (!/^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(id) || id.startsWith("forge.")) {
    throw new Error(`invalid plugin id ${id}: use lowercase reverse-DNS like acme.my-plugin`);
  }
  mkdirSync(dir, { recursive: true });
  cpSync(join(sdk, "templates/basic"), dir, { recursive: true });
  renameSync(join(dir, "gitignore"), join(dir, ".gitignore"));
  mkdirSync(join(dir, "scripts"), { recursive: true });
  cpSync(join(sdk, "scripts/pack.mjs"), join(dir, "scripts/pack.mjs"));
  fill(dir, {
    id,
    name,
    publisher,
    description: opts.description ?? `${name}, a Forge plugin.`,
    packageName: `forge-plugin-${slug(name)}`,
    forgeVersion: forgeVersion(),
    pluginApi: `file:${relative(dir, join(sdk, "plugin-api")).split("\\").join("/")}`,
  });
  return { dir, id, name };
}

/** Bundle `src/index.ts` with esbuild (the plugin's own, or the one Vite brings along). */
export async function build(target) {
  const dir = resolve(target);
  const entry = join(dir, "src/index.ts");
  const outfile = join(dir, "dist/plugin.js");
  let esbuild;
  // The plugin's own esbuild, or the one Vite brings along in app/ui.
  for (const from of [join(dir, "package.json"), join(repo, "app/ui/package.json")]) {
    for (const via of [null, "vite"]) {
      try {
        const base = createRequire(from);
        const req = via ? createRequire(base.resolve(via)) : base;
        esbuild = await import(req.resolve("esbuild"));
        break;
      } catch {
        /* try the next location */
      }
    }
    if (esbuild) break;
  }
  if (!esbuild) throw new Error("esbuild not found: run `npm install` in the plugin folder (or `pnpm install` in app/ui)");
  mkdirSync(dirname(outfile), { recursive: true });
  await esbuild.build({
    entryPoints: [entry],
    bundle: true,
    format: "iife",
    globalName: "__forgePlugin",
    target: "es2020",
    charset: "utf8",
    legalComments: "none",
    outfile,
    logLevel: "warning",
    banner: { js: `// ${JSON.parse(readFileSync(join(dir, "forge-plugin.json"), "utf8")).id}: built by the Forge plugin SDK` },
  });
  return outfile;
}

async function main() {
  const [command, ...args] = process.argv.slice(2);
  if (command === "new") {
    const id = option(args, "id");
    const name = option(args, "name");
    const publisher = option(args, "publisher");
    if (!args[0]) throw new Error("usage: plugin new <folder> [--id acme.my-plugin] [--name 'My Plugin'] [--publisher Acme]");
    const r = scaffold(args[0], { id, name, publisher });
    console.log(`created ${r.name} (${r.id}) in ${r.dir}\nnext: cd ${r.dir} && npm install && npm run pack`);
  } else if (command === "build") {
    console.log(`wrote ${await build(args[0] ?? ".")}`);
  } else if (command === "pack") {
    await build(args[0] ?? ".");
    const { file, sha256, size } = pack(args[0] ?? ".");
    console.log(`wrote ${file} (${size} bytes)\nsha256 ${sha256}`);
  } else {
    console.log("usage: plugin <new|build|pack> <folder>");
    process.exitCode = command ? 1 : 0;
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  main().catch((e) => {
    console.error(e instanceof Error ? e.message : e);
    process.exitCode = 1;
  });
}
