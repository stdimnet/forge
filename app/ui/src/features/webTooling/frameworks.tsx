/**
 * Framework basics of the web tooling: Nuxt (pages as routes in the Endpoints tool window,
 * Go to Declaration on auto-imported component tags), Prisma (`prisma generate` /
 * `prisma migrate dev` actions; the Prisma language server comes from the catalog), and
 * Deno / Bun (tasks and scripts in Run Anything; their run configuration types are
 * declared in the manifest). The models are read in Rust (`webforge-web-tooling`).
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { File, Puzzle, RefreshCw } from "lucide-solid";
import { denoApi, nuxtApi } from "@/ipc/webTooling";
import { registerEndpointsView } from "@/features/endpoints/registry";
import { registerDeclarationProvider } from "@/features/lsp/goto";
import { registerRunAnythingProvider } from "@/features/runAnything/providers";
import { launch } from "@/features/run";
import { askName } from "@/features/contextMenu/dialogs";
import { registerCommand } from "@/state/commands";
import { openFileAt } from "@/editor/views";
import { isPluginActive, projectInfo, usesFramework } from "@/state/project";
import { npmPackages } from "./npmPackages";

// ---- Nuxt ------------------------------------------------------------------------------

const nuxtRoot = () => projectInfo()?.frameworks.find((f) => f.id === "nuxt")?.dir ?? "";
const nuxtActive = () => usesFramework("nuxt");
const [nuxt, { refetch: reloadNuxt }] = createResource(
  () => (nuxtActive() ? nuxtRoot() : null),
  (root) => nuxtApi.project(root || undefined),
);
const [nuxtTab, setNuxtTab] = createSignal<"pages" | "components">("pages");

function NuxtView() {
  return (
    <div class="wt-endpoints" data-testid="nuxt-view">
      <div class="wt-tabs">
        <button classList={{ active: nuxtTab() === "pages" }} onClick={() => setNuxtTab("pages")}>
          Pages
        </button>
        <button classList={{ active: nuxtTab() === "components" }} onClick={() => setNuxtTab("components")}>
          Components
        </button>
      </div>
      <Show
        when={nuxtTab() === "pages"}
        fallback={
          <For each={nuxt()?.components ?? []}>
            {(c) => (
              <div class="wt-row" onClick={() => void openFileAt(c.path, 1)}>
                <Puzzle size={13} class="wt-lib" />
                <span>{`<${c.name}>`}</span>
                <span class="muted wt-grow">{c.path}</span>
              </div>
            )}
          </For>
        }
      >
        <For each={nuxt()?.routes ?? []} fallback={<div class="set-list-empty">No pages.</div>}>
          {(r) => (
            <div class="wt-row" onClick={() => void openFileAt(r.path, 1)}>
              <File size={13} class="muted" />
              <span class="oa-path">{r.route}</span>
              <span class="muted">{r.name}</span>
              <span class="muted wt-grow">{r.path}</span>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}

registerEndpointsView({
  id: "nuxt",
  title: "Nuxt",
  order: 25,
  when: nuxtActive,
  component: NuxtView,
  actions: () => (
    <button class="icon-btn" title="Refresh" aria-label="Refresh" onClick={() => void reloadNuxt()}>
      <RefreshCw size={14} />
    </button>
  ),
});

// `<BaseButton>` / `<base-button>` / `<LazyBaseButton>` in Vue templates of a Nuxt app.
registerDeclarationProvider(async (path, view, pos) => {
  if (!nuxtActive() || !/\.vue$/.test(path)) return false;
  const word = view.state.wordAt(pos);
  if (!word) return false;
  const before = view.state.sliceDoc(Math.max(0, word.from - 2), word.from);
  if (!/<\/?$/.test(before)) return false;
  let tag = view.state.sliceDoc(word.from, word.to);
  // `base-button`: wordAt stops at `-`.
  const line = view.state.doc.lineAt(pos);
  const m = /^[\w-]+/.exec(line.text.slice(word.from - line.from));
  if (m) tag = m[0];
  const project = nuxt() ?? (await nuxtApi.project(nuxtRoot() || undefined).catch(() => null));
  const pascal = tag.includes("-") ? tag.split("-").map((s) => s.charAt(0).toUpperCase() + s.slice(1)).join("") : tag;
  const name = pascal.startsWith("Lazy") && pascal.length > 4 ? pascal.slice(4) : pascal;
  const target = project?.components.find((c) => c.name === name);
  if (!target) return false;
  await openFileAt(target.path, 1);
  return true;
});

// ---- Prisma ----------------------------------------------------------------------------

const prismaActive = () => isPluginActive("forge.prisma");

function runPrisma(args: string[], name: string) {
  void launch({ name, type: "shell", options: { mode: "text", scriptText: `npx prisma ${args.join(" ")}` } });
}

registerCommand({ id: "Prisma.Generate", title: "Prisma: Generate Client", category: "Tools", run: () => runPrisma(["generate"], "prisma generate") });
registerCommand({
  id: "Prisma.MigrateDev",
  title: "Prisma: Migrate Dev…",
  category: "Tools",
  run: async () => {
    const name = await askName({ title: "Prisma Migrate Dev", label: "Migration name:", initial: "", confirm: "Migrate" });
    if (name === null) return;
    runPrisma(["migrate", "dev", ...(name.trim() ? ["--name", JSON.stringify(name.trim())] : [])], "prisma migrate dev");
  },
});
registerRunAnythingProvider({
  id: "prisma",
  title: "Prisma",
  when: prismaActive,
  commands: async () => [
    { command: "npx prisma generate", description: "Generate the Prisma Client" },
    { command: "npx prisma migrate dev", description: "Create and apply a migration" },
    { command: "npx prisma migrate deploy", description: "Apply pending migrations" },
    { command: "npx prisma db push", description: "Push the schema to the database" },
    { command: "npx prisma studio", description: "Open Prisma Studio" },
  ],
});

// ---- Deno and Bun ----------------------------------------------------------------------

registerRunAnythingProvider({
  id: "deno",
  title: "Deno tasks",
  when: () => isPluginActive("forge.deno"),
  commands: async () => (await denoApi.tasks()).map((t) => ({ command: `deno task ${t.name}`, description: t.command })),
});

registerRunAnythingProvider({
  id: "bun",
  title: "Bun",
  when: () => isPluginActive("forge.bun"),
  commands: async () => [
    { command: "bun test", description: "Run the tests with Bun" },
    { command: "bun install", description: "Install dependencies" },
    ...(await npmPackages()).flatMap((p) => p.scripts.map((s) => ({ command: `bun run ${s.name}`, description: s.command }))),
  ],
});
