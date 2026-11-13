/**
 * Settings | Appearance & Behavior | Scopes, like PHPStorm's page: the named scopes (Local
 * ones in the user settings, Shared ones in the project) on the left; the selected scope's
 * name and pattern on the right above the project tree, where the files in the scope are
 * highlighted and Include / Include Recursively / Exclude / Exclude Recursively add the
 * selected file or folder to the pattern. Edits go to the dialog draft (Apply / Cancel).
 */
import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import { ChevronDown, ChevronRight, Copy, Minus, Plus } from "lucide-solid";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { FileIcon } from "@/shell/FileIcon";
import { scopesApi, type NamedScope, type ScopeEditOp, type ScopeTreeNode } from "@/ipc/scopes";
import { showResolvedMenu } from "@/features/contextMenu/state";
import { checkPattern, editPattern, LOCAL_SCOPES, scopeContext, scopeEntries, scopeTree, SHARED_SCOPES } from "./model";
import { t } from "@/i18n";

type Storage = "local" | "shared";
const keyOf = (s: Storage) => (s === "shared" ? SHARED_SCOPES : LOCAL_SCOPES);

interface Selection {
  storage: Storage;
  index: number;
}

function uniqueName(taken: string[], base = "Scope"): string {
  let name = base;
  for (let i = 2; taken.includes(name); i++) name = `${base} (${i})`;
  return name;
}

