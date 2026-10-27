/**
 * Inspections, like PHPStorm: Settings | Editor | Inspections (profiles), Code | Inspect
 * Code…, Run Inspection by Name… (Ctrl+Alt+Shift+I), Code Cleanup…, Analyze Code | Inspect
 * Current File and Locate Duplicates…, the Inspection Results tool window, the
 * profile applied to every problem the editor shows, and Alt+Enter's Suppress actions. The
 * logic lives in Rust (`forge-inspections`, `commands/inspections.rs`).
 */
import { createEffect, createRoot, on, Show } from "solid-js";
import { render } from "solid-js/web";
import { ViewPlugin, type ViewUpdate } from "@codemirror/view";
import { ListChecks, X } from "lucide-solid";
import { inspectionsApi } from "@/ipc/inspections";
import { registerToolWindow } from "@/features/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerStatusItem } from "@/features/statusItems";
import { registerDraftSource } from "@/features/settings/dialogState";
import { registerCommand } from "@/state/commands";
import { registerEditorExtension } from "@/editor/extensions";
import { refilterDiagnostics } from "@/features/lsp/store";
import { isDialogWindow } from "@/shell/window/mode";
import { workspace } from "@/state/workspace";
import { activeTab } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { showBalloon } from "@/keymap";
import { InspectionsPage } from "./InspectionsPage";
import { ResultsTabs, ResultsView } from "./ResultsView";
import { applyDraft, discardDraft, hasChanges, INSPECTIONS_PAGE, resetPage } from "./draft";
import { askCleanup, askInspectionByName, askScope, byNamePopup, cleanupPreview, CleanupPreview, RunByNamePopup, ScopeDialog, scopeDialog } from "./dialogs";
import { loadInspections, setProfiles } from "./model";
import { applyCleanup, cancelRun, cleanupPlan, RESULTS_WINDOW, resultTabs, runInspections, runningInspection } from "./run";
import "./suppress";
import "./inspections.css";

registerSettingsPage({
  id: INSPECTIONS_PAGE,
  title: "Inspections",
  group: "Editor",
  order: 25,
  keywords: ["inspection", "profile", "severity", "suppress", "noinspection", "duplicates", "data flow", "Project Default"],
  component: InspectionsPage,
});

registerDraftSource({
  modified: (page) => page === INSPECTIONS_PAGE && hasChanges(),
  hasChanges,
  apply: applyDraft,
  discard: discardDraft,
  resetPage: (page) => page === INSPECTIONS_PAGE && resetPage(),
});

registerToolWindow({
  id: RESULTS_WINDOW,
  title: "Inspection Results",
  icon: ListChecks,
  anchor: "bottom",
  order: 58,
  component: ResultsView,
  headerContent: ResultsTabs,
  // Like PHPStorm, the stripe button appears once there are results.
  when: () => resultTabs().length > 0,
});

async function inspectCode(): Promise<void> {
  const req = await askScope({ title: "Specify Inspection Scope", confirm: "Analyze" });
  if (req) await runInspections(req);
}

async function runByName(): Promise<void> {
  const spec = await askInspectionByName();
  if (!spec) return;
  if (spec.batch === false) {
    showBalloon(`'${spec.name}' runs in the editor only`, "Its problems come from a language server or tool while files are open.");
    return;
  }
  const req = await askScope({ title: `Run '${spec.name}'`, confirm: "OK", inspection: spec });
  if (req) await runInspections(req);
}

async function codeCleanup(): Promise<void> {
  const req = await askScope({ title: "Specify Code Cleanup Scope", confirm: "OK" });
  if (!req) return;
  const plan = await cleanupPlan(req);
  if (!plan) return;
  if (!plan.files.length) {
    showBalloon("Code Cleanup", "No safe fixes found in the scope.");
    return;
  }
  if (await askCleanup(plan)) await applyCleanup(plan);
}

function inspectCurrentFile(): void {
  const path = activeTab()?.path;
  if (!path) return;
  void runInspections({ scope: { kind: "file", path, label: `File '${path}'` } });
}

async function locateDuplicates(): Promise<void> {
  const req = await askScope({ title: "Locate Duplicates", confirm: "OK", inspection: { id: "DuplicatedCode", name: "Duplicated code fragment", group: "General", severity: "WEAK WARNING", enabled: true } });
  if (req) await runInspections(req);
}

registerCommand({ id: "InspectCode", title: "Inspect Code…", category: "Code", run: inspectCode });
registerCommand({ id: "RunInspection", title: "Run Inspection by Name…", category: "Code", run: runByName });
registerCommand({ id: "CodeCleanup", title: "Code Cleanup…", category: "Code", run: codeCleanup });
registerCommand({ id: "CodeInspection.OnEditor", title: "Inspect Current File", category: "Code", run: inspectCurrentFile });
registerCommand({ id: "LocateDuplicates", title: "Locate Duplicates…", category: "Code", run: locateDuplicates });

function InspectingStatus() {
  return (
    <Show when={runningInspection()}>
      {(r) => {
        const percent = () => (r().total ? Math.round((100 * r().done) / r().total) : 0);
        return (
          <span class="status-item insp-status" title={r().text}>
            <span>
              {r().title}… {percent()}%
            </span>
            <span class="insp-progress">
              <span class="insp-progress-bar" style={{ width: `${percent()}%` }} />
            </span>
            <button class="icon-btn insp-cancel" title="Cancel" aria-label="Cancel inspection" onClick={cancelRun}>
              <X size={12} />
            </button>
          </span>
        );
      }}
    </Show>
  );
}

registerStatusItem({ id: "inspections.progress", align: "right", order: -9, component: InspectingStatus });

// Suppression comments typed in the editor hide their problems right away.
registerEditorExtension(({ path }) =>
  ViewPlugin.define(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    return {
      update(u: ViewUpdate) {
        if (!u.docChanged) return;
        clearTimeout(timer);
        timer = setTimeout(() => refilterDiagnostics(path), 400);
      },
      destroy() {
        clearTimeout(timer);
      },
    };
  }),
);

if (typeof document !== "undefined") {
  // Profiles: when a project opens, when plugins change and when any window saves them.
  createRoot(() => {
    createEffect(
      on(
        [() => workspace()?.root, () => JSON.stringify(getSetting<string[]>("plugins.disabled", []))],
        () => void loadInspections().catch((e) => console.warn("inspections could not be loaded", e)),
      ),
    );
  });
  void inspectionsApi.onChanged((p) => setProfiles(p));

  if (!isDialogWindow()) {
    const host = document.createElement("div");
    host.id = "inspections-overlay";
    document.body.appendChild(host);
    createRoot(() =>
      render(
        () => (
          <>
            <Show when={scopeDialog()} keyed>
              {(r) => <ScopeDialog req={r} />}
            </Show>
            <Show when={byNamePopup()} keyed>
              {(done) => <RunByNamePopup done={done} />}
            </Show>
            <Show when={cleanupPreview()} keyed>
              {(c) => <CleanupPreview plan={c.plan} done={c.done} />}
            </Show>
          </>
        ),
        host,
      ),
    );
  }
}
