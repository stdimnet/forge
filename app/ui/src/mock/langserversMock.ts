/**
 * Browser-mode language servers: a catalog with mixed states (running, installed, not
 * installed, disabled), installs that stream fake npm output, a TypeScript server that
 * indexes when a script or style file opens, workspace symbols over the demo project and
 * the Intelephense licence (kept in memory, like a keychain that forgets on reload).
 */
import { emitMock, invoke, registerMock } from "@/ipc";
import type { InstallRow, LicenceStatus, RowStatus, ServerRow, WorkspaceSymbol } from "@/ipc/langservers";
import type { ServerStatus } from "@/ipc/lsp";
import type { SettingValue } from "@/ipc/settings";
import { demoConfigFiles } from "./demoConfigFiles";
import { demoFiles } from "./demoProject";
import { intelephenseMissingAtStart, markMockIntelephenseInstalled, onMockDidOpen, setMockServerStatus } from "./lspMock";
import { currentRemote } from "./remoteMock";

type Install = [tool: string, command: string, runnable?: boolean];

interface MockServer {
  id: string;
  name: string;
  group: string;
  languages: string[];
  command: string;
  args?: string[];
  status: RowStatus;
  path?: string;
  inProject?: boolean;
  install: Install[];
  note?: string;
  license?: string;
  homepage?: string;
  source?: string;
  skip?: ServerRow["skip"];
  builtinVersion?: string;
}

const HOME = "/home/marlon";
const MANAGED = `${HOME}/.local/share/forge/langservers`;
const npm = (pkgs: string): Install => ["npm", `npm install --prefix ${MANAGED} --no-fund --no-audit ${pkgs}`];
const brew = (pkg: string): Install => ["brew", `brew install ${pkg}`];
const pacman = (pkg: string): Install => ["pacman", `sudo pacman -S ${pkg}`, false];
const download = (url: string): Install => ["download", url, false];

