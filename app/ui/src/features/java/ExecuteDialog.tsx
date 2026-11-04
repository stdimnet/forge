/**
 * Execute Gradle Task… / Execute Maven Goal…: a command line of tasks or goals with
 * completion from the build model (`:app:test`, `jacoco:report`) and the recent ones.
 * Enter runs it as a temporary Gradle / Maven configuration.
 */
import { createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { render } from "solid-js/web";
import { Cog, History } from "lucide-solid";
import { suspendKeymap } from "@/keymap";
import { isDialogWindow } from "@/shell/window/mode";
import { buildModel, refreshModel } from "./store";
import { gradleTask, runGradle, runMaven } from "./actions";
import { t as tr } from "@/i18n";

const [open, setOpen] = createSignal(false);
const [recent, setRecent] = createSignal<string[]>([]);

export function openExecuteDialog(): void {
  if (!buildModel()) void refreshModel();
  setOpen(true);
}

/** Task / goal names of the model: Gradle `:path:task`, Maven phases and `prefix:goal`. */
export function completions(): { name: string; detail: string }[] {
  const m = buildModel();
  if (!m) return [];
  const out = new Map<string, string>();
  for (const p of m.projects) {
    for (const t of p.tasks) {
      const name = m.tool === "gradle" ? gradleTask(p, t.name) : t.name;
      if (!out.has(name)) out.set(name, t.description);
    }
    for (const pl of p.plugins) for (const g of pl.goals) out.set(`${pl.prefix}:${g}`, pl.id);
  }
  return [...out.entries()].map(([name, detail]) => ({ name, detail }));
}

function Dialog() {
  const gradle = () => buildModel()?.tool !== "maven";
  const [line, setLine] = createSignal("");
  const [active, setActive] = createSignal(0);
  const release = suspendKeymap();
  onCleanup(release);
  /** The word being typed (the last one). */
  const word = () => line().split(/\s+/).pop() ?? "";
  const items = createMemo(() => {
    const w = word().toLowerCase();
    const history = recent()
      .filter((r) => !w || r.toLowerCase().includes(w))
      .map((r) => ({ name: r, detail: "recent", recent: true }));
    const names = completions()
      .filter((c) => !w || c.name.toLowerCase().includes(w))
      .sort((a, b) => Number(!a.name.toLowerCase().startsWith(w)) - Number(!b.name.toLowerCase().startsWith(w)))
      .map((c) => ({ ...c, recent: false }));
    return [...(w ? [] : history), ...names].slice(0, 12);
  });
  const close = () => setOpen(false);
  const accept = (name: string, recentLine: boolean) => {
    if (recentLine) setLine(name);
    else setLine(`${line().slice(0, line().length - word().length)}${name} `);
    setActive(0);
  };
  const run = () => {
    const cmd = line().trim();
    if (!cmd) return;
    setRecent((r) => [cmd, ...r.filter((x) => x !== cmd)].slice(0, 10));
    close();
    if (gradle()) runGradle(cmd);
    else runMaven(cmd);
  };
  return (
    <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        class="dialog java-execute-dialog"
        style={{ width: "min(560px, calc(100vw - 32px))" }}
        role="dialog"
        data-testid="java-execute-dialog"
        onKeyDown={(e) => {
          const list = items();
          if (e.key === "Escape") close();
          else if (e.key === "ArrowDown") setActive((a) => Math.min(a + 1, list.length - 1));
          else if (e.key === "ArrowUp") setActive((a) => Math.max(a - 1, 0));
          else if (e.key === "Tab" && list[active()]) accept(list[active()]!.name, list[active()]!.recent);
          else if (e.key === "Enter") {
            const pick = list[active()];
            if (pick && word() && pick.name !== word()) accept(pick.name, pick.recent);
            else run();
          } else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">{gradle() ? tr("langs.java.executeGradleTask") : tr("langs.java.executeMavenGoal")}</div>
        <div class="dialog-body">
          <label class="form-label">{gradle() ? tr("langs.java.commandLineTasksAndArguments") : tr("langs.java.commandLineGoalsAndArguments")}</label>
          <input
            ref={(el) => queueMicrotask(() => el.focus())}
            value={line()}
            placeholder={gradle() ? tr("langs.java.cleanAppTestInfo") : tr("langs.java.cleanInstallDskiptests")}
            spellcheck={false}
            data-testid="java-execute-input"
            onInput={(e) => {
              setLine(e.currentTarget.value);
              setActive(0);
            }}
          />
          <div class="java-execute-list">
            <For each={items()}>
              {(it, i) => (
                <div class="java-execute-item" classList={{ selected: i() === active() }} onMouseDown={(e) => (e.preventDefault(), accept(it.name, it.recent))}>
                  {it.recent ? <History size={13} class="muted" /> : <Cog size={13} class="muted" />}
                  <span class="java-mono">{it.name}</span>
                  <span class="java-bt-secondary">{it.detail}</span>
                </div>
              )}
            </For>
            <Show when={!items().length}>
              <div class="muted java-pad">{buildModel() ? tr("langs.java.noMatchingTasks") : tr("langs.java.noBuildInTheProject2")}</div>
            </Show>
          </div>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={close}>
            {tr("common.cancel")}
          </button>
          <button class="btn-primary" disabled={!line().trim()} onClick={run}>
            {tr("common.run2")}
          </button>
        </div>
      </div>
    </div>
  );
}

if (typeof document !== "undefined" && !isDialogWindow()) {
  const host = document.createElement("div");
  host.id = "java-execute-overlay";
  document.body.appendChild(host);
  render(() => (
    <Show when={open()}>
      <Dialog />
    </Show>
  ), host);
}
