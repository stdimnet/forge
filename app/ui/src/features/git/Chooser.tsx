/** Modal list for the Git actions that pick one entry: revisions, branches, stashes, remotes. */
import { createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { Search } from "lucide-solid";
import { suspendKeymap } from "@/keymap";
import { setChooserDialog, type ChooserDialog, type ChooserItem } from "./overlays";
import { t } from "@/i18n";

export function Chooser(props: { d: ChooserDialog }) {
  const [query, setQuery] = createSignal("");
  const [selected, setSelected] = createSignal<string | null>(props.d.items[0]?.id ?? null);
  const [checks, setChecks] = createSignal((props.d.checkboxes ?? []).map((c) => c.checked));
  let input!: HTMLInputElement;
  let list!: HTMLDivElement;

  const visible = createMemo(() => {
    const q = query().trim().toLowerCase();
    return q ? props.d.items.filter((i) => `${i.label} ${i.detail ?? ""}`.toLowerCase().includes(q)) : props.d.items;
  });
  const current = (): ChooserItem | null => visible().find((i) => i.id === selected()) ?? null;
  const close = () => setChooserDialog(null);
  const ok = async () => {
    const item = current();
    if (!props.d.onOk) return close();
    if (!item) return;
    close();
    await props.d.onOk(item, checks());
  };
  const move = (dir: 1 | -1) => {
    const items = visible();
    if (!items.length) return;
    const at = items.findIndex((i) => i.id === selected());
    const next = items[Math.max(0, Math.min(items.length - 1, at + dir))];
    setSelected(next.id);
    list.querySelector(`[data-id="${CSS.escape(next.id)}"]`)?.scrollIntoView({ block: "nearest" });
  };
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") close();
    else if (e.key === "Enter") void ok();
    else if (e.key === "ArrowDown") move(1);
    else if (e.key === "ArrowUp") move(-1);
    else return;
    e.preventDefault();
    e.stopPropagation();
  };

  const release = suspendKeymap();
  onCleanup(release);
  onMount(() => input.focus());

  return (
    <div class="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div class="git-dialog git-chooser" role="dialog" aria-label={props.d.title} onKeyDown={onKeyDown}>
        <header class="dialog-title">
          <span>{props.d.title}</span>
        </header>
        <div class="git-dialog-body">
          <label class="git-chooser-search">
            <Search size={13} class="muted" />
            <input ref={input} value={query()} onInput={(e) => setQuery(e.currentTarget.value)} spellcheck={false} placeholder={t("git.chooser.search")} />
          </label>
          <div class="git-chooser-list" ref={list} role="listbox">
            <For each={visible()} fallback={<div class="git-chooser-empty">{props.d.empty}</div>}>
              {(item) => (
                <div
                  class="git-chooser-row"
                  classList={{ selected: item.id === selected() }}
                  role="option"
                  aria-selected={item.id === selected()}
                  data-id={item.id}
                  onClick={() => setSelected(item.id)}
                  onDblClick={() => void ok()}
                >
                  <span class="git-chooser-label">{item.label}</span>
                  <Show when={item.detail}>
                    <span class="git-chooser-detail">{item.detail}</span>
                  </Show>
                </div>
              )}
            </For>
          </div>
          <For each={props.d.checkboxes ?? []}>
            {(c, i) => (
              <label class="git-dialog-check">
                <input
                  type="checkbox"
                  checked={checks()[i()]}
                  onChange={(e) => {
                    const on = e.currentTarget.checked;
                    setChecks((prev) => prev.map((v, j) => (j === i() ? on : v)));
                  }}
                />
                {c.label}
              </label>
            )}
          </For>
        </div>
        <footer class="git-dialog-actions">
          <For each={props.d.extra ?? []}>
            {(b) => (
              <button class="btn-secondary" disabled={b.needsSelection && !current()} onClick={() => void b.run(current())}>
                {b.label}
              </button>
            )}
          </For>
          <span class="spacer" />
          <Show
            when={props.d.onOk}
            fallback={
              <button class="btn-primary" onClick={close}>
                {t("common.close")}
              </button>
            }
          >
            <button class="btn-secondary" onClick={close}>
              {t("common.cancel")}
            </button>
            <button class="btn-primary" disabled={!current()} onClick={() => void ok()}>
              {props.d.okLabel}
            </button>
          </Show>
        </footer>
      </div>
    </div>
  );
}
