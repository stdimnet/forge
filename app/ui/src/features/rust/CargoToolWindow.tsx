/**
 * The Cargo tool window, like RustRover's: the workspace members with their targets (lib,
 * bin, test, bench, example, build script), features with checkboxes (Settings | Rust |
 * Cargo, used by every cargo command and rust-analyzer) and the dependency tree, from
 * `cargo metadata` (parsed and cached in Rust, `cargo_workspace`).
 *
 * Toolbar: Reload, Run and Debug the selected target, Build, Check, Clean, Update.
 * Double-click runs a binary or example, or the tests of a test target.
 */
import { createMemo, createSignal, For, onMount, Show, type JSX } from "solid-js";
import {
  BookOpen,
  Box,
  Bug,
  ChevronDown,
  ChevronRight,
  CircleCheck,
  FlaskConical,
  Gauge,
  Hammer,
  Library,
  Network,
  Package,
  Play,
  RefreshCw,
  Sparkles,
  Terminal,
  Trash2,
  Wrench,
} from "lucide-solid";
import type { CargoAction, CargoPackage, CargoTarget, DependencyNode } from "@/ipc/rust";
import { rustApi } from "@/ipc/rust";
import type { RunConfiguration } from "@/ipc/run";
import { useTemporary } from "@/features/run/configStore";
import { debugConfiguration, launch } from "@/features/run/runStore";
import { getSetting, saveSettings } from "@/state/settings";
import { cargoLoading, cargoWorkspace, cargoWorkspaceError, refreshCargo, runRustCommand, showRustBalloon } from "./state";
import { t as tr } from "@/i18n";
import "./cargo.css";

const KIND_LABEL: Record<CargoTarget["kind"], string> = {
  lib: "lib",
  procMacro: "proc-macro",
  bin: "bin",
  test: "test",
  bench: "bench",
  example: "example",
  buildScript: "build script",
};

function targetIcon(t: CargoTarget): (p: { size?: number; class?: string }) => JSX.Element {
  switch (t.kind) {
    case "bin":
      return Terminal;
    case "test":
      return FlaskConical;
    case "bench":
      return Gauge;
    case "example":
      return Sparkles;
    case "buildScript":
      return Wrench;
    default:
      return Library;
  }
}

/** `lib`, `bin:shop`, …: how configurations name a target. */
const targetKey = (t: CargoTarget) => (t.kind === "lib" || t.kind === "procMacro" ? "lib" : `${t.kind}:${t.name}`);

/** The configuration that runs a target: `cargo run` for binaries and examples, Cargo Test for tests. */
function configurationFor(pkg: CargoPackage, t: CargoTarget): RunConfiguration | null {
  if (t.kind === "bin" || t.kind === "example")
    return { name: t.name, type: "cargo", options: { command: "run", package: pkg.name, target: targetKey(t) } };
  if (t.kind === "test" || t.kind === "lib" || t.kind === "bench")
    return {
      name: t.kind === "lib" ? `Tests in ${pkg.name}` : t.name,
      type: "cargo-test",
      options: { scope: "target", package: pkg.name, target: targetKey(t), ...(t.kind === "bench" ? { kind: "bench" } : {}) },
    };
  return null;
}

async function cargoAction(action: CargoAction, pkg?: string, target?: string): Promise<void> {
  try {
    await runRustCommand(await rustApi.cargoCommand(action, pkg, target));
  } catch (e) {
    showRustBalloon(`cargo ${action}`, String(e).replace(/^Error: /, ""), 6000);
  }
}

// ---- Features ------------------------------------------------------------------------------

const featureList = () => getSetting<string[]>("rust.cargo.features", []) ?? [];
const qualified = (pkg: CargoPackage, name: string) => ((cargoWorkspace()?.members.length ?? 0) > 1 ? `${pkg.name}/${name}` : name);

function featureOn(pkg: CargoPackage, name: string): boolean {
  if (getSetting<boolean>("rust.cargo.allFeatures", false)) return true;
  const list = featureList();
  if (list.includes(qualified(pkg, name)) || list.includes(name)) return true;
  const f = pkg.features.find((x) => x.name === name);
  return !!f?.default && !getSetting<boolean>("rust.cargo.noDefaultFeatures", false);
}

