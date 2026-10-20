/**
 * npm tool window, like WebStorm's: every package.json of the project with its scripts.
 * Double-click runs a script, the toolbar and the context menu run or debug it or jump to
 * its line; each package shows the package manager its scripts run with.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { Bug, ChevronDown, ChevronRight, ChevronsDownUp, ChevronsUpDown, Hexagon, Play, RefreshCw, Settings, SquareArrowOutUpRight, Terminal } from "lucide-solid";
import type { NpmPackage, NpmScript, PackageManagerReason } from "@/ipc/web";
import { openSettings } from "@/features/settings/dialogState";
import { jumpToScript, runScript } from "./npm";
import { loaded, loadPackages, loading, packages } from "./store";
import { showMenu } from "./popup";

const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
/** `path` of a package or `path#script`. */
const [selected, setSelected] = createSignal<string | null>(null);

const scriptKey = (pkg: NpmPackage, s: NpmScript) => `${pkg.path}#${s.name}`;

function toggle(path: string) {
  setCollapsed((c) => {
    const n = new Set(c);
    if (n.has(path)) n.delete(path);
    else n.add(path);
    return n;
  });
}

/** The selected script (or the first script of the selected package). */
function selection(): { pkg: NpmPackage; script?: NpmScript } | null {
  const key = selected();
  if (!key) return null;
  const [path, name] = key.split("#");
  const pkg = packages().find((p) => p.path === path);
  if (!pkg) return null;
  return { pkg, script: name ? pkg.scripts.find((s) => s.name === name) : undefined };
}

function reasonText(r: PackageManagerReason): string {
  switch (r.kind) {
    case "setting":
      return "chosen in Settings | Languages & Frameworks | Node.js";
    case "field":
      return `"packageManager": "${r.value}" in ${r.path}`;
    case "lockFile":
      return `detected from ${r.path}`;
    case "default":
      return "no lock file found";
  }
}

function contextMenu(e: MouseEvent, pkg: NpmPackage, script?: NpmScript) {
  e.preventDefault();
  setSelected(script ? scriptKey(pkg, script) : pkg.path);
  const items = script
    ? [
        { label: `Run '${script.name}'`, icon: <Play size={14} />, run: () => void runScript(pkg, script.name) },
        { label: `Debug '${script.name}'`, icon: <Bug size={14} />, run: () => void runScript(pkg, script.name, true) },
        { separator: true, label: "" },
        { label: "Jump to Source", icon: <SquareArrowOutUpRight size={14} />, run: () => jumpToScript(pkg, script) },
      ]
    : [
        { label: "Jump to Source", icon: <SquareArrowOutUpRight size={14} />, run: () => jumpToScript(pkg) },
        { label: "Reload Scripts", icon: <RefreshCw size={14} />, run: () => void loadPackages(true) },
      ];
  showMenu(items, e.clientX, e.clientY);
}

function PackageNode(props: { pkg: NpmPackage }) {
  const open = () => !collapsed().has(props.pkg.path);
  const pm = () => props.pkg.packageManager;
  return (
    <>
      <div
        class="tree-row npm-row npm-package"
        classList={{ selected: selected() === props.pkg.path }}
        data-package={props.pkg.path}
        title={props.pkg.path}
        onClick={() => {
          setSelected(props.pkg.path);
          toggle(props.pkg.path);
        }}
        onDblClick={() => jumpToScript(props.pkg)}
        onContextMenu={(e) => contextMenu(e, props.pkg)}
      >
        <span class="tree-chevron">{open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</span>
        <Hexagon size={14} class="npm-icon-package" />
        <span class="tree-label">package.json</span>
        <span class="tree-hint">{[props.pkg.dir, props.pkg.name].filter(Boolean).join(" · ")}</span>
        <span class="npm-spacer" />
        <span class={`npm-pm npm-pm-${pm().manager}`} title={`${pm().manager}${pm().version ? ` ${pm().version}` : ""}: ${reasonText(pm().reason)}`}>
          {pm().manager}
        </span>
      </div>
      <Show when={open()}>
        <For each={props.pkg.scripts} fallback={<div class="tree-row npm-row npm-empty-scripts muted">No scripts</div>}>
          {(s) => (
            <div
              class="tree-row npm-row npm-script"
              classList={{ selected: selected() === scriptKey(props.pkg, s) }}
              data-script={s.name}
              title={s.command}
              onClick={() => setSelected(scriptKey(props.pkg, s))}
              onDblClick={() => void runScript(props.pkg, s.name)}
              onContextMenu={(e) => contextMenu(e, props.pkg, s)}
            >
              <Terminal size={13} class="npm-icon-script" />
              <span class="tree-label npm-script-name">{s.name}</span>
              <span class="tree-hint npm-command">{s.command}</span>
            </div>
          )}
        </For>
      </Show>
    </>
  );
}

export function NpmView() {
  const total = createMemo(() => packages().reduce((n, p) => n + p.scripts.length, 0));
  return (
    <div class="npm-panel" data-testid="npm-tool-window">
      <div class="npm-tree" role="tree">
        <Show
          when={packages().length}
          fallback={
            <div class="placeholder-panel">
              {loading() || !loaded() ? "Reading package.json files…" : "No package.json in this project."}
            </div>
          }
        >
          <For each={packages()}>{(p) => <PackageNode pkg={p} />}</For>
        </Show>
      </div>
      <Show when={packages().length}>
        <div class="npm-footer muted">
          {total()} scripts in {packages().length} package.json {packages().length === 1 ? "file" : "files"}
        </div>
      </Show>
    </div>
  );
}

export function NpmActions() {
  const sel = () => selection();
  const canRun = () => !!sel()?.script;
  return (
    <>
      <button class="icon-btn" classList={{ spinning: loading() }} title="Reload Scripts" onClick={() => void loadPackages(true)}>
        <RefreshCw size={14} />
      </button>
      <button
        class="icon-btn npm-run-btn"
        title={canRun() ? `Run '${sel()!.script!.name}'` : "Run"}
        disabled={!canRun()}
        onClick={() => sel()?.script && void runScript(sel()!.pkg, sel()!.script!.name)}
      >
        <Play size={14} />
      </button>
      <button
        class="icon-btn npm-debug-btn"
        title={canRun() ? `Debug '${sel()!.script!.name}'` : "Debug"}
        disabled={!canRun()}
        onClick={() => sel()?.script && void runScript(sel()!.pkg, sel()!.script!.name, true)}
      >
        <Bug size={14} />
      </button>
      <button class="icon-btn" title="Expand All" onClick={() => setCollapsed(new Set<string>())}>
        <ChevronsUpDown size={14} />
      </button>
      <button class="icon-btn" title="Collapse All" onClick={() => setCollapsed(new Set(packages().map((p) => p.path)))}>
        <ChevronsDownUp size={14} />
      </button>
      <button class="icon-btn" title="Settings" onClick={() => openSettings("lang.node")}>
        <Settings size={14} />
      </button>
    </>
  );
}

