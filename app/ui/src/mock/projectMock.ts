/**
 * Browser-mode project detection and New Project wizard. Detection describes the demo
 * Laravel project; creation only simulates progress and never writes files.
 */
import { emitMock, registerMock } from "@/ipc";
import {
  CREATE_EVENT,
  type CreateArgs,
  type CreateEvent,
  type Generator,
  type OptionValue,
  type Plan,
  type ProjectInfo,
} from "@/ipc/project";
import { validateName, joinPath } from "@/features/new-project/wizard";
import { demoProjectName } from "./demoProject";
import { setSetting } from "./settingsMock";
import generators from "./projectGenerators.json";

const gens = generators as unknown as Generator[];

export const demoProjectInfo: ProjectInfo = {
  languages: ["php", "javascript", "css"],
  frameworks: [
    { id: "laravel", name: "Laravel", version: "12.4.1", language: "php" },
    { id: "vite", name: "Vite", version: "6.2.0", language: "javascript" },
    { id: "phpunit", name: "PHPUnit", version: "11.5.3", language: "php" },
  ],
  packs: ["phpforge", "webforge"],
  tools: ["git", "composer", "npm", "docker"],
  primary: { id: "laravel", name: "Laravel", version: "12.4.1", language: "php" },
};

const fw = (id: string, name: string, version: string, language: string) => ({
  id,
  name,
  version: version || undefined,
  language,
});

registerMock("project_detect", () => structuredClone(demoProjectInfo));
registerMock("project_generators_list", () => structuredClone(gens));

/** Badges for the mocked recent projects (see workspaceMock). */
export const recentPrimaries: Record<string, ReturnType<typeof fw>> = {
  [demoProjectName]: fw("laravel", "Laravel", "12.4.1", "php"),
  "symfony-api": fw("symfony", "Symfony", "7.2.3", "php"),
  "legacy-crm": fw("php", "PHP", "7.4", "php"),
  "storefront-ui": fw("react", "React", "19.1.0", "javascript"),
  "pixel-dungeon": fw("love", "LÖVE", "11.5", "lua"),
  "shop-api": fw("python", "Python", "", "python"),
  "dotfiles.nvim": fw("neovim", "Neovim", "", "lua"),
};

// In browser mode the demo project opens on every page load. Announce its framework only
// when asked (`?announce`), so the balloon does not cover other screenshots.
const DEMO_ROOT = `~/Projects/${demoProjectName}`;
setSetting(
  "project.announcedFrameworks",
  new URLSearchParams(location.search).has("announce") ? null : [`${DEMO_ROOT}#laravel`],
  "user",
);

// ---- Wizard -------------------------------------------------------------------------

/** Tools the simulated machine has; `?missing=composer,npm` removes some for screenshots. */
const missingTools = new Set((new URLSearchParams(location.search).get("missing") ?? "").split(",").filter(Boolean));
const hasTool = (t: string) => !missingTools.has(t) && t !== "symfony";

const opt = (args: CreateArgs, gen: Generator, id: string): OptionValue | undefined =>
  args.options?.[id] ?? gen.options.find((o) => o.id === id)?.default;

const words = (name: string) => name.split(/[^A-Za-z0-9]+|(?<=[a-z0-9])(?=[A-Z])/).filter(Boolean);

/** Files the Rust templates write (see `crates/forge-project/src/templates.rs`). */
function templateFiles(id: string, name: string, sample: boolean): string[] {
  const pkg = words(name).join("-").toLowerCase() || "app";
  const mod = words(name).join("_").toLowerCase() || "app";
  const php = ["composer.json", "README.md", "phpunit.xml", ".gitignore"];
  const phpSample = sample ? ["src/Greeter.php", "tests/GreeterTest.php"] : [];
  const files: Record<string, string[]> = {
    "empty-php": [...php, ...phpSample],
    "composer-package": [...php, "LICENSE", ".gitattributes", ...phpSample],
    "wordpress-plugin": [`${pkg}.php`, `includes/class-${pkg}.php`, "readme.txt", "uninstall.php"],
    "lua-script": ["main.lua", ...(sample ? ["lib/greeter.lua"] : []), ".luarc.json", "README.md"],
    "love-game": ["main.lua", "conf.lua", ".luarc.json", "README.md"],
    "neovim-plugin": [`lua/${mod}/init.lua`, `plugin/${mod}.lua`, `doc/${mod}.txt`, ".luarc.json", "README.md"],
    "empty-web": ["index.html", "css/style.css", ...(sample ? ["js/main.js"] : []), "README.md"],
  };
  return files[id] ?? [];
}

