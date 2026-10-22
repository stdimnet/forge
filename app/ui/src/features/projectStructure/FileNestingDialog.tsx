/**
 * File Nesting (Project view ⋮ › Appearance › File Nesting…), like PHPStorm: turn nesting
 * on or off and edit the rules (parent file suffix → child file suffixes, separated by `;`).
 * The rules start as PHPStorm's predefined ones (from Rust, `forge_scopes::view`); the
 * Project view applies them in Rust (`project_tree_list`).
 */
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { render } from "solid-js/web";
import { Minus, Plus, RotateCcw } from "lucide-solid";
import { suspendKeymap } from "@/keymap";
import { getSetting, saveSettings } from "@/state/settings";
import type { SettingValue } from "@/ipc/settings";
import type { NestingRule } from "@/ipc/projectStructure";
import { defaultNestingRules, storedNestingRules, VIEW_KEYS } from "./model";

const [open, setOpen] = createSignal(false);

/** Open the File Nesting dialog. */
export const showFileNestingDialog = () => setOpen(true);

const same = (a: NestingRule[], b: NestingRule[]) => JSON.stringify(a) === JSON.stringify(b);

function FileNestingDialog() {
  const initial = storedNestingRules().length ? storedNestingRules() : defaultNestingRules();
  const [enabled, setEnabled] = createSignal(getSetting<boolean>(VIEW_KEYS.nesting, true));
  const [rules, setRules] = createSignal<NestingRule[]>(initial.map((r) => ({ ...r })));
  const [selected, setSelected] = createSignal<number | null>(null);
  let dialog!: HTMLDivElement;

  const close = () => setOpen(false);
  const ok = async () => {
    const clean = rules()
      .map((r) => ({ parent: r.parent.trim(), children: r.children.trim() }))
      .filter((r) => r.parent && r.children);
    // The predefined rules are stored as "no rules", so new defaults reach the user.
    const stored = same(clean, defaultNestingRules()) ? [] : clean;
    close();
    await saveSettings({
      [VIEW_KEYS.nesting]: enabled(),
      [VIEW_KEYS.nestingRules]: stored as unknown as SettingValue,
    });
  };
  const update = (i: number, field: keyof NestingRule, value: string) =>
    setRules((all) => all.map((r, j) => (j === i ? { ...r, [field]: value } : r)));
  const add = () => {
    setRules((all) => [...all, { parent: "", children: "" }]);
    setSelected(rules().length - 1);
    requestAnimationFrame(() => dialog.querySelector<HTMLInputElement>(".nesting-row:last-child input")?.focus());
  };
  const remove = () => {
    const i = selected();
    if (i == null) return;
    setRules((all) => all.filter((_, j) => j !== i));
    setSelected(null);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") close();
    else if (e.key === "Enter" && !(e.target instanceof HTMLInputElement && e.target.type === "text")) void ok();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };

  const release = suspendKeymap();
  onCleanup(release);
  onMount(() => dialog.focus());

  return (
    <div class="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div class="git-dialog file-nesting-dialog" role="dialog" aria-label="File Nesting" tabIndex={-1} ref={dialog} onKeyDown={onKeyDown}>
        <header class="dialog-title">
          <span>File Nesting</span>
        </header>
        <div class="git-dialog-body">
          <label class="nesting-enable">
            <input type="checkbox" checked={enabled()} onChange={(e) => setEnabled(e.currentTarget.checked)} />
            <span>Enable file nesting</span>
          </label>
          <p class="nesting-hint">
            Files with the same name as a parent file and one of its child suffixes are shown below it in the Project view.
          </p>
          <div class="nesting-table" classList={{ disabled: !enabled() }}>
            <div class="nesting-toolbar">
              <button class="icon-btn" title="Add" aria-label="Add" disabled={!enabled()} onClick={add}>
                <Plus size={14} />
              </button>
              <button class="icon-btn" title="Remove" aria-label="Remove" disabled={!enabled() || selected() == null} onClick={remove}>
                <Minus size={14} />
              </button>
              <button
                class="icon-btn"
                title="Restore Defaults"
                aria-label="Restore Defaults"
                disabled={!enabled() || same(rules(), defaultNestingRules())}
                onClick={() => {
                  setRules(defaultNestingRules());
                  setSelected(null);
                }}
              >
                <RotateCcw size={13} />
              </button>
            </div>
            <div class="nesting-head">
              <span>Parent file suffix</span>
              <span>Child file suffix</span>
            </div>
            <div class="nesting-rows">
              <For each={rules()}>
                {(rule, i) => (
                  <div class="nesting-row" classList={{ selected: selected() === i() }} onMouseDown={() => setSelected(i())}>
                    <input
                      type="text"
                      value={rule.parent}
                      disabled={!enabled()}
                      spellcheck={false}
                      onChange={(e) => update(i(), "parent", e.currentTarget.value)}
                    />
                    <input
                      type="text"
                      value={rule.children}
                      disabled={!enabled()}
                      spellcheck={false}
                      onChange={(e) => update(i(), "children", e.currentTarget.value)}
                    />
                  </div>
                )}
              </For>
              <Show when={!rules().length}>
                <div class="nesting-empty">No rules</div>
              </Show>
            </div>
          </div>
        </div>
        <footer class="git-dialog-actions">
          <button class="btn-secondary" onClick={close}>
            Cancel
          </button>
          <button class="btn-primary" onClick={() => void ok()}>
            OK
          </button>
        </footer>
      </div>
    </div>
  );
}

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "file-nesting-overlay";
  document.body.append(host);
  render(() => <Show when={open()}>
      <FileNestingDialog />
    </Show>, host);
}
