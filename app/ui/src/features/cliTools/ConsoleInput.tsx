/**
 * The input line of the Command Line Tools Console (`alias command args`), shared by the
 * Run Command popup and the tool window: completion of aliases, commands, options and
 * values as you type (computed in Rust), the help of the command at the cursor, the history
 * (Up/Down) and Ctrl+C to stop the running command. Keys are handled by the input field
 * itself like any text field; they are not IDE shortcuts.
 */
import { createEffect, createSignal, For, Match, on, onMount, Show, Switch } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ChevronRight, Flag, Folder, Hash, SquareTerminal, Wrench } from "lucide-solid";
import { cliToolsApi, type CliCompletion, type CliCompletionItem, type CommandHelp, type CompletionKind } from "@/ipc/cliTools";
import { history } from "./store";
import { t } from "@/i18n";

const KIND_ICONS: Record<CompletionKind, typeof Wrench> = {
  tool: Wrench,
  command: SquareTerminal,
  group: Folder,
  option: Flag,
  value: Hash,
};

export interface ConsoleInputProps {
  /** Initial text, e.g. `a ` from Run Artisan Command…. */
  initial?: string;
  onRun: (input: string) => void;
  /** Escape with no completion open. */
  onEscape?: () => void;
  /** Ctrl+C with nothing selected while a command runs. */
  onStop?: () => void;
  running?: boolean;
  /** Completion above the input (the tool window's input is at the bottom). */
  dropUp?: boolean;
  autofocus?: boolean;
  placeholder?: string;
  /** The input element, for focusing it from outside. */
  ref?: (el: HTMLInputElement) => void;
}

/** The help of the command being typed: usage, description, arguments and options. */
export function CommandHelpView(props: { help: CommandHelp }) {
  const cmd = () => props.help.command;
  const active = () => props.help.active;
  const own = () => (cmd().options ?? []).filter((o) => !o.global);
  return (
    <div class="cli-help" data-testid="cli-help">
      <div class="cli-help-title">
        <span class="cli-help-name">{cmd().name}</span>
        <span class="muted">{props.help.tool}</span>
      </div>
      <Show when={cmd().description}>
        <div class="cli-help-desc">{cmd().description}</div>
      </Show>
      <Show when={cmd().usage?.length}>
        <div class="cli-help-section">{t("tools.cliTools.usageLabel")}</div>
        <For each={cmd().usage!.slice(0, 2)}>{(u) => <code class="cli-help-usage">{u}</code>}</For>
      </Show>
      <Show when={cmd().arguments?.length}>
        <div class="cli-help-section">{t("common.argumentsLabel")}</div>
        <For each={cmd().arguments}>
          {(a) => (
            <div class="cli-help-row" classList={{ active: active()?.kind === "argument" && active()?.name === a.name }}>
              <code>{a.required ? a.name : `[${a.name}]`}</code>
              <span>{a.description}</span>
            </div>
          )}
        </For>
      </Show>
      <Show when={own().length}>
        <div class="cli-help-section">{t("tools.cliTools.optionsLabel")}</div>
        <For each={own().slice(0, 8)}>
          {(o) => (
            <div class="cli-help-row" classList={{ active: active()?.kind === "option" && active()?.name === o.name }}>
              <code>
                {o.shortcuts?.length ? `-${o.shortcuts[0]}, ` : ""}--{o.name}
                {o.valueRequired ? "=VALUE" : o.acceptValue ? "[=VALUE]" : ""}
              </code>
              <span>{o.description}</span>
            </div>
          )}
        </For>
        <Show when={own().length > 8}>
          <div class="cli-help-row muted">{t("tools.cliTools.more", { value: own().length - 8 })}</div>
        </Show>
      </Show>
    </div>
  );
}