const servers: MockServer[] = [
  { id: "typescript-language-server", name: "TypeScript Language Server", group: "JavaScript & TypeScript", languages: ["TypeScript", "JavaScript"], command: "typescript-language-server", args: ["--stdio"], status: "running", path: `${HOME}/.npm-global/bin/typescript-language-server`, inProject: true, install: [npm("typescript-language-server typescript"), brew("typescript-language-server"), pacman("typescript-language-server")], note: "Uses the project's TypeScript (node_modules) or a global one. Serves .vue files too when the Vue language server is installed (hybrid mode)." },
  { id: "vtsls", name: "vtsls", group: "JavaScript & TypeScript", languages: ["TypeScript", "JavaScript"], command: "vtsls", args: ["--stdio"], status: "notInstalled", inProject: true, install: [npm("@vtsls/language-server")], skip: { reason: "alternative", used: "typescript-language-server" }, note: "Alternative to typescript-language-server built on VS Code's TypeScript extension. Starts only when typescript-language-server is missing or disabled." },
  { id: "vscode-eslint-language-server", name: "ESLint", group: "JavaScript & TypeScript", languages: ["JavaScript", "TypeScript", "Vue", "Svelte", "Astro"], command: "vscode-eslint-language-server", args: ["--stdio"], status: "installed", path: `${HOME}/.npm-global/bin/vscode-eslint-language-server`, install: [npm("vscode-langservers-extracted"), brew("vscode-langservers-extracted")], skip: { reason: "notUsed" }, note: "Starts in projects with an ESLint configuration or an eslint dependency." },
  { id: "vue-language-server", name: "Vue Language Server (Volar)", group: "Vue", languages: ["Vue"], command: "vue-language-server", args: ["--stdio"], status: "notInstalled", install: [npm("@vue/language-server @vue/typescript-plugin typescript")], skip: { reason: "notInProject" }, note: "Hybrid mode: templates and styles here, script blocks through the TypeScript server with @vue/typescript-plugin." },
  { id: "ngserver", name: "Angular Language Service", group: "Angular", languages: ["TypeScript", "HTML"], command: "ngserver", args: ["--stdio"], status: "notInstalled", install: [npm("@angular/language-server typescript")], skip: { reason: "notUsed" }, note: "Starts in projects that depend on @angular/core." },
  { id: "svelteserver", name: "Svelte Language Server", group: "Svelte", languages: ["Svelte"], command: "svelteserver", args: ["--stdio"], status: "notInstalled", install: [npm("svelte-language-server")], skip: { reason: "notInProject" } },
  { id: "astro-ls", name: "Astro Language Server", group: "Astro", languages: ["Astro"], command: "astro-ls", args: ["--stdio"], status: "notInstalled", install: [npm("@astrojs/language-server typescript")], skip: { reason: "notInProject" } },
  { id: "vscode-html-language-server", name: "HTML Language Server", group: "HTML & CSS", languages: ["HTML"], command: "vscode-html-language-server", args: ["--stdio"], status: "installed", path: `${HOME}/.npm-global/bin/vscode-html-language-server`, inProject: true, install: [npm("vscode-langservers-extracted"), brew("vscode-langservers-extracted"), pacman("vscode-html-languageserver")] },
  { id: "vscode-css-language-server", name: "CSS Language Server", group: "HTML & CSS", languages: ["CSS", "SCSS", "Less"], command: "vscode-css-language-server", args: ["--stdio"], status: "running", path: `${HOME}/.npm-global/bin/vscode-css-language-server`, inProject: true, install: [npm("vscode-langservers-extracted"), brew("vscode-langservers-extracted"), pacman("vscode-css-languageserver")] },
  { id: "tailwindcss-language-server", name: "Tailwind CSS IntelliSense", group: "HTML & CSS", languages: ["HTML", "CSS", "SCSS", "JavaScript", "TypeScript", "Vue", "Svelte", "Astro", "PHP", "Twig"], command: "tailwindcss-language-server", args: ["--stdio"], status: "running", path: `${HOME}/.npm-global/bin/tailwindcss-language-server`, inProject: true, install: [npm("@tailwindcss/language-server")], note: "Starts in projects with a Tailwind config or a tailwindcss dependency. Class completion in Blade and Twig templates too." },
  { id: "vscode-json-language-server", name: "JSON Language Server", group: "JSON", languages: ["JSON"], command: "vscode-json-language-server", args: ["--stdio"], status: "running", path: `${HOME}/.npm-global/bin/vscode-json-language-server`, inProject: true, install: [npm("vscode-langservers-extracted"), brew("vscode-langservers-extracted")], note: "Validates composer.json, package.json, tsconfig.json and more with schemas from SchemaStore (downloaded by the server)." },
  { id: "yaml-language-server", name: "YAML Language Server", group: "YAML", languages: ["YAML", "Docker Compose"], command: "yaml-language-server", args: ["--stdio"], status: "running", path: `${HOME}/.npm-global/bin/yaml-language-server`, inProject: true, install: [npm("yaml-language-server"), brew("yaml-language-server"), pacman("yaml-language-server")], note: "Schemas for GitHub Actions, docker-compose and Kubernetes manifests (in k8s/, kubernetes/, deploy/ or manifests/)." },
  { id: "taplo", name: "Taplo (TOML)", group: "TOML", languages: ["TOML"], command: "taplo", args: ["lsp", "stdio"], status: "notInstalled", install: [["cargo", "cargo install --locked taplo-cli"], npm("@taplo/cli"), brew("taplo"), pacman("taplo-cli")], skip: { reason: "notInProject" } },
  { id: "lemminx", name: "LemMinX (XML)", group: "XML", languages: ["XML"], command: "lemminx", status: "notInstalled", inProject: true, install: [download("https://github.com/redhat-developer/vscode-xml/releases"), brew("lemminx")], license: "EPL-2.0", note: "Validates phpunit.xml, Maven POMs and any XML with an XSD or DTD. Needs Java unless you use the native binary." },
  { id: "marksman", name: "Marksman (Markdown)", group: "Markdown", languages: ["Markdown"], command: "marksman", args: ["server"], status: "installed", path: "/home/linuxbrew/.linuxbrew/bin/marksman", inProject: true, install: [brew("marksman"), pacman("marksman"), download("https://github.com/artempyanykh/marksman/releases")] },
  { id: "docker-langserver", name: "Dockerfile Language Server", group: "Docker", languages: ["Dockerfile"], command: "docker-langserver", args: ["--stdio"], status: "notInstalled", install: [npm("dockerfile-language-server-nodejs")], skip: { reason: "notInProject" } },
  { id: "docker-compose-langserver", name: "Docker Compose Language Service", group: "Docker", languages: ["Docker Compose"], command: "docker-compose-langserver", args: ["--stdio"], status: "notInstalled", inProject: true, install: [npm("@microsoft/compose-language-service")] },
  { id: "bash-language-server", name: "Bash Language Server", group: "Shell Script", languages: ["Shell Script"], command: "bash-language-server", args: ["start"], status: "installed", path: "/usr/bin/bash-language-server", install: [npm("bash-language-server"), brew("bash-language-server"), pacman("bash-language-server")], skip: { reason: "notInProject" }, note: "Uses shellcheck for diagnostics and shfmt for formatting when they are installed." },
  { id: "sqls", name: "sqls", group: "SQL", languages: ["SQL"], command: "sqls", status: "running", path: `${HOME}/go/bin/sqls`, inProject: true, install: [["go", "go install github.com/sqls-server/sqls@latest"]], note: "Completion and formatting for SQL. Configure connections in ~/.config/sqls/config.yml." },
  { id: "texlab", name: "TexLab", group: "LaTeX", languages: ["LaTeX"], command: "texlab", status: "installed", path: "/usr/bin/texlab", install: [["cargo", "cargo install --locked texlab"], brew("texlab"), pacman("texlab")], license: "GPL-3.0", skip: { reason: "notInProject" } },
  { id: "twiggy-language-server", name: "Twiggy (Twig)", group: "Twig", languages: ["Twig"], command: "twiggy-language-server", args: ["--stdio"], status: "notInstalled", inProject: true, install: [npm("twiggy-language-server")], license: "MPL-2.0" },
  { id: "lua-language-server", name: "Lua Language Server", group: "Lua", languages: ["Lua"], command: "lua-language-server", status: "disabled", path: `${HOME}/.local/share/nvim/mason/bin/lua-language-server`, install: [brew("lua-language-server"), pacman("lua-language-server"), download("https://github.com/LuaLS/lua-language-server/releases")], skip: { reason: "disabled" }, note: "Pick LÖVE or Neovim under Lua environment to get their APIs and globals." },
  { id: "basedpyright", name: "basedpyright", group: "Python", languages: ["Python"], command: "basedpyright-langserver", args: ["--stdio"], status: "installed", path: `${HOME}/.local/bin/basedpyright-langserver`, install: [["pipx", "pipx install basedpyright"], ["pip", "python3 -m pip install --user basedpyright"], npm("basedpyright")], skip: { reason: "notInProject" } },
  { id: "pyright", name: "Pyright", group: "Python", languages: ["Python"], command: "pyright-langserver", args: ["--stdio"], status: "notInstalled", install: [npm("pyright"), ["pipx", "pipx install pyright"], pacman("pyright")], skip: { reason: "notInProject" }, note: "Starts only when basedpyright is missing or disabled." },
  { id: "gopls", name: "gopls", group: "Go", languages: ["Go"], command: "gopls", status: "installed", path: `${HOME}/go/bin/gopls`, install: [["go", "go install golang.org/x/tools/gopls@latest"], pacman("gopls")], license: "BSD-3-Clause", skip: { reason: "notInProject" } },
  { id: "rust-analyzer", name: "rust-analyzer", group: "Rust", languages: ["Rust"], command: "rust-analyzer", status: "installed", path: `${HOME}/.cargo/bin/rust-analyzer`, install: [["rustup", "rustup component add rust-analyzer"], brew("rust-analyzer"), pacman("rust-analyzer")], license: "MIT OR Apache-2.0", skip: { reason: "notInProject" } },
  { id: "clangd", name: "clangd", group: "C/C++", languages: ["C", "C++"], command: "clangd", args: ["--background-index", "--clang-tidy", "--header-insertion=iwyu"], status: "installed", path: "/usr/bin/clangd", install: [pacman("clang"), ["apt", "sudo apt install clangd", false], download("https://github.com/clangd/clangd/releases")], license: "Apache-2.0 WITH LLVM-exception", skip: { reason: "notInProject" } },
  { id: "zls", name: "ZLS", group: "Zig", languages: ["Zig"], command: "zls", status: "notInstalled", install: [brew("zls"), pacman("zls"), download("https://github.com/zigtools/zls/releases")], skip: { reason: "notInProject" } },
  { id: "jdtls", name: "Eclipse JDT Language Server", group: "Java", languages: ["Java"], command: "jdtls", status: "notInstalled", install: [brew("jdtls"), download("https://download.eclipse.org/jdtls/snapshots/")], license: "EPL-2.0", skip: { reason: "notInProject" }, note: "Needs Java 21+. Forge keeps its workspace data in the user cache directory." },
  { id: "kotlin-lsp", name: "Kotlin LSP", group: "Kotlin", languages: ["Kotlin"], command: "intellij-server", status: "notInstalled", install: [brew("JetBrains/utils/kotlin-lsp"), download("https://github.com/Kotlin/kotlin-lsp/blob/main/RELEASES.md")], license: "Apache-2.0 (the server build includes closed JetBrains components)", skip: { reason: "notInProject" }, note: "KotlinForge downloads the standalone build from download.jetbrains.com on request (pinned version, SHA-256 checked) and starts it with the project JDK." },
  { id: "kotlin-language-server", name: "Kotlin Language Server", group: "Kotlin", languages: ["Kotlin"], command: "kotlin-language-server", status: "notInstalled", install: [brew("kotlin-language-server"), download("https://github.com/fwcd/kotlin-language-server/releases")], license: "MIT", skip: { reason: "notInProject" }, note: "Used when chosen in the Kotlin settings instead of kotlin-lsp." },
  { id: "csharp-ls", name: "csharp-ls", group: "C#", languages: ["C#"], command: "csharp-ls", status: "notInstalled", install: [["dotnet", "dotnet tool install --global csharp-ls"]], skip: { reason: "notInProject" }, note: "Needs the .NET SDK." },
  { id: "ruby-lsp", name: "Ruby LSP", group: "Ruby", languages: ["Ruby"], command: "ruby-lsp", status: "notInstalled", install: [["gem", "gem install --user-install ruby-lsp"]], skip: { reason: "notInProject" } },
  { id: "php-lsp", name: "php-lsp (built-in)", group: "PHP", languages: ["PHP"], command: "forge", args: ["--php-lsp", "--stdio"], status: "running", path: "/usr/bin/forge", inProject: true, install: [], source: "php", homepage: "https://github.com/stdimnet/php-lsp", license: "MIT OR Apache-2.0", builtinVersion: "0.1.0 (6e00c57)", note: "Built into Forge, nothing to install: completion, navigation, diagnostics, rename, refactorings and formatting in Intelephense's style, premium features included. The default PHP language server." },
  { id: "intelephense", name: "Intelephense", group: "PHP", languages: ["PHP"], command: "intelephense", args: ["--stdio"], status: "installed", path: `${HOME}/.npm-global/bin/intelephense`, inProject: true, install: [npm("intelephense")], source: "php", homepage: "https://intelephense.com", license: "Proprietary (free core, premium licence key optional)", note: "Used when chosen in the PHP settings. Free by default; enter your own licence key to unlock premium features such as rename, code actions and type hierarchy." },
  { id: "phpactor", name: "Phpactor", group: "PHP", languages: ["PHP"], command: "phpactor", args: ["language-server"], status: "notInstalled", inProject: true, install: [["composer", "composer global require phpactor/phpactor"], download("https://github.com/phpactor/phpactor/releases")], source: "php", homepage: "https://phpactor.readthedocs.io", note: "Used when chosen in the PHP settings." },
];

