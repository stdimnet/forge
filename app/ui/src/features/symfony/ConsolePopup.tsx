/**
 * Run Console Command popup (Tools | Symfony | Run Console Command…): a speed-search list of
 * `bin/console` commands like PHPStorm's Run Anything. Text after the command name is passed
 * on as arguments.
 */
import { createEffect, createMemo, createSignal, For, on, onMount, Show } from "solid-js";
import { SquareTerminal } from "lucide-solid";
import { createFuzzyFilter } from "@/features/search/fuzzyFilter";
import { Highlight } from "@/features/search/SearchEverywhere";
import { commands, loadSymfony } from "./store";
import { runConsoleCommand, splitCommandLine } from "./console";
import { t } from "@/i18n";

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
  // Project commands before built-in ones; the Rust ranking keeps that order on ties.
  const pool = createMemo(() =>
    commands()
      .filter((c) => !c.hidden)
      .sort((a, b) => Number(!!a.builtin) - Number(!!b.builtin)),
  );
  const hits = createFuzzyFilter(() => ({
    query: parsed().command,
    candidates: pool().map((c) => [c.name, ...(c.aliases ?? [])]),
  }));
  const shown = createMemo(() =>
    hits()
      .filter((h) => h.index < pool().length)
      // Highlight only when the name itself (not an alias) matched.
      .map((h) => ({ c: pool()[h.index], m: { score: h.score, positions: h.text === 0 ? h.positions : [] } })),
  );
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
      <div class="popup se-popup sf-console-popup" role="dialog" aria-label={t("langs.symfony.runConsoleCommand")} onKeyDown={onKeyDown}>
        <div class="sf-console-title">
          <span>{t("langs.symfony.runConsoleCommand")}</span>
          <span class="muted">{t("langs.symfony.phpBinConsole")}</span>
        </div>
        <label class="se-search">
          <SquareTerminal size={15} class="muted" />
          <input
            ref={input}
            class="se-input"
            value={query()}
            placeholder={t("langs.symfony.commandNameEGCache")}
            spellcheck={false}
            onInput={(e) => setQuery(e.currentTarget.value)}
          />
        </label>
        <div class="se-results" ref={list}>
          <For each={shown()} fallback={<div class="se-empty">{t("langs.symfony.noMatchingCommandsPressEnter", { command: parsed().command })}</div>}>
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
        <div class="se-footer">{t("langs.symfony.enterToRunTabTo")}</div>
      </div>
    </div>
  );
}