async function toggleFeature(pkg: CargoPackage, name: string): Promise<void> {
  const q = qualified(pkg, name);
  let list = featureList().filter((f) => f !== q && f !== name);
  const changes: Record<string, string[] | boolean> = {};
  if (getSetting<boolean>("rust.cargo.allFeatures", false)) {
    // All features but this one.
    changes["rust.cargo.allFeatures"] = false;
    changes["rust.cargo.noDefaultFeatures"] = true;
    list = (cargoWorkspace()?.members ?? []).flatMap((m) => m.features.map((f) => qualified(m, f.name))).filter((f) => f !== q);
  } else if (featureOn(pkg, name)) {
    const f = pkg.features.find((x) => x.name === name);
    if (f?.default && !getSetting<boolean>("rust.cargo.noDefaultFeatures", false)) {
      // Turning off a default feature: no defaults, the other defaults by name.
      changes["rust.cargo.noDefaultFeatures"] = true;
      for (const m of cargoWorkspace()?.members ?? [])
        for (const d of m.features) if (d.default && !(m.name === pkg.name && d.name === name)) list.push(qualified(m, d.name));
    }
  } else {
    list.push(q);
  }
  changes["rust.cargo.features"] = [...new Set(list)];
  await saveSettings(changes);
}

// ---- Tree ----------------------------------------------------------------------------------

interface Row {
  key: string;
  depth: number;
  label: string;
  detail?: string;
  icon?: (p: { size?: number; class?: string }) => JSX.Element;
  iconClass?: string;
  children: boolean;
  /** A feature row: its package and name. */
  feature?: { pkg: CargoPackage; name: string; enables: string[] };
  target?: { pkg: CargoPackage; target: CargoTarget };
  pkg?: CargoPackage;
}

function dependencyRows(node: DependencyNode, key: string, depth: number, open: (k: string) => boolean, out: Row[]): void {
  const k = `${key}/${node.name}@${node.version}`;
  const kids = node.children ?? [];
  out.push({
    key: k,
    depth,
    label: node.name,
    detail: [node.version && (node.source === "registry" ? node.version : `${node.version} (${node.source || "workspace"})`), node.kind !== "normal" ? node.kind : "", node.repeated ? "(*)" : ""]
      .filter(Boolean)
      .join("  "),
    icon: Box,
    iconClass: node.source === "registry" ? "cargo-dep" : "cargo-member",
    children: kids.length > 0,
  });
  if (kids.length && open(k)) for (const c of kids) dependencyRows(c, k, depth + 1, open, out);
}