// `?intelephense=missing`: Intelephense (and Node.js) are not installed yet.
const intelephense = servers.find((s) => s.id === "intelephense")!;
let nodeInstalled = true;
if (intelephenseMissingAtStart()) {
  intelephense.status = "notInstalled";
  intelephense.path = undefined;
  nodeInstalled = false;
  // The project chose Intelephense: the built-in server is not running.
  servers.find((s) => s.id === "php-lsp")!.status = "installed";
}

/** Tools present on the "machine" in browser mode. */
const TOOLS = new Set(["npm", "cargo", "go", "pipx", "rustup", "composer"]);

const installRows = (s: MockServer): InstallRow[] =>
  s.install.map(([tool, command, runnable], index) => {
    const isUrl = command.startsWith("https://");
    // npm packages are installed by Forge itself (no npm needed), like the desktop app.
    const managed = tool === "npm";
    return {
      index,
      tool,
      command,
      runnable: runnable ?? !isUrl,
      available: managed || TOOLS.has(tool),
      url: isUrl ? command : null,
      managed,
    };
  });

const DEFAULTS: Record<string, unknown> = {
  "typescript-language-server": {
    initializationOptions: { hostInfo: "forge", preferences: { importModuleSpecifierPreference: "shortest", quotePreference: "auto" } },
    settings: { typescript: { format: { enable: true } }, javascript: { format: { enable: true } } },
  },
  "vscode-json-language-server": {
    initializationOptions: { provideFormatter: true },
    settings: { json: { validate: { enable: true }, schemas: [{ fileMatch: ["composer.json"], url: "https://getcomposer.org/schema.json" }] } },
  },
};

