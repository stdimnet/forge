/**
 * PHPStorm's Show Usages popup: the usages grouped by usage type, each with its code line
 * and file, keyboard navigable; Enter opens, the pin button keeps them in the Find tool
 * window.
 */
import { createMemo, createSignal, For, Match, onCleanup, onMount, Switch, type JSX } from "solid-js";
import { Pin } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import type { UsageRow } from "@/ipc/codeintel";
import { closeUsagesPopup, countLabel, openInFindWindow, openUsage, type UsageItem, type UsagesPopupState } from "./usages";

type Line = { kind: "group"; label: string; count: number } | ({ kind: "usage"; index: number } & UsageItem);

const fileName = (path: string) => path.split("/").pop() ?? path;

/** The code of a usage with the reference highlighted. */
export function UsageCode(props: { row: UsageRow }) {
  const parts = () => {
    const [s, e] = props.row.ranges[0] ?? [0, 0];
    const t = props.row.text;
    return [t.slice(0, s), t.slice(s, e), t.slice(e)];
  };
  return (
    <span class="usage-code">
      {parts()[0]}
      <mark class="usage-match">{parts()[1]}</mark>
      {parts()[2]}
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
      aria-label={`Usages of ${props.popup.answer.title}`}
      style={{ left: `${props.popup.x}px`, top: `${props.popup.y}px` }}
      onKeyDown={onKey}
    >
      <div class="usages-popup-header">
        <span class="usages-popup-title">
          Usages of <strong>{props.popup.answer.title}</strong> in Project Files
        </span>
        <span class="usages-popup-count">{countLabel(total())}</span>
        <button
          class="usages-popup-pin"
          title="Open in Find Window"
          aria-label="Open in Find Window"
          onClick={() => openInFindWindow(props.popup.answer)}
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
