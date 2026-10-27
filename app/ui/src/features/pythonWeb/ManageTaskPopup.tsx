/**
 * Tools | Run manage.py Task… (PyCharm's): a popup that completes the project's manage.py
 * commands (from `manage.py help --commands` in trusted projects, else Django's built-in
 * commands and the project's `management/commands`) and runs the typed task with its
 * arguments as a temporary "manage.py" run in the Run tool window.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { RefreshCw, SquareTerminal, X } from "lucide-solid";
import { pythonWebApi, type ManageCommand, type ManageCommands } from "@/ipc/pythonWeb";
import { launch } from "@/features/run/runStore";
import { useTemporary } from "@/features/run/configStore";
import { shortcutLabel } from "@/keymap";

const [open, setOpen] = createSignal(false);
const [text, setText] = createSignal("");
const [list, setList] = createSignal<ManageCommands | null>(null);
const [loading, setLoading] = createSignal(false);
const [selected, setSelected] = createSignal(0);
const [history, setHistory] = createSignal<string[]>([]);

export { open as manageTaskOpen };

async function load(refresh = false): Promise<void> {
  setLoading(true);
  try {
    setList(await pythonWebApi.manageCommands(refresh));
  } catch (e) {
    setList({ commands: [], source: "static", message: String(e) });
  } finally {
    setLoading(false);
  }
}

export function openManageTask(prefill = ""): void {
  setText(prefill);
  setSelected(0);
  setOpen(true);
  void load();
}

export function closeManageTask(): void {
  setOpen(false);
}

/** The command word typed so far (completion applies only while it is the first word). */
const commandWord = () => text().trimStart().split(/\s+/)[0] ?? "";
const typingCommand = () => !/\s/.test(text().trimStart());

/** A manage.py task as a temporary run configuration. */
export function manageConfiguration(task: string) {
  return { name: `manage.py ${task}`.slice(0, 60), type: "django-manage", options: { task } };
}

export async function runManageTask(task: string): Promise<void> {
  const t = task.trim();
  if (!t) return;
  setHistory((h) => [t, ...h.filter((x) => x !== t)].slice(0, 20));
  await launch(useTemporary(manageConfiguration(t)));
}

export function ManageTaskPopup() {
  let input: HTMLInputElement | undefined;
  const matches = createMemo<ManageCommand[]>(() => {
    const all = list()?.commands ?? [];
    if (!typingCommand()) return all.filter((c) => c.name === commandWord());
    const q = commandWord().toLowerCase();
    const starts = all.filter((c) => c.name.toLowerCase().startsWith(q));
    const contains = all.filter((c) => !c.name.toLowerCase().startsWith(q) && c.name.toLowerCase().includes(q));
    return [...starts, ...contains];
  });
  const complete = (c: ManageCommand) => {
    setText(`${c.name} `);
    setSelected(0);
    input?.focus();
  };
  const run = () => {
    const pick = typingCommand() ? matches()[selected()] : undefined;
    const task = pick && pick.name !== commandWord() ? pick.name : text();
    closeManageTask();
    void runManageTask(task);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const n = matches().length;
    if (e.key === "Escape") {
      e.preventDefault();
      closeManageTask();
    } else if (e.key === "ArrowDown" && n) {
      e.preventDefault();
      setSelected((i) => (i + 1) % n);
    } else if (e.key === "ArrowUp" && n) {
      e.preventDefault();
      setSelected((i) => (i - 1 + n) % n);
    } else if (e.key === "Tab" && n && typingCommand()) {
      e.preventDefault();
      complete(matches()[selected()]);
    } else if (e.key === "Enter") {
      e.preventDefault();
      run();
    }
  };
  return (
    <Show when={open()}>
      <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeManageTask()}>
        <div class="popup pyweb-manage-popup" role="dialog" aria-label="Run manage.py Task">
          <div class="cli-popup-header">
            <SquareTerminal size={15} class="cli-popup-icon" />
            <span class="cli-popup-title">manage.py@{list()?.source === "manage.py" ? "project" : "Django"}</span>
            <span class="muted pyweb-manage-source">
              {list()?.source === "manage.py" ? "Commands from manage.py help --commands" : "Built-in and project commands"}
            </span>
            <span class="kbd cli-popup-shortcut">{shortcutLabel("Django.RunManageTask")}</span>
            <button class="icon-btn" title="Reload Commands" onClick={() => void load(true)}>
              <RefreshCw size={13} classList={{ spin: loading() }} />
            </button>
            <button class="icon-btn" title="Close (Esc)" onClick={closeManageTask}>
              <X size={14} />
            </button>
          </div>
          <Show when={list()?.message}>
            <div class="cli-popup-notice">{list()!.message}</div>
          </Show>
          <div class="pyweb-manage-input">
            <span class="pyweb-manage-prompt">manage.py</span>
            <input
              ref={(el) => {
                input = el;
                queueMicrotask(() => el.focus());
              }}
              value={text()}
              placeholder="Enter a task, e.g. migrate or makemigrations catalog"
              spellcheck={false}
              onInput={(e) => {
                setText(e.currentTarget.value);
                setSelected(0);
              }}
              onKeyDown={onKeyDown}
            />
          </div>
          <div class="pyweb-manage-list">
            <For each={matches().slice(0, 200)}>
              {(c, i) => (
                <div
                  class="pyweb-manage-item"
                  classList={{ selected: selected() === i() }}
                  data-manage-command={c.name}
                  onMouseEnter={() => setSelected(i())}
                  onClick={() => complete(c)}
                  onDblClick={() => {
                    setText(c.name);
                    run();
                  }}
                >
                  <span class="pyweb-manage-name">{c.name}</span>
                  <span class="pyweb-manage-app">{c.app}</span>
                  <span class="muted pyweb-manage-desc">{c.description}</span>
                </div>
              )}
            </For>
            <Show when={!matches().length && !loading()}>
              <div class="pyweb-manage-empty muted">
                <Show when={history().length} fallback="No matching commands">
                  Recent: {history().slice(0, 5).join(" · ")}
                </Show>
              </div>
            </Show>
          </div>
          <div class="cli-popup-footer muted">
            <span>Tab to complete · ↑↓ to choose · Enter to run in the Run tool window · Esc to close</span>
          </div>
        </div>
      </div>
    </Show>
  );
}