/** Live detail of the TypeScript server ("Indexing 45%"). */
let tsDetail: string | null = null;

async function settingsValues(): Promise<Record<string, SettingValue>> {
  const snap = await invoke<{ values: Record<string, SettingValue> }>("settings_get_all");
  return snap.values;
}

function toRow(s: MockServer, values: Record<string, SettingValue>): ServerRow {
  const key = (f: string) => `langservers.servers.${s.id}.${f}`;
  const enabledPref = values[key("enabled")];
  const enabled = typeof enabledPref === "boolean" ? enabledPref : s.status !== "disabled";
  const customPath = (values[key("path")] as string | undefined) || null;
  const extra = (values[key("settings")] as string | undefined) || null;
  let extraError: string | null = null;
  if (extra) {
    try {
      const v = JSON.parse(extra);
      if (typeof v !== "object" || Array.isArray(v) || v === null) extraError = "extra settings must be a JSON object";
    } catch (e) {
      extraError = `invalid JSON: ${(e as Error).message}`;
    }
  }
  const path = customPath ?? s.path ?? null;
  let status: RowStatus = s.status;
  if (!enabled) status = "disabled";
  else if (s.status === "disabled") status = path ? "installed" : "notInstalled";
  return {
    id: s.id,
    name: s.name,
    group: s.group,
    languages: s.languages,
    command: s.command,
    args: s.args ?? [],
    homepage: s.homepage ?? `https://github.com/search?q=${encodeURIComponent(s.command)}`,
    license: s.license ?? "MIT",
    note: s.note ?? null,
    enabled,
    defaultEnabled: true,
    status,
    detail: s.id === "typescript-language-server" && status === "running" ? tsDetail : null,
    path,
    customPath,
    inProject: s.inProject ?? false,
    skip: !enabled ? { reason: "disabled" } : s.skip?.reason === "disabled" ? null : (s.skip ?? null),
    install: installRows(s),
    defaults: (DEFAULTS[s.id] as ServerRow["defaults"]) ?? { initializationOptions: null, settings: null },
    extraSettings: extra,
    extraSettingsError: extraError,
    source: s.source ?? "catalog",
    builtinVersion: s.builtinVersion ?? null,
  };
}