export function ScopesPage() {
  const list = (s: Storage) => draftValue<NamedScope[]>(keyOf(s)) ?? [];
  const setList = (s: Storage, scopes: NamedScope[]) => setDraft(keyOf(s), scopes);
  const allNames = () => [...list("local"), ...list("shared")].map((s) => s.name);
  const [selected, setSelected] = createSignal<Selection | null>(
    list("local").length ? { storage: "local", index: 0 } : list("shared").length ? { storage: "shared", index: 0 } : null,
  );
  const current = () => {
    const s = selected();
    return s ? list(s.storage)[s.index] : undefined;
  };
  const update = (patch: Partial<NamedScope>) => {
    const s = selected();
    if (!s) return;
    setList(
      s.storage,
      list(s.storage).map((x, i) => (i === s.index ? { ...x, ...patch } : x)),
    );
  };
  const add = (storage: Storage, from?: NamedScope) => {
    const scope = { name: uniqueName(allNames(), from ? `${from.name} copy` : "Scope"), pattern: from?.pattern ?? "" };
    setList(storage, [...list(storage), scope]);
    setSelected({ storage, index: list(storage).length - 1 });
  };
  const remove = () => {
    const s = selected();
    if (!s) return;
    setList(
      s.storage,
      list(s.storage).filter((_, i) => i !== s.index),
    );
    const rest = list(s.storage).length;
    setSelected(rest ? { storage: s.storage, index: Math.min(s.index, rest - 1) } : null);
  };
  const addMenu = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    showResolvedMenu(
      [
        { kind: "action", label: t("tools.scopes.local"), run: () => add("local") },
        { kind: "action", label: t("tools.scopes.shared"), run: () => add("shared") },
      ],
      r.left,
      r.bottom + 2,
    );
  };

  const problems = createMemo(() => scopeEntries({ shared: list("shared"), local: list("local") }).problems);
  const nameProblem = () => problems().find(([name, message]) => name === current()?.name && message.includes("name"))?.[1];
  const patternError = createMemo(() => checkPattern(current()?.pattern ?? ""));

  // The project's files and the tree under the pattern.
  const [files] = createResource(() => scopesApi.files().catch(() => [] as string[]));
  // The dialog may run in its own window: ask for the test folders here.
  const [testRoots] = createResource(() => scopesApi.testRoots().catch(() => [] as string[]));
  const tree = createMemo(() =>
    current()
      ? scopeTree(files() ?? [], {
          scope: { pattern: current()!.pattern },
          context: { ...scopeContext(), testRoots: testRoots() ?? [] },
        })
      : null,
  );
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set());
  const [treeSel, setTreeSel] = createSignal<{ path: string; dir: boolean } | null>(null);
  const toggle = (path: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  const press = (op: ScopeEditOp) => {
    const sel = treeSel();
    const scope = current();
    if (!sel || !scope) return;
    update({ pattern: editPattern(scope.pattern, op, sel.path, sel.dir) });
  };

  const Node = (props: { node: ScopeTreeNode; depth: number }) => {
    const open = () => expanded().has(props.node.path);
    return (
      <>
        <div
          class="scp-tree-row"
          classList={{
            selected: treeSel()?.path === props.node.path,
            "in-scope": props.node.matched,
            partial: props.node.dir && !props.node.matched && props.node.matchedFiles > 0,
          }}
          style={{ "padding-left": `${6 + props.depth * 16}px` }}
          data-path={props.node.path}
          onClick={() => setTreeSel({ path: props.node.path, dir: props.node.dir })}
          onDblClick={() => props.node.dir && toggle(props.node.path)}
        >
          <span
            class="scp-chevron"
            onClick={(e) => {
              e.stopPropagation();
              if (props.node.dir) toggle(props.node.path);
            }}
          >
            <Show when={props.node.dir}>{open() ? <ChevronDown size={13} /> : <ChevronRight size={13} />}</Show>
          </span>
          <FileIcon name={props.node.name} dir={props.node.dir} open={open()} size={14} />
          <span class="scp-name">{props.node.name}</span>
          <Show when={props.node.dir && props.node.matchedFiles > 0}>
            <span class="scp-count">
              {props.node.matchedFiles} of {props.node.totalFiles}
            </span>
          </Show>
        </div>
        <Show when={props.node.dir && open()}>
          <For each={props.node.children ?? []}>{(c) => <Node node={c} depth={props.depth + 1} />}</For>
        </Show>
      </>
    );
  };

  const Group = (props: { storage: Storage; title: string }) => (
    <Show when={list(props.storage).length}>
      <div class="scp-group">{props.title}</div>
      <For each={list(props.storage)}>
        {(scope, i) => (
          <div
            class="scp-item"
            classList={{ selected: selected()?.storage === props.storage && selected()?.index === i() }}
            data-scope={scope.name}
            onClick={() => setSelected({ storage: props.storage, index: i() })}
          >
            <span>{scope.name || t("tools.scopes.unnamed")}</span>
            <Show when={problems().some(([n]) => n === scope.name)}>
              <span class="scp-bad" title={t("tools.scopes.thisScopeHasAProblem")} />
            </Show>
          </div>
        )}
      </For>
    </Show>
  );

  return (
    <div class="scp-page">
      <div class="scp-list set-list">
        <div class="set-list-toolbar">
          <button class="icon-btn" title={t("tools.scopes.addScope")} aria-label={t("tools.scopes.addScope")} onClick={addMenu}>
            <Plus size={14} />
          </button>
          <button class="icon-btn" title={t("common.remove")} aria-label={t("common.remove")} disabled={!current()} onClick={remove}>
            <Minus size={14} />
          </button>
          <button
            class="icon-btn"
            title={t("common.copy")}
            aria-label={t("common.copy")}
            disabled={!current()}
            onClick={() => current() && add(selected()!.storage, current())}
          >
            <Copy size={14} />
          </button>
        </div>
        <div class="scp-items">
          <Group storage="local" title={t("tools.scopes.local")} />
          <Group storage="shared" title={t("tools.scopes.shared")} />
          <Show when={!list("local").length && !list("shared").length}>
            <div class="set-list-empty">{t("tools.scopes.noScopesDefined")}</div>
          </Show>
        </div>
      </div>
      <Show
        when={current()}
        fallback={
          <div class="scp-hint set-desc">
            {t("tools.scopes.scopesAreNamedSetsOf")}
          </div>
        }
      >
        {(scope) => (
          <div class="scp-editor">
            <label class="scp-field">
              <span>{t("common.nameLabel")}</span>
              <input
                class="set-input"
                classList={{ invalid: !!nameProblem() }}
                value={scope().name}
                spellcheck={false}
                onInput={(e) => update({ name: e.currentTarget.value })}
              />
              <span class="scp-storage">{selected()?.storage === "shared" ? t("tools.scopes.sharedProject") : t("tools.scopes.local")}</span>
            </label>
            <Show when={nameProblem()}>
              <div class="set-error">{nameProblem()}</div>
            </Show>
            <label class="scp-field">
              <span>{t("tools.scopes.patternLabel")}</span>
              <input
                class="set-input scp-pattern"
                classList={{ invalid: !!patternError() }}
                value={scope().pattern}
                spellcheck={false}
                placeholder={t("tools.scopes.fileAppPhpTest")}
                aria-label={t("tools.scopes.pattern")}
                onInput={(e) => update({ pattern: e.currentTarget.value })}
              />
            </label>
            <Show when={patternError()}>
              {(err) => (
                <div class="set-error">
                  {t("tools.scopes.atPosition", { message: err().message, value: err().offset + 1 })}
                </div>
              )}
            </Show>
            <div class="scp-tree-box">
              <div class="scp-tree" role="tree">
                <For each={tree()?.nodes ?? []}>{(n) => <Node node={n} depth={0} />}</For>
              </div>
              <div class="scp-buttons">
                <button class="btn-secondary" disabled={!treeSel()} onClick={() => press("include")}>
                  {t("tools.scopes.include")}
                </button>
                <button class="btn-secondary" disabled={!treeSel()?.dir} onClick={() => press("includeRecursively")}>
                  {t("tools.scopes.includeRecursively")}
                </button>
                <button class="btn-secondary" disabled={!treeSel()} onClick={() => press("exclude")}>
                  {t("tools.scopes.exclude")}
                </button>
                <button class="btn-secondary" disabled={!treeSel()?.dir} onClick={() => press("excludeRecursively")}>
                  {t("tools.scopes.excludeRecursively")}
                </button>
              </div>
            </div>
            <div class="scp-legend">
              <span class="scp-swatch" /> {t("tools.scopes.inScope")}
              <span class="scp-sep">·</span>
              {t("tools.scopes.ofFiles", { value: tree()?.matched ?? 0, value2: tree()?.total ?? 0 })}
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}
