/**
 * Run Artisan Command… (Ctrl+Shift+X): a popup with the project's artisan commands and
 * speed search; Enter runs the typed command line and the popup shows its output live,
 * with Stop and Rerun, like Laravel Idea's artisan runner.
 */
import { createEffect, createMemo, createSignal, For, on, onCleanup, Show } from "solid-js";
import { CircleStop, RotateCcw, SquareTerminal, X } from "lucide-solid";
import { laravelApi, type ArtisanCommand, type ArtisanEvent } from "@/ipc/laravel";
import { shortcutLabel } from "@/keymap";

type Line = { text: string; stderr: boolean };

const [open, setOpen] = createSignal(false);
const [mode, setMode] = createSignal<"pick" | "run">("pick");
const [input, setInput] = createSignal("");
const [commands, setCommands] = createSignal<ArtisanCommand[]>([]);
const [fromProject, setFromProject] = createSignal(true);
const [index, setIndex] = createSignal(0);
const [output, setOutput] = createSignal<Line[]>([]);
const [running, setRunning] = createSignal<string | null>(null);
const [commandLine, setCommandLine] = createSignal("");
const [exitCode, setExitCode] = createSignal<number | null>(null);
const [recent, setRecent] = createSignal<string[]>([]);

export { open as artisanOpen };

let loaded = false;

export function openArtisanPopup(prefill = ""): void {
  setMode("pick");
  setInput(prefill);
  setIndex(0);
  setOpen(true);
  if (!loaded) {
    loaded = true;
    void laravelApi
      .artisanCommands()
      .then((r) => {
        setCommands(r.commands);
        setFromProject(r.fromProject);
      })
      .catch(() => (loaded = false));
  }
}

export function closeArtisanPopup(): void {
  setOpen(false);
}

/** Commands matching the first word of the input: prefix, then substring, then description. */
export function filterCommands(all: ArtisanCommand[], text: string): ArtisanCommand[] {
  const q = text.trim().split(/\s+/)[0]?.toLowerCase() ?? "";
  if (!q) return all;
  const score = (c: ArtisanCommand) => {
    const n = c.name.toLowerCase();
    if (n === q) return 0;
    if (n.startsWith(q)) return 1;
    if (n.split(":").some((p) => p.startsWith(q))) return 2;
    if (n.includes(q)) return 3;
    // Abbreviations like `m:m` for make:model.
    const parts = q.split(":");
    if (parts.length > 1 && n.split(":").length === parts.length && n.split(":").every((p, i) => p.startsWith(parts[i]))) return 2;
    if (c.description.toLowerCase().includes(q)) return 4;
    return 9;
  };
  return all
    .map((c) => ({ c, s: score(c) }))
    .filter((x) => x.s < 9)
    .sort((a, b) => a.s - b.s || a.c.name.localeCompare(b.c.name))
    .map((x) => x.c);
}

let runSeq = 0;
let currentRun: string | null = null;

async function run(line: string): Promise<void> {
  const text = line.trim();
  if (!text) return;
  setRecent((r) => [text, ...r.filter((x) => x !== text)].slice(0, 8));
  setMode("run");
  setOutput([]);
  setExitCode(null);
  setCommandLine(`php artisan ${text}`);
  const id = `artisan-${Date.now()}-${++runSeq}`;
  currentRun = id;
  setRunning(id);
  const off = await laravelApi.onArtisan(id, (e: ArtisanEvent) => {
    if (currentRun !== id) return;
    if (e.type === "started") setCommandLine(e.commandLine);
    else if (e.type === "output") setOutput((o) => [...o, { text: e.text, stderr: e.stderr }]);
    else if (e.type === "error") setOutput((o) => [...o, { text: e.message, stderr: true }]);
    else if (e.type === "exit") setExitCode(e.code);
  });
  try {
    const code = await laravelApi.artisanRun(text, id);
    if (currentRun === id) setExitCode(code);
  } catch (e) {
    if (currentRun === id) {
      setOutput((o) => [...o, { text: String(e), stderr: true }]);
      setExitCode(-1);
    }
  } finally {
    off();
    if (currentRun === id) setRunning(null);
  }
}

function stop() {
  const id = running();
  if (id) void laravelApi.artisanCancel(id);
}

/** Argument placeholders for the list (`<name>`). */
const argsHint = (c: ArtisanCommand) => c.arguments.map((a) => `<${a}>`).join(" ");