export async function mockRows(): Promise<ServerRow[]> {
  const values = await settingsValues();
  return servers.map((s) => toRow(s, values));
}

registerMock("langservers_list", () => mockRows());
registerMock("langservers_apply", async () => {
  emitMock("langservers://changed", null);
  return mockRows();
});

// ---------- installs ----------

const running = new Map<string, () => void>();
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function fakeOutput(s: MockServer, command: string): [stream: "stdout" | "stderr", line: string, delay: number][] {
  if (command.startsWith("npm")) {
    const pkg = command.split(" ").slice(6).join(" ");
    return [
      ["stderr", "npm warn deprecated inflight@1.0.6: This module is not supported, and leaks memory.", 350],
      ["stderr", "npm warn deprecated glob@7.2.3: Glob versions prior to v9 are no longer supported", 250],
      ["stdout", "", 600],
      ["stdout", `added ${12 + pkg.length} packages in 3s`, 500],
      ["stdout", "", 80],
      ["stdout", `${4 + (pkg.length % 5)} packages are looking for funding`, 80],
      ["stdout", "  run `npm fund` for details", 80],
    ];
  }
  return [
    ["stdout", `Installing ${s.command}…`, 400],
    ["stdout", `Installed ${s.command}`, 900],
  ];
}

/** Screenshot scenarios hold a simulated download at this fraction (browser mode only). */
const hold = () => (window as unknown as { __forgeMockInstallHold?: number }).__forgeMockInstallHold;

