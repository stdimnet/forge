/**
 * Structure breadcrumbs (Settings | Editor | General | Breadcrumbs): the classes, functions,
 * CSS rules and HTML elements around the caret, at the bottom of the editor by default or
 * at the top. Clicking a crumb moves the caret there; the arrow next to it lists its
 * siblings. The file path stays in the bar above the editor (PHPStorm's navigation bar).
 */
import { createEffect, createRoot, createSignal, For, on, Show } from "solid-js";
import { render } from "solid-js/web";
import { StateEffect, StateField, type Extension } from "@codemirror/state";
import { EditorView, showPanel, ViewPlugin, type Panel } from "@codemirror/view";
import { ChevronDown, ChevronRight } from "lucide-solid";
import { SymbolIcon } from "@/features/php/SymbolIcon";
import type { SymbolKind } from "@/features/php/api";
import { openMenu } from "@/features/run/PopupMenu";
import type { IconComponent } from "@/features/run/types";
import { breadcrumbsOn, breadcrumbsOnTop, scopesAt, structureOf, structureReady, type Scope, type ScopeKind } from "./model";

/** The Structure view's badge for a scope kind. */
export function symbolKindOf(kind: ScopeKind): SymbolKind {
  switch (kind) {
    case "closure":
      return "function";
    case "object":
      return "variable";
    case "control":
      return "module";
    default:
      return kind;
  }
}

/** A crumb: a scope and its index in the document's scope list. */
interface Crumb {
  scope: Scope;
  index: number;
}

/** The crumbs at the main caret, outermost first. */
export function crumbsAt(view: EditorView, language: string): Crumb[] {
  const { state } = view;
  const head = state.selection.main.head;
  const line = state.doc.lineAt(head);
  const structure = structureOf(state.doc, language);
  const inside = new Set(scopesAt(structure, line.number - 1, head - line.from));
  return structure.scopes.flatMap((scope, index) => (scope.crumb && inside.has(scope) ? [{ scope, index }] : []));
}

/** Move the caret to a scope's name and scroll it into view. */
export function revealScope(view: EditorView, scope: Scope): void {
  const doc = view.state.doc;
  if (scope.line >= doc.lines) return;
  const line = doc.line(scope.line + 1);
  const pos = Math.min(line.from + scope.column, line.to);
  view.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: "nearest", yMargin: 60 }) });
  view.focus();
}

function CrumbBar(props: { view: EditorView; language: string; crumbs: () => Crumb[] }) {
  const siblings = (crumb: Crumb, e: MouseEvent) => {
    e.stopPropagation();
    const scopes = structureOf(props.view.state.doc, props.language).scopes;
    const list = scopes.filter((s) => s.crumb && s.parent === crumb.scope.parent);
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openMenu({
      x: rect.left,
      y: breadcrumbsOnTop() ? rect.bottom + 2 : rect.top - 4 - Math.min(list.length, 20) * 24,
      items: list.map((s) => ({
        label: s.name,
        icon: (() => <SymbolIcon kind={symbolKindOf(s.kind)} />) as unknown as IconComponent,
        run: () => revealScope(props.view, s),
      })),
    });
  };
  return (
    <div class="cm-structure-crumbs" role="navigation" aria-label="Breadcrumbs">
      <For each={props.crumbs()}>
        {(crumb, i) => (
          <>
            <Show when={i() > 0}>
              <ChevronRight size={12} class="cm-crumb-sep" />
            </Show>
            <span
              class="cm-crumb"
              classList={{ last: i() === props.crumbs().length - 1 }}
              title={crumb.scope.name}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => revealScope(props.view, crumb.scope)}
            >
              <SymbolIcon kind={symbolKindOf(crumb.scope.kind)} />
              <span class="cm-crumb-name">{crumb.scope.name}</span>
              <button
                class="cm-crumb-more"
                title="Show siblings"
                onMouseDown={(e) => e.preventDefault()}
                onClick={(e) => siblings(crumb, e)}
              >
                <ChevronDown size={11} />
              </button>
            </span>
          </>
        )}
      </For>
    </div>
  );
}

interface CrumbConfig {
  show: boolean;
  top: boolean;
}

const setCrumbConfig = StateEffect.define<CrumbConfig>();

function crumbPanel(language: string, top: boolean) {
  return (view: EditorView): Panel => {
    const dom = document.createElement("div");
    dom.className = "cm-breadcrumbs-panel";
    const same = (a: Crumb[], b: Crumb[]) => a.length === b.length && a.every((c, i) => c.scope === b[i].scope);
    const [crumbs, setCrumbs] = createSignal<Crumb[]>(crumbsAt(view, language), { equals: same });
    const dispose = render(() => <CrumbBar view={view} language={language} crumbs={crumbs} />, dom);
    const stopReady = createRoot((d) => {
      createEffect(on(structureReady, () => setCrumbs(crumbsAt(view, language)), { defer: true }));
      return d;
    });
    return {
      dom,
      top,
      update(u) {
        if (u.docChanged || u.selectionSet) setCrumbs(crumbsAt(u.view, language));
      },
      destroy() {
        stopReady();
        dispose();
      },
    };
  };
}

/** The breadcrumbs extension of an editor for `language`. */
export function structureBreadcrumbs(language: string): Extension {
  const constructors = new Map<string, ReturnType<typeof crumbPanel>>();
  const panelFor = (c: CrumbConfig) => {
    if (!c.show) return null;
    const key = c.top ? "top" : "bottom";
    if (!constructors.has(key)) constructors.set(key, crumbPanel(language, c.top));
    return constructors.get(key)!;
  };
  const config = StateField.define<CrumbConfig>({
    create: () => ({ show: breadcrumbsOn(language), top: breadcrumbsOnTop() }),
    update(value, tr) {
      for (const e of tr.effects) if (e.is(setCrumbConfig)) return e.value;
      return value;
    },
    provide: (f) => showPanel.from(f, panelFor),
  });
  // Settings changes reach open editors.
  const follow = ViewPlugin.define((view) => {
    const dispose = createRoot((d) => {
      createEffect(
        on(
          () => ({ show: breadcrumbsOn(language), top: breadcrumbsOnTop() }),
          (next) => {
            const cur = view.state.field(config);
            if (cur.show !== next.show || cur.top !== next.top) queueMicrotask(() => view.dispatch({ effects: setCrumbConfig.of(next) }));
          },
          { defer: true },
        ),
      );
      return d;
    });
    return { destroy: dispose };
  });
  return [config, follow];
}
