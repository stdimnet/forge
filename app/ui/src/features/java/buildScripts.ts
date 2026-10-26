/**
 * Gradle build scripts (`*.gradle.kts`, `*.gradle`) and `pom.xml` in the editor, like
 * IntelliJ IDEA: completion of the DSL (blocks, configurations, `java { toolchain }`,
 * `tasks.register`, …), of dependency coordinates and versions from Maven Central, and the
 * "A newer version is available" inspection with its gutter icon and quick fix
 * (Alt+Enter). Everything is computed in Rust (`java_build_complete`,
 * `java_build_outdated`); highlighting comes from the Kotlin and Groovy modes.
 */
import { createSignal } from "solid-js";
import { EditorState, type Extension } from "@codemirror/state";
import { ViewPlugin } from "@codemirror/view";
import { startCompletion, type CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import { CircleArrowUp } from "lucide-solid";
import { javaApi, type OutdatedDependency } from "@/ipc/java";
import type { ContextAction } from "@/ipc/codeintel";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { registerContextActionHandler, registerContextActionProvider } from "@/features/codeintel/intentions";
import { publishDiagnostics } from "@/features/lsp/store";
import { toOption } from "@/features/jsonSchema/editor";
import { getSetting } from "@/state/settings";
import { active } from "./store";

const SOURCE = "java-outdated";

/** A Gradle build or settings script, or a Maven pom. */
export const isBuildScript = (path: string) => /(^|[/\\])(pom\.xml|[^/\\]+\.gradle(\.kts)?)$/.test(path);

const [outdated, setOutdated] = createSignal<Record<string, OutdatedDependency[]>>({});

function setMarkers(path: string, markers: OutdatedDependency[]): void {
  setOutdated((prev) => ({ ...prev, [path]: markers }));
  const diags: EditorDiagnostic[] = markers.map((m) => ({
    range: { start: m.start, end: m.end },
    severity: "weak",
    message: m.message,
    source: "Maven Central",
    code: "NewerVersionAvailable",
  }));
  publishDiagnostics(path, diags, SOURCE);
}

registerLineMarkerProvider({
  appliesTo: isBuildScript,
  watch: () => [active(), getSetting<boolean>("java.mavenCentral.enabled", true)],
  markers: async (path, text) => {
    if (!active() || !getSetting<boolean>("java.mavenCentral.enabled", true)) {
      setMarkers(path, []);
      return [];
    }
    const markers = await javaApi.buildOutdated(path, text).catch(() => [] as OutdatedDependency[]);
    setMarkers(path, markers);
    return markers.map((m) => ({
      line: m.line,
      icon: CircleArrowUp,
      title: `${m.group}:${m.artifact}: ${m.current} → ${m.latest}`,
      className: "marker-composer-outdated",
      onClick: (view) => {
        const doc = view.state.doc;
        // The marker's range is in the text it was computed for; the version is still there
        // unless it was edited since.
        if (doc.sliceString(m.from, m.to) === m.current) view.dispatch({ changes: { from: m.from, to: m.to, insert: m.latest } });
      },
    }));
  },
});

// Alt+Enter on a declaration with a newer version.
registerContextActionProvider(async (path, view) => {
  if (!isBuildScript(path)) return [];
  const pos = view.state.selection.main.head;
  const line = view.state.doc.lineAt(pos).number - 1;
  const m = (outdated()[path] ?? []).find((x) => x.line === line || x.start.line === line);
  if (!m) return [];
  const action: ContextAction = {
    title: `Change to ${m.latest}`,
    kind: "quickFix",
    source: "java",
    isPreferred: true,
    needsResolve: false,
    action: "Java.UpdateDependencyVersion",
    data: m,
  };
  return [action];
});

registerContextActionHandler("Java.UpdateDependencyVersion", (data, { view }) => {
  const m = data as OutdatedDependency;
  if (view.state.doc.sliceString(m.from, m.to) !== m.current) return;
  view.dispatch({ changes: { from: m.from, to: m.to, insert: m.latest } });
});

function completionSource(path: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!active()) return null;
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    if (!ctx.explicit && !/[\w"'.:>\-]$/.test(before)) return null;
    const answer = await javaApi.buildComplete(path, ctx.state.doc.toString(), ctx.pos).catch(() => null);
    if (!answer || ctx.aborted || !answer.items.length) return null;
    return {
      from: answer.from,
      options: answer.items.map((item, i) => toOption(item, i, answer, ctx.pos)),
      validFor: /^[\w$.\-:]*$/,
    };
  };
}

/** Typing `:` in a coordinate string (`"com.google.guava:`) asks again (artifacts, versions). */
const reopenAfterColon = ViewPlugin.define((view) => ({
  update(u) {
    if (!u.docChanged || !active()) return;
    let at = -1;
    u.changes.iterChanges((_a, _b, _from, to, inserted) => {
      if (inserted.toString() === ":") at = to;
    });
    if (at < 0) return;
    const line = u.state.doc.lineAt(at);
    const before = line.text.slice(0, at - line.from);
    const quotes = (before.match(/["']/g) ?? []).length;
    if (quotes % 2 === 1) setTimeout(() => startCompletion(view), 0);
  },
}));

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

/** Build script support of one editor. */
export function buildScriptEditor(path: string): Extension | null {
  if (!isBuildScript(path)) return null;
  const data = [{ autocomplete: completionSource(path) }];
  return [EditorState.languageData.of(() => data), reopenAfterColon, cleanup(path)];
}