const MB = 1048576;
const NODE = "v22.11.0";
const PACKAGE_SIZES: Record<string, number> = { intelephense: 9.6 * MB };

/**
 * A Forge-managed npm install as the backend reports it: Node.js download (when "missing"),
 * registry lookup, tarball download with progress, integrity check, unpack.
 */
async function managedInstall(id: string, packages: string[], cancelled: () => boolean): Promise<boolean> {
  const emit = (line: string) => emitMock("langservers://install-output", { id, stream: "info", line });
  // A remote project's servers are installed by the agent on its host.
  const remote = currentRemote();
  const home = remote ? `/home/${remote.user}` : HOME;
  const managedDir = `${home}/.local/share/forge/langservers`;
  if (remote) emit(`Installing on ${remote.label}…`);
  const download = async (label: string, total: number) => {
    for (let done = 0; ; ) {
      emitMock("langservers://install-progress", { id, label, done, total });
      if (done >= total) return;
      await sleep(180);
      while (hold() !== undefined && done / total >= hold()!) await sleep(100);
      if (cancelled()) return;
      done = Math.min(total, done + total / 12);
    }
  };
  if (nodeInstalled) {
    emit("Using Node.js at /usr/bin/node.");
  } else {
    emit("No Node.js 18 or newer found; Forge downloads its own copy.");
    emit("Looking up the current Node.js LTS release…");
    await sleep(250);
    emit(`Downloading Node.js ${NODE} (linux-x64)…`);
    await download(`Downloading Node.js ${NODE}`, 48.2 * MB);
    if (cancelled()) return false;
    emit(`Verified SHA-256 of node-${NODE}-linux-x64.tar.gz.`);
    emit("Unpacking Node.js…");
    await sleep(300);
    emit(`Node.js ${NODE} is ready in ${home}/.local/share/forge/runtimes/node-${NODE}.`);
    nodeInstalled = true;
  }
  for (const pkg of packages) {
    emit(`Fetching ${pkg} from https://registry.npmjs.org…`);
    await sleep(200);
    const version = pkg === "intelephense" ? "1.14.4" : "1.0.0";
    emit(`Downloading ${pkg}@${version} from https://registry.npmjs.org/${pkg}/-/${pkg.split("/").pop()}-${version}.tgz…`);
    await download(`Downloading ${pkg} ${version}`, PACKAGE_SIZES[pkg] ?? 2.4 * MB);
    if (cancelled()) return false;
    emit(`Verified sha512 integrity of ${pkg}@${version}.`);
    emit(`Installed ${pkg}@${version} into ${managedDir}/node_modules/${pkg}.`);
  }
  return true;
}

