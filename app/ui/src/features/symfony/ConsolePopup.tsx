/**
 * Run Console Command popup (Tools | Symfony | Run Console Command…): a speed-search list of
 * `bin/console` commands like PHPStorm's Run Anything. Text after the command name is passed
 * on as arguments.
 */
import { createEffect, createMemo, createSignal, For, on, onMount, Show } from "solid-js";
import { SquareTerminal } from "lucide-solid";
import { fuzzyMatch } from "@/features/search/fuzzy";
import { Highlight } from "@/features/search/SearchEverywhere";
import { commands, loadSymfony } from "./store";
import { runConsoleCommand, splitCommandLine } from "./console";

const [open, setOpen] = createSignal(false);
const [initial, setInitial] = createSignal("");

export { open as consolePopupOpen };

export function openConsolePopup(text = ""): void {
  setInitial(text);
  setOpen(true);
  if (!commands().length) void loadSymfony();
}

export const closeConsolePopup = () => setOpen(false);

export function ConsolePopup() {
  let input!: HTMLInputElement;
  let list!: HTMLDivElement;
  const [query, setQuery] = createSignal(initial());
  const [index, setIndex] = createSignal(0);

  const parsed = () => splitCommandLine(query());
  const shown = createMemo(() => {
    const q = parsed().command;
    return commands()
      .filter((c) => !c.hidden)
      .map((c) => {
        const names = [c.name, ...(c.aliases ?? [])];
        const m = q ? names.map((n) => fuzzyMatch(q, n)).find((x) => x) ?? null : { score: 0, positions: [] as number[] };
        return { c, m };
      })
      .filter((x) => x.m)
      .sort((a, b) => (q ? b.m!.score - a.m!.score : 0) || Number(!!a.c.builtin) - Number(!!b.c.builtin));
  });

  onMount(() => {
    input.focus();
    input.setSelectionRange(input.value.length, input.value.length);
  });
  createEffect(on(() => parsed().command, () => setIndex(0), { defer: true }));
  createEffect(() => list?.querySelector(`[data-index="${index()}"]`)?.scrollIntoView({ block: "nearest" }));

  function run(i = index()) {
    const pick = shown()[i]?.c;
    const { command, args } = parsed();
    const name = pick && (pick.name === command || pick.aliases?.includes(command) || !args) ? pick.name : command;
    if (!name) return;
    closeConsolePopup();
    void runConsoleCommand(name, args);
  }

  function onKeyDown(e: KeyboardEvent) {
    const n = shown().length;
    if (e.key === "Escape") closeConsolePopup();
    else if (e.key === "ArrowDown") setIndex((i) => (n ? (i + 1) % n : 0));
    else if (e.key === "ArrowUp") setIndex((i) => (n ? (i - 1 + n) % n : 0));
    else if (e.key === "Enter") run();
    else if (e.key === "Tab" && shown()[index()]) {
      setQuery(`${shown()[index()].c.name} `);
    } else return;
    e.preventDefault();
    e.stopPropagation();
  }

  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeConsolePopup()}>
      <div class="popup se-popup sf-console-popup" role="dialog" aria-label="Run Console Command" onKeyDown={onKeyDown}>
        <div class="sf-console-title">
          <span>Run Console Command</span>
          <span class="muted">php bin/console</span>
        </div>
        <label class="se-search">
          <SquareTerminal size={15} class="muted" />
          <input
            ref={input}
            class="se-input"
            value={query()}
            placeholder="Command name, e.g. cache:clear --env=prod"
            spellcheck={false}
            onInput={(e) => setQuery(e.currentTarget.value)}
          />
        </label>
        <div class="se-results" ref={list}>
          <For each={shown()} fallback={<div class="se-empty">No matching commands. Press Enter to run “{parsed().command}”.</div>}>
            {(x, i) => (
              <div
                class="se-row"
                classList={{ selected: i() === index() }}
                data-index={i()}
                onMouseMove={() => setIndex(i())}
                onClick={() => run(i())}
              >
                <span class="se-icon">
                  <SquareTerminal size={14} class={x.c.builtin ? "muted" : "sf-icon-command"} />
                </span>
                <span class="se-label">
                  <Highlight text={x.c.name} positions={x.m!.positions} />
                </span>
                <span class="se-detail">{x.c.description}</span>
                <Show when={!x.c.builtin}>
                  <span class="se-hint">{x.c.class?.split("\\").pop()}</span>
                </Show>
              </div>
            )}
          </For>
        </div>
        <div class="se-footer">Enter to run · Tab to complete · arguments after the command name are passed on</div>
      </div>
    </div>
  );
}