export function ArtisanPopup() {
  let inputEl!: HTMLInputElement;
  let popupEl!: HTMLDivElement;
  let outputEl!: HTMLDivElement;
  const recentCommands = createMemo(() => (input().trim() ? [] : recent()));
  const matches = createMemo(() => filterCommands(commands(), input()));
  const rows = createMemo(() => [
    ...recentCommands().map((line) => ({ kind: "recent" as const, line })),
    ...matches().map((c) => ({ kind: "command" as const, line: c.name, cmd: c })),
  ]);

  createEffect(
    on([open, mode], () => {
      if (!open()) return;
      queueMicrotask(() => (mode() === "pick" ? inputEl : popupEl)?.focus());
    }),
  );
  createEffect(on(output, () => {
    if (outputEl) outputEl.scrollTop = outputEl.scrollHeight;
  }));

  const choose = (i: number) => {
    const row = rows()[i];
    if (!row) return;
    if (row.kind === "recent") void run(row.line);
    else {
      // "make:model Order" runs as typed; "mig" (or "m:m Order") runs the selected command.
      const [first, ...rest] = input().trim().split(/\s+/);
      void run(first === row.cmd.name ? input() : [row.cmd.name, ...rest].join(" "));
    }
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      closeArtisanPopup();
      return;
    }
    if (mode() === "run") {
      if (e.key === "Enter" && !running()) {
        e.preventDefault();
        setMode("pick");
      }
      return;
    }
    const n = rows().length;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setIndex((i) => (n ? (i + 1) % n : 0));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setIndex((i) => (n ? (i - 1 + n) % n : 0));
    } else if (e.key === "Tab") {
      e.preventDefault();
      const row = rows()[index()];
      if (row) setInput(`${row.line} `);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (!n) void run(input());
      else choose(index());
    }
  };

  onCleanup(() => stop());

  return (
    <Show when={open()}>
      <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeArtisanPopup()}>
        <div ref={popupEl} class="popup artisan-popup" classList={{ running: mode() === "run" }} tabIndex={-1} onKeyDown={onKeyDown}>
          <div class="artisan-header">
            <SquareTerminal size={15} class="artisan-header-icon" />
            <span class="artisan-title">Run Artisan Command</span>
            <Show when={mode() === "pick" && !fromProject()}>
              <span class="artisan-note" title="php artisan list could not run; showing Laravel's standard commands">
                built-in list
              </span>
            </Show>
            <span class="kbd artisan-shortcut">{shortcutLabel("Laravel.RunArtisan")}</span>
            <button class="icon-btn" title="Close (Esc)" onClick={closeArtisanPopup}>
              <X size={14} />
            </button>
          </div>
          <Show
            when={mode() === "run"}
            fallback={
              <>
                <div class="se-search artisan-search">
                  <span class="artisan-prompt">php artisan</span>
                  <input
                    ref={inputEl}
                    class="se-input"
                    value={input()}
                    placeholder="command [arguments] [--options]"
                    spellcheck={false}
                    onInput={(e) => {
                      setInput(e.currentTarget.value);
                      setIndex(0);
                    }}
                  />
                </div>
                <div class="se-results artisan-results">
                  <Show when={recentCommands().length}>
                    <div class="se-group">Recently run</div>
                  </Show>
                  <For each={rows()}>
                    {(row, i) => (
                      <>
                        <Show when={row.kind === "command" && i() === recentCommands().length && recentCommands().length}>
                          <div class="se-group">Commands</div>
                        </Show>
                        <div
                          class="se-row artisan-row"
                          classList={{ selected: index() === i() }}
                          data-artisan={row.line}
                          onMouseEnter={() => setIndex(i())}
                          onClick={() => choose(i())}
                        >
                          <span class="artisan-name">{row.line}</span>
                          <Show when={row.kind === "command" && row.cmd}>
                            {(c) => (
                              <>
                                <span class="artisan-args">{argsHint(c())}</span>
                                <span class="se-detail artisan-desc">{c().description}</span>
                              </>
                            )}
                          </Show>
                        </div>
                      </>
                    )}
                  </For>
                  <Show when={!rows().length}>
                    <div class="se-empty">
                      <Show when={commands().length} fallback="Loading commands…">
                        Press Enter to run <code>php artisan {input().trim()}</code>
                      </Show>
                    </div>
                  </Show>
                </div>
                <div class="artisan-footer muted">
                  <span>↑↓ to select · Tab to complete · Enter to run</span>
                </div>
              </>
            }
          >
            <div class="artisan-run-bar">
              <code class="artisan-cmdline">{commandLine()}</code>
              <span
                class="artisan-status"
                classList={{ ok: exitCode() === 0, failed: exitCode() !== null && exitCode() !== 0 }}
              >
                <Show when={running()} fallback={exitCode() === 0 ? "Finished" : exitCode() === -1 ? "Stopped" : `Exit code ${exitCode()}`}>
                  <span class="se-spinner" /> Running…
                </Show>
              </span>
              <Show
                when={running()}
                fallback={
                  <button class="icon-btn" title="Rerun" onClick={() => void run(commandLine().replace(/^php artisan /, ""))}>
                    <RotateCcw size={14} />
                  </button>
                }
              >
                <button class="icon-btn" title="Stop" onClick={stop}>
                  <CircleStop size={14} />
                </button>
              </Show>
            </div>
            <div class="artisan-output" ref={outputEl} tabIndex={0}>
              <For each={output()}>{(l) => <div class="artisan-line" classList={{ stderr: l.stderr }}>{l.text || " "}</div>}</For>
            </div>
            <div class="artisan-footer muted">
              <span>Enter for another command · Esc to close</span>
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
}