export function CargoToolWindow() {
  const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set());
  const [selected, setSelected] = createSignal<string | null>(null);
  onMount(() => {
    if (!cargoWorkspace()) void refreshCargo();
  });
  // Packages, targets and features start open; dependencies closed (they can be deep).
  const isOpen = (k: string) => (k.includes("/deps/") || k.endsWith("/deps") ? expanded().has(k) : !collapsed().has(k));
  const toggle = (k: string) => {
    if (k.includes("/deps")) {
      setExpanded((s) => {
        const n = new Set(s);
        if (n.has(k)) n.delete(k);
        else n.add(k);
        return n;
      });
    } else {
      setCollapsed((s) => {
        const n = new Set(s);
        if (n.has(k)) n.delete(k);
        else n.add(k);
        return n;
      });
    }
  };
  const rows = createMemo<Row[]>(() => {
    const ws = cargoWorkspace();
    if (!ws) return [];
    const out: Row[] = [];
    ws.members.forEach((pkg, i) => {
      const pk = `pkg:${pkg.name}`;
      out.push({ key: pk, depth: 0, label: pkg.name, detail: pkg.version, icon: Package, iconClass: "cargo-member", children: true, pkg });
      if (!isOpen(pk)) return;
      const tk = `${pk}/targets`;
      out.push({ key: tk, depth: 1, label: tr("langs.rust.targets"), icon: Hammer, iconClass: "muted", children: pkg.targets.length > 0 });
      if (isOpen(tk))
        for (const t of pkg.targets)
          out.push({
            key: `${tk}/${targetKey(t)}`,
            depth: 2,
            label: t.name,
            detail: KIND_LABEL[t.kind],
            icon: targetIcon(t),
            iconClass: `cargo-target-${t.kind}`,
            children: false,
            target: { pkg, target: t },
          });
      if (pkg.features.length) {
        const fk = `${pk}/features`;
        out.push({ key: fk, depth: 1, label: tr("langs.rust.features"), icon: CircleCheck, iconClass: "muted", children: true });
        if (isOpen(fk))
          for (const f of pkg.features)
            out.push({
              key: `${fk}/${f.name}`,
              depth: 2,
              label: f.name,
              detail: f.enables.join(", "),
              children: false,
              feature: { pkg, name: f.name, enables: f.enables },
            });
      }
      const tree = ws.trees[i];
      if (tree?.children?.length) {
        const dk = `${pk}/deps`;
        out.push({ key: dk, depth: 1, label: tr("common.dependencies"), icon: Network, iconClass: "muted", children: true });
        if (isOpen(dk)) for (const c of tree.children) dependencyRows(c, dk, 2, isOpen, out);
      }
    });
    return out;
  });
  const selectedRow = () => rows().find((r) => r.key === selected());
  const selectedPackage = () => {
    const r = selectedRow();
    if (!r) return undefined;
    if (r.pkg) return r.pkg;
    if (r.target) return r.target.pkg;
    if (r.feature) return r.feature.pkg;
    const name = r.key.match(/^pkg:([^/]+)/)?.[1];
    return cargoWorkspace()?.members.find((m) => m.name === name);
  };
  const selectedConfig = () => {
    const t = selectedRow()?.target;
    return t ? configurationFor(t.pkg, t.target) : null;
  };
  const run = (cfg: RunConfiguration | null) => cfg && void launch(useTemporary(cfg));
  const debug = (cfg: RunConfiguration | null) => cfg && void debugConfiguration(useTemporary(cfg));
  const action = (a: CargoAction) => void cargoAction(a, selectedPackage()?.name, selectedRow()?.target ? targetKey(selectedRow()!.target!.target) : undefined);
  return (
    <div class="cargo-tw">
      <div class="cargo-toolbar">
        <button class="icon-btn" title={tr("langs.rust.reloadCargoProject")} data-action="cargo-reload" onClick={() => void refreshCargo(true)}>
          <RefreshCw size={14} classList={{ "cargo-spin": cargoLoading() }} />
        </button>
        <span class="cargo-sep" />
        <button class="icon-btn" title={tr("common.run2")} disabled={!selectedConfig()} onClick={() => run(selectedConfig())}>
          <Play size={14} class="cargo-run" />
        </button>
        <button class="icon-btn" title={tr("common.debug2")} disabled={!selectedConfig()} onClick={() => debug(selectedConfig())}>
          <Bug size={14} class="cargo-run" />
        </button>
        <span class="cargo-sep" />
        <button class="icon-btn" title={tr("langs.rust.buildCargoBuild")} data-action="cargo-build" onClick={() => action("build")}>
          <Hammer size={14} />
        </button>
        <button class="icon-btn" title={tr("langs.rust.checkCargoCheck")} onClick={() => action("check")}>
          <CircleCheck size={14} />
        </button>
        <button class="icon-btn" title={tr("langs.rust.documentationCargoDoc")} onClick={() => action("doc")}>
          <BookOpen size={14} />
        </button>
        <button class="icon-btn" title={tr("langs.rust.cleanCargoClean")} onClick={() => action("clean")}>
          <Trash2 size={14} />
        </button>
        <button class="icon-btn" title={tr("langs.rust.updateDependenciesCargoUpdate")} onClick={() => void cargoAction("update")}>
          <Network size={14} />
        </button>
      </div>
      <Show when={cargoWorkspaceError() && !cargoWorkspace()}>
        <div class="cargo-message">
          <div>{tr("langs.rust.cannotLoadTheCargoProject")}</div>
          <div class="muted">{cargoWorkspaceError()}</div>
        </div>
      </Show>
      <Show when={!cargoWorkspace() && !cargoWorkspaceError()}>
        <div class="cargo-message muted">{cargoLoading() ? tr("langs.rust.loadingCargoMetadataEllipsis") : tr("langs.rust.noCargoProject")}</div>
      </Show>
      <div class="cargo-tree" role="tree">
        <For each={rows()}>
          {(r) => (
            <div
              class="tree-row cargo-row"
              classList={{ selected: selected() === r.key }}
              style={{ "padding-left": `${6 + r.depth * 16}px` }}
              data-cargo-row={r.key}
              title={r.feature ? (r.feature.enables.length ? `Enables ${r.feature.enables.join(", ")}` : r.feature.name) : r.target ? r.target.target.srcPath : undefined}
              onClick={() => setSelected(r.key)}
              onDblClick={() => {
                if (r.children) toggle(r.key);
                else if (r.target) run(configurationFor(r.target.pkg, r.target.target));
                else if (r.feature) void toggleFeature(r.feature.pkg, r.feature.name);
              }}
            >
              <span class="cargo-twisty" onClick={(e) => (e.stopPropagation(), r.children && toggle(r.key))}>
                <Show when={r.children}>{isOpen(r.key) ? <ChevronDown size={13} /> : <ChevronRight size={13} />}</Show>
              </span>
              <Show
                when={r.feature}
                fallback={
                  <Show when={r.icon}>
                    {(icon) => {
                      const I = icon();
                      return <I size={14} class={`cargo-icon ${r.iconClass ?? ""}`} />;
                    }}
                  </Show>
                }
              >
                {(f) => (
                  <input
                    type="checkbox"
                    class="cargo-feature"
                    data-feature={f().name}
                    checked={featureOn(f().pkg, f().name)}
                    onClick={(e) => e.stopPropagation()}
                    onChange={() => void toggleFeature(f().pkg, f().name)}
                  />
                )}
              </Show>
              <span class="cargo-label">{r.label}</span>
              <Show when={r.detail}>
                <span class="muted cargo-detail">{r.detail}</span>
              </Show>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}