registerMock("langservers_install", async ({ id, index }) => {
  const s = servers.find((x) => x.id === id);
  const option = s?.install[Number(index)];
  if (!s || !option) throw new Error(`unknown language server ${String(id)}`);
  const [tool, command] = option;
  const managed = tool === "npm";
  if (!managed && !TOOLS.has(tool)) throw new Error(`${tool} was not found. Install it first or run the command in a terminal.`);
  let cancelled = false;
  running.set(s.id, () => (cancelled = true));
  const emit = (stream: string, line: string) => emitMock("langservers://install-output", { id: s.id, stream, line });
  if (managed) {
    await managedInstall(s.id, command.split(" ").slice(6), () => cancelled);
  } else {
    emit("info", `$ ${command}`);
    for (const [stream, line, delay] of fakeOutput(s, command)) {
      await sleep(delay);
      if (cancelled) break;
      emit(stream, line);
    }
  }
  running.delete(s.id);
  if (cancelled) {
    emit("info", "Cancelled.");
    return "cancelled";
  }
  s.status = "installed";
  const home = currentRemote() ? `/home/${currentRemote()!.user}` : HOME;
  s.path = managed ? `${home}/.local/share/forge/langservers/node_modules/.bin/${s.command}` : `${home}/.local/bin/${s.command}`;
  if (s.id === "intelephense") {
    // The PHP pack registers it right away and it starts for the open PHP files.
    s.status = "running";
    markMockIntelephenseInstalled();
  }
  emit("info", `${s.name} installed.`);
  emitMock("langservers://changed", null);
  return "succeeded";
});

// `?updates=1`: a newer Intelephense is available.
const offerUpdates = () => new URLSearchParams(location.search).get("updates") === "1";
let updated = false;

registerMock("langservers_check_updates", () =>
  offerUpdates() && !updated ? [{ name: "intelephense", installed: "1.12.6", latest: "1.14.4" }] : null,
);

registerMock("langservers_update", async ({ package: pkg }) => {
  const name = String(pkg);
  let cancelled = false;
  running.set(name, () => (cancelled = true));
  const ok = await managedInstall(name, [name], () => cancelled);
  running.delete(name);
  if (!ok) return "cancelled";
  updated = true;
  emitMock("langservers://changed", null);
  return "succeeded";
});

registerMock("langservers_cancel_install", ({ id }) => {
  running.get(String(id))?.();
});

// ---------- TypeScript server with indexing progress ----------

const TS_STATUS = { id: "typescript-language-server", name: "TypeScript Language Server" };
let tsStarted = false;
let frozen: number | null = null;

type StateOnly<T> = T extends unknown ? Omit<T, "id" | "name"> : never;

function setTs(state: StateOnly<ServerStatus>) {
  setMockServerStatus({ ...TS_STATUS, ...state } as ServerStatus);
}

/** Show the TypeScript server indexing at `percent` (screenshots) or let it finish (`null`). */
export function setMockIndexing(percent: number | null): void {
  frozen = percent;
  if (percent === null) {
    tsDetail = null;
    setTs({ state: "ready" });
  } else {
    tsDetail = `Indexing ${percent}%`;
    setTs({ state: "busy", message: "Indexing", percentage: percent });
  }
}

function startTsServer() {
  if (tsStarted) return;
  tsStarted = true;
  setTs({ state: "starting" });
  const steps = [0, 18, 45, 72, 100];
  steps.forEach((p, i) =>
    setTimeout(() => {
      if (frozen !== null) return;
      tsDetail = `Indexing ${p}%`;
      setTs({ state: "busy", message: "Indexing", percentage: p });
    }, 200 + i * 450),
  );
  setTimeout(() => {
    if (frozen !== null) return;
    tsDetail = null;
    setTs({ state: "ready" });
  }, 200 + steps.length * 450);
}

// Screenshot scenarios freeze the progress through this hook (browser mode only).
(window as unknown as { __forgeMockIndexing?: typeof setMockIndexing }).__forgeMockIndexing = setMockIndexing;

onMockDidOpen((path) => {
  if (/\.(ts|tsx|js|jsx|mjs|vue|css|scss)$/.test(path)) startTsServer();
});