export function ConsoleInput(props: ConsoleInputProps) {
  let inputEl!: HTMLInputElement;
  let listEl: HTMLDivElement | undefined;
  const [text, setText] = createSignal(props.initial ?? "");
  const [completion, setCompletion] = createSignal<CliCompletion | null>(null);
  const [open, setOpen] = createSignal(false);
  const [index, setIndex] = createSignal(0);
  /** Up/Down moved through the list: Enter accepts the item instead of running. */
  const [navigated, setNavigated] = createSignal(false);
  const [hovered, setHovered] = createSignal<CliCompletionItem | null>(null);
  /** Position in the history while browsing it with Up/Down (-1 = the typed text). */
  let historyPos = -1;
  let draft = "";
  let seq = 0;

  const items = () => completion()?.items ?? [];

  async function refresh(show = true): Promise<void> {
    const id = ++seq;
    const value = inputEl.value;
    const caret = inputEl.selectionStart ?? value.length;
    try {
      const c = await cliToolsApi.complete(value, caret);
      if (id !== seq) return;
      setCompletion(c);
      setIndex(0);
      setNavigated(false);
      setHovered(null);
      if (show) setOpen(c.items.length > 0 || !!c.help);
    } catch {
      if (id === seq) setCompletion(null);
    }
  }

  function setValue(value: string, caret = value.length): void {
    setText(value);
    inputEl.value = value;
    inputEl.setSelectionRange(caret, caret);
  }

  function accept(item: CliCompletionItem | undefined): void {
    const c = completion();
    if (!item || !c) return;
    const value = inputEl.value;
    let rest = value.slice(c.to);
    if (item.insert.endsWith(" ") && rest.startsWith(" ")) rest = rest.slice(1);
    const next = value.slice(0, c.from) + item.insert + rest;
    setValue(next, c.from + item.insert.length);
    void refresh();
  }

  function run(): void {
    const value = inputEl.value.trim();
    if (!value) return;
    setOpen(false);
    historyPos = -1;
    props.onRun(value);
    setValue("");
    void refresh(false);
  }

  function browseHistory(delta: number): void {
    const h = history();
    if (!h.length) return;
    if (historyPos === -1) draft = inputEl.value;
    historyPos = Math.max(-1, Math.min(h.length - 1, historyPos + delta));
    setValue(historyPos === -1 ? draft : h[historyPos]);
    setOpen(false);
  }

  function onKeyDown(e: KeyboardEvent): void {
    const n = items().length;
    const listOpen = open() && n > 0;
    if (e.key === "c" && (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey) {
      // Like a terminal: Ctrl+C without a selection stops the command.
      if (props.running && inputEl.selectionStart === inputEl.selectionEnd) {
        e.preventDefault();
        props.onStop?.();
      }
      return;
    }
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    switch (e.key) {
      case "ArrowDown":
      case "ArrowUp": {
        const up = e.key === "ArrowUp";
        // The key that moves into and along the list: Down below the input, Up above it.
        const along = props.dropUp ? up : !up;
        if (listOpen && (navigated() || along)) {
          if (!navigated()) {
            setIndex(0);
            setNavigated(true);
          } else if (along) setIndex((i) => Math.min(n - 1, i + 1));
          else if (index() === 0) setNavigated(false);
          else setIndex((i) => i - 1);
        } else browseHistory(up ? 1 : -1);
        break;
      }
      case "Tab":
        if (!listOpen) {
          void refresh();
          break;
        }
        accept(items()[index()]);
        break;
      case "Enter":
        if (listOpen && navigated()) accept(items()[index()]);
        else run();
        break;
      case "Escape":
        if (open()) setOpen(false);
        else props.onEscape?.();
        break;
      default:
        return;
    }
    e.preventDefault();
    e.stopPropagation();
  }

  onMount(() => {
    props.ref?.(inputEl);
    if (props.autofocus) {
      inputEl.focus();
      inputEl.setSelectionRange(inputEl.value.length, inputEl.value.length);
    }
    if (inputEl.value) void refresh(!!props.autofocus);
  });

  createEffect(
    on(index, (i) => listEl?.querySelector(`[data-index="${i}"]`)?.scrollIntoView({ block: "nearest" }), { defer: true }),
  );

  const doc = () => {
    const item = hovered() ?? (navigated() ? items()[index()] : null);
    if (item?.description && item.kind !== "command") return { item };
    const help = completion()?.help;
    if (help) return { help };
    return item?.description ? { item } : null;
  };

  return (
    <div class="cli-input" classList={{ "drop-up": !!props.dropUp }}>
      <div class="cli-input-row">
        <ChevronRight size={14} class="cli-prompt" />
        <input
          ref={inputEl}
          class="cli-input-field"
          value={text()}
          placeholder={props.placeholder ?? t("tools.cliTools.aliasCommandArgumentsOptions")}
          spellcheck={false}
          autocomplete="off"
          data-testid="cli-input"
          onInput={(e) => {
            setText(e.currentTarget.value);
            historyPos = -1;
            void refresh();
          }}
          onKeyDown={onKeyDown}
          onClick={() => void refresh()}
          onBlur={() => setTimeout(() => setOpen(false), 150)}
        />
      </div>
      <Show when={open() && (items().length || doc())}>
        <div class="cli-popup-list" onMouseDown={(e) => e.preventDefault()}>
          <Show when={items().length}>
            <div class="cli-items" ref={listEl} role="listbox">
              <For each={items()}>
                {(item, i) => (
                  <div
                    class="cli-item"
                    classList={{ selected: navigated() && index() === i() }}
                    role="option"
                    data-index={i()}
                    data-cli-item={item.label}
                    onMouseEnter={() => setHovered(item)}
                    onMouseLeave={() => setHovered(null)}
                    onClick={() => accept(item)}
                  >
                    <Dynamic component={KIND_ICONS[item.kind]} size={13} class={`cli-item-icon ${item.kind}`} />
                    <span class="cli-item-label">{item.label}</span>
                    <Show when={item.detail}>
                      <span class="cli-item-detail">{item.detail}</span>
                    </Show>
                    <span class="cli-item-desc">{item.description}</span>
                  </div>
                )}
              </For>
            </div>
          </Show>
          <Show when={doc()}>
            {(d) => (
              <Switch>
                <Match when={d().help}>{(h) => <CommandHelpView help={h()} />}</Match>
                <Match when={d().item}>
                  {(item) => (
                    <div class="cli-help">
                      <div class="cli-help-title">
                        <span class="cli-help-name">{item().label}</span>
                        <Show when={item().detail}>
                          <span class="muted">{item().detail}</span>
                        </Show>
                      </div>
                      <div class="cli-help-desc">{item().description}</div>
                    </div>
                  )}
                </Match>
              </Switch>
            )}
          </Show>
        </div>
      </Show>
      <Show when={!(open() && items().length) && completion()?.hint}>
        <div class="cli-hint" data-testid="cli-hint">
          {completion()!.hint}
        </div>
      </Show>
    </div>
  );
}
