/**
 * `composer audit`: runs on the project's machine (computed and parsed in Rust), shows the
 * advisories as problems in composer.json and composer.lock (Problems tool window, editor
 * squiggles) and in the "Vulnerable Dependencies" tab of the Problems tool window
 * (advisory id, package, affected versions, link), like IntelliJ's package checker.
 */
import { createSignal, For, Show } from "solid-js";
import { ExternalLink, RefreshCw, ShieldAlert } from "lucide-solid";
import { composerApi, type AuditResult, type FileProblem } from "@/ipc/composer";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { balloonsFor } from "@/keymap";
import { publishDiagnostics } from "@/features/lsp/store";
import { revealLocation } from "@/features/lsp/navigation";
import { setProblemsTab } from "@/features/lsp/ProblemsView";
import { showToolWindow } from "@/state/layout";

const showBalloon = balloonsFor("Composer");

export const AUDIT_TAB = "composer.audit";
const SOURCE = "composer-audit";

const [result, setResult] = createSignal<AuditResult | null>(null);
const [running, setRunning] = createSignal(false);
const [error, setError] = createSignal<string | null>(null);
export { result as auditResult };

const toDiagnostic = (p: FileProblem): EditorDiagnostic => ({
  range: { start: p.start, end: p.end },
  severity: p.severity,
  message: p.message,
  source: "Composer audit",
});

let published: string[] = [];

function publish(r: AuditResult | null): void {
  for (const path of published) publishDiagnostics(path, [], SOURCE);
  published = [];
  if (!r) return;
  publishDiagnostics(r.composerJson, r.jsonProblems.map(toDiagnostic), SOURCE);
  publishDiagnostics(r.composerLock, r.lockProblems.map(toDiagnostic), SOURCE);
  published = [r.composerJson, r.composerLock];
}

/** Composer.Audit: run `composer audit` and show the result. */
export async function runAudit(opts: { reveal?: boolean } = {}): Promise<AuditResult | null> {
  if (running()) return result();
  setRunning(true);
  setError(null);
  try {
    const r = await composerApi.audit();
    setResult(r);
    publish(r);
    const n = r.advisories.length;
    const packages = new Set(r.advisories.map((a) => a.packageName)).size;
    if (opts.reveal !== false) {
      setProblemsTab(AUDIT_TAB);
      showToolWindow("problems");
    }
    if (n) showBalloon(`Found ${n} security vulnerability advisor${n === 1 ? "y" : "ies"} affecting ${packages} package${packages === 1 ? "" : "s"}`, undefined, 6000, undefined, { type: "warning" });
    else showBalloon("No security vulnerability advisories found");
    return r;
  } catch (e) {
    const message = String(e instanceof Error ? e.message : e);
    setError(message);
    showBalloon("Composer audit failed", message, 8000, undefined, { type: "error" });
    return null;
  } finally {
    setRunning(false);
  }
}

function summary(): string {
  const r = result();
  if (!r) return error() ?? "Run composer audit to check the installed packages for security advisories.";
  const n = r.advisories.length;
  const packages = new Set(r.advisories.map((a) => a.packageName)).size;
  const abandoned = r.abandoned.length ? `, ${r.abandoned.length} abandoned` : "";
  return n
    ? `${n} advisor${n === 1 ? "y" : "ies"} in ${packages} package${packages === 1 ? "" : "s"}${abandoned}`
    : `No security vulnerability advisories found${abandoned}`;
}

/** Badge of the tab. */
export const auditCount = () => (result()?.advisories.length ?? 0) + (result()?.abandoned.length ?? 0);

const openLink = (url: string) => window.open(url, "_blank", "noopener");

async function revealPackage(name: string): Promise<void> {
  const r = result();
  if (!r) return;
  const inJson = r.jsonProblems.find((p) => p.package === name);
  const inLock = r.lockProblems.find((p) => p.package === name);
  if (inJson) await revealLocation(r.composerJson, inJson.start, inJson.end);
  else if (inLock) await revealLocation(r.composerLock, inLock.start, inLock.end);
}

/** The "Vulnerable Dependencies" tab. */
export function AuditView() {
  return (
    <div class="composer-audit">
      <div class="composer-audit-bar">
        <button class="icon-btn" title="Run composer audit" disabled={running()} onClick={() => void runAudit({ reveal: false })}>
          <RefreshCw size={14} />
        </button>
        <span class="composer-audit-status">
          {running() ? "Running composer audit…" : summary()}
        </span>
      </div>
      <Show when={result()?.advisories.length || result()?.abandoned.length}>
        <table class="composer-audit-table">
          <thead>
            <tr>
              <th>Package</th>
              <th>Advisory</th>
              <th>Affected versions</th>
              <th>Severity</th>
              <th>Title</th>
            </tr>
          </thead>
          <tbody>
            <For each={result()!.advisories}>
              {(a) => (
                <tr class="composer-audit-row" onClick={() => void revealPackage(a.packageName)}>
                  <td class="composer-audit-package">
                    <ShieldAlert size={14} class="composer-audit-icon" />
                    {a.packageName}
                  </td>
                  <td>
                    <Show when={a.link} fallback={a.cve ?? a.advisoryId}>
                      <a
                        class="composer-link"
                        href={a.link}
                        onClick={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          openLink(a.link!);
                        }}
                      >
                        {a.cve ?? a.advisoryId}
                        <ExternalLink size={11} />
                      </a>
                    </Show>
                  </td>
                  <td class="composer-mono">{a.affectedVersions}</td>
                  <td class={`composer-severity sev-${a.severity ?? "unknown"}`}>{a.severity ?? "—"}</td>
                  <td class="composer-audit-title">{a.title}</td>
                </tr>
              )}
            </For>
            <For each={result()!.abandoned}>
              {(a) => (
                <tr class="composer-audit-row" onClick={() => void revealPackage(a.packageName)}>
                  <td class="composer-audit-package">{a.packageName}</td>
                  <td>Abandoned</td>
                  <td />
                  <td />
                  <td class="composer-audit-title">{a.replacement ? `Use ${a.replacement} instead` : "No replacement suggested"}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>
    </div>
  );
}
