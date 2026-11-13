/**
 * The "Vulnerable Dependencies" tab of the Problems tool window: the vulnerable packages of
 * each audited manifest, grouped by package, with severity, installed and fixed versions,
 * advisory links and the Upgrade fix; tools that could not run say why.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { Archive, ArrowUpCircle, ChevronDown, ChevronRight, CircleAlert, ExternalLink, Info, RefreshCw, ShieldAlert, ShieldCheck } from "lucide-solid";
import { TOOL_LABELS, type Severity, type TargetResult, type Vulnerability } from "@/ipc/vulnerabilities";
import { revealLocation } from "@/features/lsp/navigation";
import { openSettings } from "@/features/settings/dialogState";
import {
  applyUpgrade,
  declarationOf,
  installTitle,
  installTool,
  runCheck,
  showBalloon,
  vulnerabilitiesEnabled,
  vulnerabilitiesError,
  vulnerabilitiesRunning,
  vulnerabilityResults,
} from "./store";
import { t } from "@/i18n";

const RANK: Record<Severity, number> = { critical: 5, high: 4, moderate: 3, unknown: 2, low: 1 };
const LABEL: Record<Severity, string> = { critical: "Critical", high: "High", moderate: "Moderate", unknown: "Unrated", low: "Low" };

interface PackageGroup {
  key: string;
  result: TargetResult;
  name: string;
  installed?: string;
  via: string[];
  worst: Severity;
  fixed?: string;
  vulnerabilities: Vulnerability[];
}

function compareVersions(a: string, b: string): number {
  const parts = (v: string) => v.replace(/^v/, "").split(/[.-]/).map((p) => parseInt(p, 10) || 0);
  const x = parts(a);
  const y = parts(b);
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d) return d;
  }
  return 0;
}

function groups(r: TargetResult): PackageGroup[] {
  const map = new Map<string, PackageGroup>();
  for (const v of r.vulnerabilities) {
    const key = `${r.tool}:${r.dir}:${v.package}:${v.installed ?? ""}`;
    let g = map.get(key);
    if (!g) {
      g = { key, result: r, name: v.package, installed: v.installed, via: [], worst: v.severity, vulnerabilities: [] };
      map.set(key, g);
    }
    g.vulnerabilities.push(v);
    if (RANK[v.severity] > RANK[g.worst]) g.worst = v.severity;
    for (const d of v.via ?? []) if (!g.via.includes(d)) g.via.push(d);
  }
  for (const g of map.values()) {
    const fixes = g.vulnerabilities.map((v) => v.fixed);
    if (fixes.every(Boolean)) g.fixed = (fixes as string[]).sort(compareVersions).pop();
  }
  return [...map.values()].sort((a, b) => RANK[b.worst] - RANK[a.worst] || a.name.localeCompare(b.name));
}

function ago(seconds?: number): string {
  if (!seconds) return "";
  const d = Math.max(0, Date.now() / 1000 - seconds);
  if (d < 60) return "just now";
  if (d < 3600) return `${Math.round(d / 60)} min ago`;
  if (d < 86400) return `${Math.round(d / 3600)} h ago`;
  return `${Math.round(d / 86400)} d ago`;
}

const openUrl = (url: string): void => void window.open(url, "_blank", "noopener");

const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
const toggle = (key: string) =>
  setCollapsed((prev) => {
    const next = new Set(prev);
    next.has(key) ? next.delete(key) : next.add(key);
    return next;
  });

async function reveal(g: PackageGroup): Promise<void> {
  const found = declarationOf(g.result, { package: g.name }, false);
  if (found) {
    const { path, problem } = found;
    await revealLocation(path, { line: problem.line, character: problem.start }, { line: problem.line, character: problem.end });
  } else {
    await revealLocation(g.result.lockfile ?? g.result.manifest, { line: 0, character: 0 });
  }
}

function confirmInstall(r: TargetResult): void {
  showBalloon(
    t("tools.vulnerabilities.installPipAuditIntoThe"),
    t("tools.vulnerabilities.pipAuditApache20"),
    0,
    [{ label: t("tools.vulnerabilities.installPipAudit"), run: () => void installTool(r) }],
  );
}

function AdvisoryRow(props: { v: Vulnerability }) {
  return (
    <div class="vuln-advisory" data-advisory={props.v.id}>
      <span class={`vuln-badge sev-${props.v.severity}`}>{LABEL[props.v.severity]}</span>
      <Show when={props.v.url} fallback={<span class="vuln-id">{props.v.id}</span>}>
        <a
          class="vuln-id vuln-link"
          href={props.v.url}
          title={[props.v.id, ...(props.v.aliases ?? [])].join(", ")}
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
            openUrl(props.v.url!);
          }}
        >
          {props.v.id}
          <ExternalLink size={11} />
        </a>
      </Show>
      <span class="vuln-title">{props.v.title}</span>
      <Show when={props.v.affected}>
        <span class="vuln-hint">affected {props.v.affected}</span>
      </Show>
      <span class="vuln-hint">{props.v.fixed ? t("tools.vulnerabilities.fixedIn", { fixed: props.v.fixed }) : t("tools.vulnerabilities.noFixYet")}</span>
      <Show when={props.v.reachable}>
        <span class="vuln-hint vuln-reachable">{t("tools.vulnerabilities.calledByTheProject")}</span>
      </Show>
    </div>
  );
}

function PackageRow(props: { g: PackageGroup }) {
  const open = () => !collapsed().has(props.g.key);
  const upgrade = () => declarationOf(props.g.result, { package: props.g.name })?.problem.upgrade;
  const count = () => props.g.vulnerabilities.length;
  return (
    <>
      <div class="tree-row vuln-package" data-package={props.g.name} onClick={() => void reveal(props.g)} tabIndex={0} onKeyDown={(e) => e.key === "Enter" && void reveal(props.g)}>
        <span
          class="tree-chevron"
          onClick={(e) => {
            e.stopPropagation();
            toggle(props.g.key);
          }}
        >
          {open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </span>
        <span class={`vuln-icon sev-${props.g.worst}`}>
          <ShieldAlert size={14} />
        </span>
        <span class="vuln-name">{props.g.name}</span>
        <Show when={props.g.installed}>
          <span class="vuln-version">{props.g.installed}</span>
        </Show>
        <Show when={props.g.via.length && !declarationOf(props.g.result, { package: props.g.name })}>
          <span class="vuln-hint">via {props.g.via.join(", ")}</span>
        </Show>
        <span class="vuln-hint">
          {count()} {count() === 1 ? "vulnerability" : "vulnerabilities"}
          {props.g.fixed ? t("tools.vulnerabilities.fixedIn2", { fixed: props.g.fixed }) : ""}
        </span>
        <Show when={upgrade()}>
          {(u) => {
            const path = () => declarationOf(props.g.result, { package: props.g.name })!.path;
            return (
              <button
                class="link-btn vuln-upgrade"
                title={t("tools.vulnerabilities.changeTheConstraintTo", { constraint: u().constraint })}
                onClick={(e) => {
                  e.stopPropagation();
                  void applyUpgrade(path(), u());
                }}
              >
                <ArrowUpCircle size={13} />
                {t("tools.vulnerabilities.upgradeTo", { fixed: u().fixed })}
              </button>
            );
          }}
        </Show>
      </div>
      <Show when={open()}>
        <For each={props.g.vulnerabilities}>{(v) => <AdvisoryRow v={v} />}</For>
      </Show>
    </>
  );
}

function TargetSection(props: { r: TargetResult }) {
  const list = createMemo(() => groups(props.r));
  const notices = () => props.r.notices ?? [];
  return (
    <div class="vuln-target" data-tool={props.r.tool}>
      <div class="vuln-target-head">
        <span class="vuln-target-file">{props.r.manifest}</span>
        <span class="vuln-hint">
          {TOOL_LABELS[props.r.tool]}
          {props.r.checkedAt ? t("tools.vulnerabilities.checked", { value: ago(props.r.checkedAt), value2: props.r.cached ? " (cached)" : "" }) : ""}
        </span>
      </div>
      <Show when={props.r.status !== "ok"}>
        <div class="vuln-status" classList={{ "vuln-status-error": props.r.status === "failed" }}>
          <Show when={props.r.status === "failed"} fallback={<Info size={14} />}>
            <CircleAlert size={14} />
          </Show>
          <span>{props.r.message}</span>
          <Show when={props.r.status === "missing" && props.r.tool === "pip-audit"}>
            <button class="link-btn" onClick={() => confirmInstall(props.r)}>
              {installTitle(props.r)}…
            </button>
          </Show>
        </div>
      </Show>
      <Show when={props.r.status === "ok" && !list().length && !notices().length}>
        <div class="vuln-status vuln-status-ok">
          <ShieldCheck size={14} />
          <span>{t("tools.vulnerabilities.noVulnerableDependencies")}</span>
        </div>
      </Show>
      <For each={list()}>{(g) => <PackageRow g={g} />}</For>
      <For each={notices()}>
        {(n) => (
          <div class="tree-row vuln-package vuln-notice">
            <span class="tree-chevron" />
            <span class="vuln-icon">
              <Archive size={14} />
            </span>
            <span class="vuln-name">{n.package}</span>
            <span class="vuln-hint">{n.message}</span>
          </div>
        )}
      </For>
    </div>
  );
}

export function VulnerabilitiesView() {
  const total = () => vulnerabilityResults().reduce((n, r) => n + r.vulnerabilities.length, 0);
  const packages = () => vulnerabilityResults().reduce((n, r) => n + groups(r).length, 0);
  const summary = () => {
    if (vulnerabilitiesRunning()) return "Checking dependencies…";
    if (vulnerabilitiesError()) return vulnerabilitiesError()!;
    if (!vulnerabilityResults().length) return "Check the project's dependencies for known vulnerabilities.";
    return total()
      ? `${total()} ${total() === 1 ? "vulnerability" : "vulnerabilities"} in ${packages()} ${packages() === 1 ? "package" : "packages"}`
      : "No vulnerable dependencies found";
  };
  return (
    <div class="vuln-view">
      <div class="vuln-bar">
        <button class="icon-btn" title={t("tools.vulnerabilities.checkForVulnerableDependencies")} disabled={vulnerabilitiesRunning()} onClick={() => void runCheck({ force: true })}>
          <RefreshCw size={14} class={vulnerabilitiesRunning() ? "vuln-spin" : undefined} />
        </button>
        <span class="vuln-summary">{summary()}</span>
      </div>
      <Show
        when={vulnerabilitiesEnabled()}
        fallback={
          <div class="vuln-empty">
            {t("tools.vulnerabilities.theVulnerableDependencyCheckIs")}
            <button class="link-btn" onClick={() => openSettings("tools.vulnerabilities")}>
              {t("tools.vulnerabilities.settingsToolsPackageChecker")}
            </button>
          </div>
        }
      >
        <div class="vuln-list">
          <For each={vulnerabilityResults()}>{(r) => <TargetSection r={r} />}</For>
        </div>
      </Show>
    </div>
  );
}
