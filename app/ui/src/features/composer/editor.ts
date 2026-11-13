/**
 * composer.json in the editor, like PHPStorm: the Composer toolbar above the editor
 * (Install, Update, Validate, Show Outdated, Audit, Add Dependency…), outdated packages as
 * gutter icons and weak warnings with the "Update package" quick fix (Alt+Enter),
 * completion of package names and version constraints and the package hover. Everything
 * is computed in Rust (`composer_*`); this module renders the answers.
 */
import { createEffect, createRoot, createSignal } from "solid-js";
import { EditorState, type Extension } from "@codemirror/state";
import { hoverTooltip, showPanel, ViewPlugin, type Panel } from "@codemirror/view";
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { CircleArrowUp, Package, Settings } from "lucide-solid";
import { composerApi, type OutdatedMarker } from "@/ipc/composer";
import type { ContextAction } from "@/ipc/codeintel";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { registerContextActionHandler, registerContextActionProvider } from "@/features/codeintel/intentions";
import { publishDiagnostics } from "@/features/lsp/store";
import { h, iconNode } from "@/features/lsp/util";
import { docDom } from "@/features/languages/shared";
import { toOption } from "@/features/jsonSchema/editor";
import { openSettings } from "@/features/settings/dialogState";
import { runAction } from "@/keymap";
import { getSetting } from "@/state/settings";
import { composerActive, composerRevision, composerRunning, isComposerJson, runComposer } from "./state";
import { updateCommand } from "./model";

const SOURCE = "composer-outdated";

const [outdated, setOutdated] = createSignal<Record<string, OutdatedMarker[]>>({});
export { outdated as outdatedMarkers };

function setMarkers(path: string, markers: OutdatedMarker[]): void {
  setOutdated((prev) => ({ ...prev, [path]: markers }));
  const diags: EditorDiagnostic[] = markers.map((m) => ({
    range: { start: m.start, end: m.end },
    severity: "weak",
    message: m.message,
    source: "Composer",
  }));
  publishDiagnostics(path, diags, SOURCE);
}

/** "Update package": `composer update <name>` within the constraint, else `composer require`. */
export function updatePackage(m: Pick<OutdatedMarker, "name" | "status" | "suggested" | "dev">, composerJson?: string): Promise<number | null> {
  return runComposer(updateCommand(m), composerJson);
}

registerLineMarkerProvider({
  appliesTo: isComposerJson,
  watch: () => [composerActive(), composerRevision(), getSetting<boolean>("php.composer.packagist", true)],
  markers: async (path, text) => {
    if (!composerActive()) {
      setMarkers(path, []);
      return [];
    }
    const markers = await composerApi.markers(path, text).catch(() => [] as OutdatedMarker[]);
    setMarkers(path, markers);
    return markers.map((m) => ({
      line: m.line,
      icon: CircleArrowUp,
      title: `${m.name}: ${m.installed} → ${m.latest}. Click to update`,
      className: "marker-composer-outdated",
      onClick: () => void updatePackage(m, path),
    }));
  },
});

// Alt+Enter on an outdated requirement.
registerContextActionProvider(async (path, view) => {
  if (!isComposerJson(path)) return [];
  const pos = view.state.selection.main.head;
  const line = view.state.doc.lineAt(pos).number - 1;
  const m = (outdated()[path] ?? []).find((x) => x.line === line);
  if (!m) return [];
  const action: ContextAction = {
    title: `Update '${m.name}' to ${m.latest}`,
    kind: "quickFix",
    source: "composer",
    isPreferred: true,
    needsResolve: false,
    action: "Composer.UpdatePackage",
    data: { ...m, path },
  };
  return [action];
});

registerContextActionHandler("Composer.UpdatePackage", (data) => {
  const m = data as OutdatedMarker & { path: string };
  void updatePackage(m, m.path);
});

/** The Composer toolbar above composer.json. */
function composerBanner(path: string): Extension {
  return showPanel.of((): Panel => {
    const dom = h("div", "composer-banner");
    dom.hidden = true;
    const link = (label: string, action: string, title?: string) => {
      const b = h("button", "link-btn composer-banner-link", label);
      b.dataset.composerAction = action;
      if (title) b.title = title;
      b.onclick = () => void runAction(action);
      return b;
    };
    const actions = [
      link("Install", "Composer.Install", "composer install"),
      link("Update", "Composer.Update", "composer update"),
      link("Validate", "Composer.Validate", "composer validate"),
      link("Show Outdated", "Composer.ShowOutdated", "composer outdated --direct"),
      link("Audit", "Composer.Audit", "composer audit"),
    ];
    const info = h("span", "composer-banner-info");
    const add = link("Add Dependency…", "Composer.AddDependency");
    const settings = h("button", "icon-btn composer-banner-settings");
    settings.title = "Composer Settings";
    settings.append(iconNode(Settings, 14));
    settings.onclick = () => openSettings("lang.php.composer");
    dom.append(iconNode(Package, 14, "composer-banner-icon"), h("span", "composer-banner-title", "Composer"), ...actions, info, add, settings);
    let dispose = () => {};
    createRoot((d) => {
      dispose = d;
      createEffect(() => {
        dom.hidden = !composerActive();
        const busy = composerRunning();
        for (const b of actions) b.disabled = busy;
        const n = (outdated()[path] ?? []).length;
        info.textContent = busy ? "Running Composer…" : n ? `${n} package${n === 1 ? " has" : "s have"} updates` : "";
      });
    });
    return { dom, top: true, destroy: () => dispose() };
  });
}

function completionSource(path: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!composerActive()) return null;
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    if (!ctx.explicit && !/[\w"$.\-/@^~*]$/.test(before)) return null;
    const answer = await composerApi.complete(path, ctx.state.doc.toString(), ctx.pos).catch(() => null);
    if (!answer || ctx.aborted || !answer.items.length) return null;
    return {
      from: answer.from,
      options: answer.items.map((item, i) => toOption(item, i, answer, ctx.pos)),
      validFor: /^[\w$.\-/@^~*]*$/,
    };
  };
}

function composerHover(path: string): Extension {
  return hoverTooltip(
    async (view, pos) => {
      if (!composerActive()) return null;
      const hover = await composerApi.hover(path, view.state.doc.toString(), pos).catch(() => null);
      if (!hover) return null;
      return {
        pos: hover.from,
        end: hover.to,
        above: false,
        create: () => ({ dom: docDom({ title: hover.title, doc: hover.markdown }) }),
      };
    },
    { hoverTime: 350 },
  );
}

/** Clears the file's markers when its editor closes. */
const cleanup = (path: string) =>
  ViewPlugin.define(() => ({
    destroy() {
      setOutdated((prev) => {
        const next = { ...prev };
        delete next[path];
        return next;
      });
      publishDiagnostics(path, [], SOURCE);
    },
  }));

/** Composer support of one composer.json editor. */
export function composerEditor(path: string): Extension | null {
  if (!isComposerJson(path)) return null;
  const data = [{ autocomplete: completionSource(path) }];
  return [composerBanner(path), EditorState.languageData.of(() => data), composerHover(path), cleanup(path)];
}