/** Mirrors `forge_project::plan` closely enough for previews. */
export function mockPlan(args: CreateArgs): Plan {
  const gen = gens.find((g) => g.id === args.generator);
  if (!gen) throw new Error(`unknown generator ${args.generator}`);
  const n = args.name.trim();
  const preview: string[] = [];
  const missing: string[] = [];
  const need = (t: string) => {
    if (!hasTool(t) && !missing.includes(t)) missing.push(t);
  };
  const install = opt(args, gen, "install") === true;
  const ts = opt(args, gen, "typescript") === true;
  switch (gen.id) {
    case "laravel": {
      need("composer");
      const kit = opt(args, gen, "starterKit");
      const pkg = kit && kit !== "none" ? `laravel/${kit}-starter-kit` : "laravel/laravel";
      preview.push(`composer create-project ${pkg} ${n} --no-interaction`);
      break;
    }
    case "symfony":
      need("composer");
      preview.push(`composer create-project symfony/skeleton ${n} --no-interaction`);
      if (opt(args, gen, "variant") !== "skeleton") preview.push("composer require webapp --no-interaction");
      break;
    case "vite":
    case "react":
    case "vue": {
      need("npm");
      const template = gen.id === "vite" ? "vanilla-ts" : `${gen.id}${ts ? "-ts" : ""}`;
      preview.push(`npm create vite@latest ${n} -- --template ${template} --no-interactive`);
      if (install) preview.push("npm install");
      break;
    }
    case "angular":
      need("npx");
      preview.push(`npx -y @angular/cli@latest new ${n} --defaults --skip-git${install ? "" : " --skip-install"}`);
      break;
    case "svelte":
      need("npx");
      preview.push(
        `npx -y sv create ${n} --template minimal ${ts ? "--types ts" : "--no-types"} --no-add-ons ${install ? "--install npm" : "--no-install"}`,
      );
      break;
    default:
      preview.push(`Create ${n}/ from the built-in template`);
  }
  if (opt(args, gen, "git") === true) {
    need("git");
    preview.push("git init");
  }
  const error = validateName(n) ?? (args.location.trim() ? undefined : "Choose a location") ?? undefined;
  const sample = gen.options.some((o) => o.id === "sample") ? opt(args, gen, "sample") === true : true;
  return {
    target: joinPath(args.location, n),
    preview,
    files: templateFiles(gen.id, n, sample),
    missingTools: missing,
    fallback: missing.some((m) => m !== "git") ? gen.fallback : undefined,
    error: error ?? undefined,
  };
}

registerMock("project_plan", (args) => mockPlan(args as unknown as CreateArgs));

/** Simulated tool output per generator, so the progress view looks realistic. */
function outputFor(gen: string, name: string): string[] {
  if (gen === "laravel")
    return [
      `Creating a "laravel/laravel" project at "./${name}"`,
      "Installing laravel/laravel (v12.4.1)",
      "  - Installing laravel/laravel (v12.4.1): Extracting archive",
      `Created project in /home/forge/Projects/${name}`,
      "> @php -r \"file_exists('.env') || copy('.env.example', '.env');\"",
      "Loading composer repositories with package information",
      "Updating dependencies",
      "Lock file operations: 111 installs, 0 updates, 0 removals",
      "  - Locking laravel/framework (v12.4.1)",
      "  - Locking laravel/tinker (v2.10.1)",
      "  - Locking phpunit/phpunit (11.5.15)",
      "Writing lock file",
      "Installing dependencies from lock file (including require-dev)",
      "Package operations: 111 installs, 0 updates, 0 removals",
      "  - Downloading symfony/console (v7.2.5)",
      "  - Downloading nesbot/carbon (3.9.0)",
      "  - Installing illuminate/support (v12.4.1): Extracting archive",
      "  - Installing laravel/framework (v12.4.1): Extracting archive",
      "Generating optimized autoload files",
      "> @php artisan package:discover --ansi",
      "   INFO  Discovering packages.",
      "  laravel/tinker .................................................. DONE",
      "  nesbot/carbon ................................................... DONE",
      "> @php artisan key:generate --ansi",
      "   INFO  Application key set successfully.",
    ];
  if (["vite", "react", "vue"].includes(gen))
    return [
      `Scaffolding project in /home/forge/Projects/${name}...`,
      "Done. Now run:",
      `  cd ${name}`,
      "  npm install",
    ];
  return [];
}

registerMock("project_create", async (raw) => {
  const args = raw as unknown as CreateArgs;
  const plan = mockPlan(args);
  if (plan.error) throw new Error(plan.error);
  if (plan.missingTools.length) throw new Error(`Required tools are not installed: ${plan.missingTools.join(", ")}`);
  const emit = (e: CreateEvent) => emitMock(CREATE_EVENT, e);
  const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const lines = outputFor(args.generator, args.name);
  for (const [index, label] of plan.preview.entries()) {
    emit({ type: "step", index, total: plan.preview.length, label });
    await wait(150);
    const out =
      index === 0 ? lines : label === "git init" ? [`Initialized empty Git repository in ${plan.target}/.git/`] : [];
    for (const line of out) {
      if (cancelled) break;
      emit({ type: "output", line, stderr: false });
      await wait(90);
    }
    if (cancelled) {
      cancelled = false;
      emit({ type: "failed", error: "Cancelled" });
      throw new Error("Cancelled");
    }
  }
  emit({ type: "done", root: plan.target, name: args.name });
  return plan.target;
});

let cancelled = false;
registerMock("project_create_cancel", () => {
  cancelled = true;
});
