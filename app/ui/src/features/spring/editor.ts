/**
 * Spring in the editor: gutter icons in Java / Kotlin files (a bean → where it is injected,
 * an injection point → the matching beans, endpoints → HTTP Client and the Endpoints tool
 * window, JPA entities ↔ repositories, `@ConfigurationProperties` → the configuration
 * values), and `application*.properties` / `application*.yml` support: completion of keys
 * and values with documentation, the unknown / deprecated property and invalid value
 * inspections (with "Replace with …" for deprecated keys), hover documentation and Ctrl+B
 * from a key to its `@ConfigurationProperties` field. Everything is computed in Rust.
 */
import { EditorState, type Extension } from "@codemirror/state";
import { hoverTooltip, ViewPlugin, type EditorView, type ViewUpdate } from "@codemirror/view";
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { Database, Globe, Leaf, Link2, Send, SlidersHorizontal, Table2 } from "lucide-solid";
import { springApi, type ConfigProblem, type EndpointRow, type SpringMarker, type SpringTarget } from "@/ipc/spring";
import type { EditorDiagnostic } from "@/ipc/lsp";
import type { ContextAction } from "@/ipc/codeintel";
import { registerEditorExtension } from "@/editor/extensions";
import { registerLineMarkerProvider, type LineMarker } from "@/editor/lineMarkers";
import { registerDeclarationProvider } from "@/features/lsp/goto";
import { publishDiagnostics } from "@/features/lsp/store";
import { toLsp } from "@/features/lsp/util";
import { docDom } from "@/features/languages/shared";
import { toOption } from "@/features/jsonSchema/editor";
import { registerContextActionHandler, registerContextActionProvider } from "@/features/codeintel/intentions";
import { chooseDeclarationAt } from "@/features/navigation/declarationChooser";
import { revealSymbol } from "@/features/php/editorViews";
import { openMenu } from "@/features/run/PopupMenu";
import { showEndpointsView } from "@/features/endpoints/registry";
import { getSetting } from "@/state/settings";
import { openInHttpClient, overview, springActive } from "./store";
import { setEndpointsQuery } from "./EndpointsView";

const SOURCE = "spring-config";

/** `application.yml`, `application-dev.properties`, `bootstrap.yaml`. */
export const isConfigFile = (path: string) => /(^|[/\\])(application|bootstrap)(-[\w.-]+)?\.(properties|ya?ml)$/.test(path);
const isSource = (path: string) => /\.(java|kt)$/.test(path);

// ---- Gutter ---------------------------------------------------------------------------------

const ICONS: Record<SpringMarker["kind"], LineMarker["icon"]> = {
  bean: Leaf,
  injection: Link2,
  endpoint: Globe,
  entity: Table2,
  repository: Database,
  configProperties: SlidersHorizontal,
};

function go(view: EditorView, line: number, targets: SpringTarget[], title: string): void {
  const open = (t: { path: string; line: number }) => void revealSymbol(t.path, { line: t.line });
  if (targets.length === 1) open(targets[0]);
  else if (targets.length > 1) chooseDeclarationAt(view, view.state.doc.line(Math.min(view.state.doc.lines, line + 1)).from, targets, { title, open });
}

/** The endpoint rows of a marker (by method and path; the model's indices may be stale). */
function endpointsOf(m: SpringMarker): EndpointRow[] {
  const titles = m.title.split(", ");
  return (overview()?.endpoints ?? []).filter((e) => titles.includes(`${e.method} ${e.path}`));
}

registerLineMarkerProvider({
  appliesTo: isSource,
  watch: () => [springActive(), getSetting<boolean>("spring.gutterIcons", true)],
  markers: async (path, text) => {
    if (!springActive() || !getSetting<boolean>("spring.gutterIcons", true)) return [];
    const marks = await springApi.gutter(path, text).catch(() => [] as SpringMarker[]);
    return marks.map((m): LineMarker => {
      const base = { line: m.line, icon: ICONS[m.kind], title: m.title, className: `marker-spring marker-spring-${m.kind}` };
      if (m.kind === "endpoint") {
        return {
          ...base,
          onClick: (_view, e) => {
            const rows = endpointsOf(m);
            openMenu({
              x: e.clientX,
              y: e.clientY + 6,
              items: [
                ...rows.map((r) => ({ label: `Open ${r.method} ${r.path} in HTTP Client`, icon: Send, iconColor: "var(--success)", run: () => void openInHttpClient(r) })),
                { separator: true },
                {
                  label: "Show in Endpoints",
                  icon: Globe,
                  run: () => {
                    setEndpointsQuery(rows[0]?.path ?? "");
                    showEndpointsView("spring");
                  },
                },
              ],
            });
          },
        };
      }
      const what = m.kind === "bean" ? "Injection Points" : m.kind === "injection" ? "Autowired Candidates" : m.kind === "configProperties" ? "Configuration Values" : "Related";
      return { ...base, onClick: (view) => go(view, m.line, m.targets, what) };
    });
  },
});

