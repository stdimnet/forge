/**
 * PHPStorm's Show Usages popup: the usages grouped by usage type, each with its code line
 * and file, keyboard navigable; Enter opens, the pin button keeps them in the Find tool
 * window.
 */
import { createMemo, createSignal, For, Match, onCleanup, onMount, Show, Switch, type JSX } from "solid-js";
import { Pin, Settings } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { toLsp } from "@/features/lsp/util";
import type { UsageRow } from "@/ipc/codeintel";
import { closeUsagesPopup, countLabel, openInFindWindow, openUsage, showUsagesSettings, type UsageItem, type UsagesPopupState } from "./usages";
import { t as tr } from "@/i18n";

type Line = { kind: "group"; label: string; count: number } | ({ kind: "usage"; index: number } & UsageItem);

const fileName = (path: string) => path.split("/").pop() ?? path;

/** Where the popup's search started, for the Find tool window's Settings button. */
const popupOrigin = (p: UsagesPopupState) => (p.origin ? { path: p.origin.path, pos: toLsp(p.origin.view.state.doc, p.origin.pos) } : undefined);

/** The code of a usage with the reference (every match of a text search) highlighted. */
export function UsageCode(props: { row: UsageRow }) {
  const parts = createMemo(() => {
    const t = props.row.text;
    const out: { text: string; hit: boolean }[] = [];
    let at = 0;
    for (const [s, e] of props.row.ranges) {
      if (s < at) continue;
      if (s > at) out.push({ text: t.slice(at, s), hit: false });
      out.push({ text: t.slice(s, e), hit: true });
      at = e;
    }
    if (at < t.length) out.push({ text: t.slice(at), hit: false });
    return out;
  });
  return (
    <span class="usage-code">
      <For each={parts()}>{(p) => (p.hit ? <mark class="usage-match">{p.text}</mark> : p.text)}</For>
    </span>
  );
}

export function ShowUsagesPopup(props: { popup: UsagesPopupState }) {
  let el!: HTMLDivElement;
  let list!: HTMLDivElement;
  const lines = createMemo<Line[]>(() => {
    const out: Line[] = [];
    let index = 0;
    for (const g of props.popup.answer.byType.groups) {
      out.push({ kind: "group", label: g.label, count: g.count });
      for (const f of g.files) for (const row of f.usages) out.push({ kind: "usage", index: index++, path: f.path, row });
    }
    return out;
  });
  const usages = createMemo(() => lines().filter((l): l is Extract<Line, { kind: "usage" }> => l.kind === "usage"));
  const [selected, setSelected] = createSignal(0);
  const total = () => props.popup.answer.byType.total;

  const away = (e: MouseEvent) => {
    if (!el.contains(e.target as Node)) closeUsagesPopup();
  };
  onMount(() => {
    window.addEventListener("mousedown", away, true);
    const r = el.getBoundingClientRect();
    if (r.bottom > window.innerHeight - 8) el.style.top = `${Math.max(8, props.popup.y - r.height - 24)}px`;
    if (r.right > window.innerWidth - 8) el.style.left = `${Math.max(8, window.innerWidth - r.width - 8)}px`;
    el.focus();
  });
  onCleanup(() => window.removeEventListener("mousedown", away, true));

  const open = (i: number) => {
    const u = usages()[i];
    if (!u) return;
    closeUsagesPopup();
    openUsage(u);
  };
  const select = (i: number) => {
    const n = usages().length;
    setSelected(((i % n) + n) % n);
    list.querySelector(`[data-usage="${selected()}"]`)?.scrollIntoView({ block: "nearest" });
  };
  // The popup owns these keys while it has focus (like the Choose Declaration list).
  const onKey: JSX.EventHandler<HTMLDivElement, KeyboardEvent> = (e) => {
    if (e.key === "ArrowDown") select(selected() + 1);
    else if (e.key === "ArrowUp") select(selected() - 1);
    else if (e.key === "PageDown") select(Math.min(selected() + 10, usages().length - 1));
    else if (e.key === "PageUp") select(Math.max(selected() - 10, 0));
    else if (e.key === "Enter") open(selected());
    else if (e.key === "Escape") closeUsagesPopup();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div
      class="usages-popup"
      ref={el}
      tabIndex={-1}
      role="dialog"
      aria-label={tr("codeintel.showUsagesPopup.usagesOfTitle", { title: props.popup.answer.title })}
      style={{ left: `${props.popup.x}px`, top: `${props.popup.y}px` }}
      onKeyDown={onKey}
    >
      <div class="usages-popup-header">
        <span class="usages-popup-title">
          {tr("tools.codeintel.usagesOf")} <strong>{props.popup.answer.title}</strong> in {props.popup.answer.scope ?? tr("tools.codeintel.projectFiles")}
        </span>
        <span class="usages-popup-count">{countLabel(total())}</span>
        <Show when={props.popup.origin}>
          <button
            class="usages-popup-pin"
            title={tr("codeintel.showUsagesPopup.settings", { shortcut: "Ctrl+Alt+Shift+F7" })}
            aria-label={tr("codeintel.showUsagesPopup.findUsagesSettings")}
            onClick={() => showUsagesSettings(props.popup)}
          >
            <Settings size={14} />
          </button>
        </Show>
        <button
          class="usages-popup-pin"
          title={tr("codeintel.showUsagesPopup.openInFindWindow")}
          aria-label={tr("codeintel.showUsagesPopup.openInFindWindow")}
          onClick={() => openInFindWindow(props.popup.answer, { origin: popupOrigin(props.popup) })}
        >
          <Pin size={14} />
        </button>
      </div>
      <div class="usages-popup-list" ref={list} role="listbox">
        <For each={lines()}>
          {(line) => (
            <Switch>
              <Match when={line.kind === "group" && line}>
                {(g) => (
                  <div class="usages-popup-group">
                    {g().label}
                    <span class="usages-popup-group-count">{g().count}</span>
                  </div>
                )}
              </Match>
              <Match when={line.kind === "usage" && line}>
                {(u) => (
                  <button
                    class="usages-popup-row"
                    classList={{ selected: u().index === selected() }}
                    role="option"
                    aria-selected={u().index === selected()}
                    data-usage={u().index}
                    onMouseEnter={() => setSelected(u().index)}
                    onClick={() => open(u().index)}
                  >
                    <FileIcon name={fileName(u().path)} size={14} />
                    <UsageCode row={u().row} />
                    <span class="usages-popup-file">
                      {fileName(u().path)} <span class="usages-popup-line">{u().row.line + 1}</span>
                    </span>
                  </button>
                )}
              </Match>
            </Switch>
          )}
        </For>
      </div>
    </div>
  );
}