// ---------- workspace symbols ----------

const files: Record<string, string> = { ...demoFiles, ...demoConfigFiles };

/** Per-language regexes standing in for the real servers' indexes. */
const RULES: { ext: RegExp; server: string; rules: [RegExp, string][] }[] = [
  {
    ext: /\.(ts|js)$/,
    server: "typescript-language-server",
    rules: [
      [/^\s*(?:export\s+)?interface\s+(\w+)/, "interface"],
      [/^\s*(?:export\s+)?(?:abstract\s+)?class\s+(\w+)/, "class"],
      [/^\s*(?:export\s+)?(?:async\s+)?function\s+(\w+)/, "function"],
      [/^\s*(?:export\s+)?type\s+(\w+)\s*=/, "class"],
      [/^\s{2}(\w+)\??:\s/, "property"],
    ],
  },
  { ext: /\.(css|scss)$/, server: "vscode-css-language-server", rules: [[/^\s*\.([\w-]+)\s*[{,]/, "class"], [/^\s*(--[\w-]+):/, "variable"]] },
  {
    ext: /\.sql$/,
    server: "sqls",
    rules: [
      [/create\s+(?:or\s+replace\s+)?(?:view|table)\s+(?:if\s+not\s+exists\s+)?(\w+)/i, "class"],
      [/^\s{4}(\w+)\s+(?:BIGINT|INT|VARCHAR|DECIMAL|TIMESTAMP|TEXT|BOOLEAN)/i, "field"],
    ],
  },
  { ext: /\.ya?ml$/, server: "yaml-language-server", rules: [[/^(\w[\w-]*):/, "module"], [/^ {2}(\w[\w-]*):/, "property"]] },
  { ext: /\.json$/, server: "vscode-json-language-server", rules: [[/^ {2}"([\w@/.-]+)":/, "property"]] },
];

export function demoWorkspaceSymbols(): WorkspaceSymbol[] {
  const out: WorkspaceSymbol[] = [];
  for (const [path, src] of Object.entries(files)) {
    const lang = RULES.find((r) => r.ext.test(path));
    if (!lang) continue;
    let container: string | null = null;
    src.split("\n").forEach((text, line) => {
      for (const [re, kind] of lang.rules) {
        const m = re.exec(text);
        if (!m) continue;
        const name = m[1];
        const character = text.indexOf(name);
        if (kind === "interface" || kind === "class") container = name;
        out.push({
          name,
          kind,
          containerName: kind === "property" || kind === "field" ? container : null,
          path,
          range: { start: { line, character }, end: { line, character: character + name.length } },
          server: lang.server,
        });
        break;
      }
    });
  }
  return out;
}

const allSymbols = demoWorkspaceSymbols();

registerMock("lsp_workspace_symbols", ({ query, limit }) => {
  const q = String(query ?? "").toLowerCase();
  if (!q) return [];
  // Servers match fuzzily themselves; a subsequence match is close enough here.
  const matches = (name: string) => {
    let i = 0;
    for (const ch of name.toLowerCase()) if (ch === q[i]) i++;
    return i === q.length;
  };
  return allSymbols.filter((s) => matches(s.name)).slice(0, Number(limit ?? 200));
});

// ---------- Intelephense licence ----------

let licenceKey: string | null = null;

const status = (): LicenceStatus => ({
  premium: licenceKey !== null,
  maskedKey: licenceKey ? `…${licenceKey.slice(-4)}` : null,
  error: null,
  purchaseUrl: "https://intelephense.com",
});

registerMock("intelephense_licence_status", () => status());
// Screenshot scenarios switch Intelephense Premium on (rename and code actions by the server).
(window as unknown as { __forgeMockLicence?: (key: string | null) => void }).__forgeMockLicence = (key) => (licenceKey = key);
registerMock("intelephense_set_licence", ({ key }) => {
  const k = String(key ?? "").trim();
  if (k && !/^[A-Za-z0-9_-]{8,64}$/.test(k)) throw new Error("This does not look like an Intelephense licence key.");
  licenceKey = k || null;
  return status();
});