// ---- Configuration files --------------------------------------------------------------------

function completionSource(path: string) {
  return async (ctx: CompletionContext): Promise<CompletionResult | null> => {
    if (!springActive()) return null;
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    if (/^\s*[#!]/.test(before)) return null;
    if (!ctx.explicit && !/[\w.\-:= ]$/.test(before)) return null;
    const answer = await springApi.complete(path, ctx.state.doc.toString(), ctx.pos).catch(() => null);
    if (!answer || ctx.aborted || !answer.items.length) return null;
    return {
      from: answer.from,
      options: answer.items.map((item, i) => toOption(item, i, answer, ctx.pos)),
      // While every match was returned, keep filtering locally.
      validFor: answer.incomplete ? undefined : /^[\w.\-[\]]*$/,
    };
  };
}

const problemsByPath = new Map<string, ConfigProblem[]>();

function diagnostics(path: string): Extension {
  return ViewPlugin.define((view) => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let seq = 0;
    const run = async () => {
      timer = undefined;
      const id = ++seq;
      if (!springActive()) {
        publishDiagnostics(path, [], SOURCE);
        return;
      }
      const text = view.state.doc.toString();
      const problems = await springApi.inspect(path, text).catch(() => null);
      if (id !== seq || !problems || view.state.doc.toString() !== text) return;
      problemsByPath.set(path, problems);
      const doc = view.state.doc;
      const diags: EditorDiagnostic[] = problems.map((p) => ({
        range: { start: toLsp(doc, p.from), end: toLsp(doc, Math.max(p.to, p.from)) },
        severity: p.severity,
        message: p.message,
        source: "Spring Boot",
        code: p.code,
      }));
      publishDiagnostics(path, diags, SOURCE);
    };
    const schedule = (delay: number) => {
      clearTimeout(timer);
      timer = setTimeout(() => void run(), delay);
    };
    schedule(0);
    return {
      update(u: ViewUpdate) {
        if (u.docChanged) schedule(400);
      },
      destroy() {
        clearTimeout(timer);
        problemsByPath.delete(path);
        publishDiagnostics(path, [], SOURCE);
      },
    };
  });
}

function hover(path: string): Extension {
  return hoverTooltip(
    async (view, pos) => {
      if (!springActive()) return null;
      const h = await springApi.hover(path, view.state.doc.toString(), pos).catch(() => null);
      if (!h) return null;
      return {
        pos: h.from,
        end: h.to,
        above: false,
        create: () => ({ dom: docDom({ title: view.state.sliceDoc(h.from, h.to), doc: h.markdown }) }),
      };
    },
    { hoverTime: 350 },
  );
}

registerEditorExtension(({ path }): Extension | null => {
  if (!isConfigFile(path)) return null;
  const data = [{ autocomplete: completionSource(path) }];
  return [EditorState.languageData.of(() => data), diagnostics(path), hover(path)];
});

registerDeclarationProvider(async (path, view, pos) => {
  if (!springActive() || !isConfigFile(path)) return false;
  const targets = await springApi.goto(path, view.state.doc.toString(), pos).catch(() => []);
  if (!targets.length) return false;
  chooseDeclarationAt(view, pos, targets, { title: "Configuration Property", open: (t) => void revealSymbol(t.path, { line: t.line }) });
  return true;
});

// Alt+Enter on a deprecated key: replace it with its replacement.
registerContextActionProvider(async (path, view) => {
  if (!isConfigFile(path)) return [];
  const pos = view.state.selection.main.head;
  const p = (problemsByPath.get(path) ?? []).find((x) => x.fix && x.from <= pos && pos <= x.to);
  if (!p) return [];
  const action: ContextAction = {
    title: `Replace with '${p.replacement}'`,
    kind: "quickFix",
    source: "spring",
    isPreferred: true,
    needsResolve: false,
    action: "Spring.ReplaceDeprecatedProperty",
    data: p,
  };
  return [action];
});

registerContextActionHandler("Spring.ReplaceDeprecatedProperty", (data, { view }) => {
  const p = data as ConfigProblem;
  if (p.fix) view.dispatch({ changes: { from: p.from, to: p.to, insert: p.fix } });
});
