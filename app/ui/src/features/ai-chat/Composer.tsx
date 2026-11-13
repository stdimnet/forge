import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { ArrowUp, AtSign, Square, TextSelect, X } from "lucide-solid";
import type { MessageContext } from "@/ipc/agent";
import { activeTab, editor } from "@/state/editor";
import { activeSelection, type EditorSelection } from "@/editor/views";
import { FileIcon } from "@/shell/FileIcon";
import { chat, interrupt, newChat, sendMessage } from "./store";
import { awaitingPermission } from "./model";
import { findChatAgent } from "./agents";
import { t as tr } from "@/i18n";

const [draft, setDraft] = createSignal("");
/** Prefill the composer (empty-state suggestions, commands). */
export const setComposerText = (text: string) => setDraft(text);

let focusComposer: () => void = () => undefined;
export const focusChatInput = () => focusComposer();

interface AttachOption {
  key: string;
  label: string;
  hint: string;
  file?: string;
  selection?: EditorSelection;
}

export function Composer() {
  let input!: HTMLTextAreaElement;
  const [files, setFiles] = createSignal<string[]>([]);
  const [selection, setSelection] = createSignal<EditorSelection | undefined>();
  const [menu, setMenu] = createSignal<"attach" | null>(null);
  const [menuIndex, setMenuIndex] = createSignal(0);

  const agent = () => findChatAgent(chat.agentId);
  const autosize = () => {
    input.style.height = "auto";
    input.style.height = `${Math.min(input.scrollHeight, 220)}px`;
  };
  onMount(() => {
    focusComposer = () => input.focus();
    autosize();
  });

  const attachOptions = createMemo<AttachOption[]>(() => {
    const opts: AttachOption[] = [];
    const sel = menu() ? activeSelection() : undefined;
    if (sel) opts.push({ key: "sel", label: tr("tools.aiChat.selection"), hint: `${sel.path.split("/").pop()}:${sel.startLine}-${sel.endLine}`, selection: sel });
    const current = activeTab();
    if (current) opts.push({ key: `f:${current.path}`, label: current.name, hint: tr("tools.aiChat.currentFile"), file: current.path });
    for (const t of editor.tabs)
      if (t.path !== current?.path) opts.push({ key: `f:${t.path}`, label: t.name, hint: t.path.split("/").slice(0, -1).join("/"), file: t.path });
    return opts.filter((o) => !o.file || !files().includes(o.file));
  });

  const slashHints = createMemo(() => {
    const d = draft();
    if (!d.startsWith("/") || d.includes(" ") || d.includes("\n")) return [];
    const cmds = [{ name: "clear", description: tr("tools.aiChat.startANewChat") }, ...(agent()?.slashCommands ?? [])];
    return cmds.filter((c) => c.name.startsWith(d.slice(1)));
  });

  const choose = (o: AttachOption) => {
    if (o.file) setFiles((f) => [...f, o.file!]);
    if (o.selection) setSelection(o.selection);
    // Drop the "@" that opened the menu.
    if (draft().endsWith("@")) setDraft(draft().slice(0, -1));
    setMenu(null);
    input.focus();
  };

  const submit = () => {
    const text = draft().trim();
    if (!text || chat.running) return;
    if (text === "/clear") {
      setDraft("");
      void newChat();
      return;
    }
    const context: MessageContext = { files: files(), selection: selection() };
    setDraft("");
    setFiles([]);
    setSelection(undefined);
    queueMicrotask(autosize);
    void sendMessage(text, context);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (menu()) {
      const n = attachOptions().length;
      if (e.key === "ArrowDown") return (e.preventDefault(), setMenuIndex((i) => (i + 1) % Math.max(n, 1)));
      if (e.key === "ArrowUp") return (e.preventDefault(), setMenuIndex((i) => (i - 1 + n) % Math.max(n, 1)));
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        const o = attachOptions()[menuIndex()];
        if (o) choose(o);
        else setMenu(null);
        return;
      }
      if (e.key === "Escape") return (e.preventDefault(), setMenu(null));
    }
    if (slashHints().length && e.key === "Tab") {
      e.preventDefault();
      setDraft(`/${slashHints()[0].name} `);
      return;
    }
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      submit();
    }
    if (e.key === "Escape" && chat.running) void interrupt();
  };

  const onInput = (e: InputEvent & { currentTarget: HTMLTextAreaElement }) => {
    const v = e.currentTarget.value;
    setDraft(v);
    autosize();
    const before = v.slice(0, e.currentTarget.selectionStart ?? v.length);
    if (e.data === "@" && /(^|\s)@$/.test(before)) {
      setMenuIndex(0);
      setMenu("attach");
    } else if (menu() && !before.endsWith("@")) setMenu(null);
  };

  return (
    <div class="chat-composer-wrap">
      <Show when={slashHints().length}>
        <div class="chat-popup chat-slash">
          <For each={slashHints()}>
            {(c) => (
              <button
                class="chat-popup-item"
                onMouseDown={(e) => {
                  e.preventDefault();
                  setDraft(`/${c.name} `);
                  input.focus();
                }}
              >
                <span class="chat-popup-label">/{c.name}</span>
                <span class="chat-popup-hint">{c.description}</span>
              </button>
            )}
          </For>
        </div>
      </Show>
      <Show when={menu()}>
        <div class="chat-popup chat-attach-menu">
          <div class="chat-popup-title">{tr("tools.aiChat.attachContext")}</div>
          <For each={attachOptions()} fallback={<div class="chat-popup-empty">{tr("tools.aiChat.openAFileToAttach")}</div>}>
            {(o, i) => (
              <button
                class="chat-popup-item"
                classList={{ active: i() === menuIndex() }}
                onMouseEnter={() => setMenuIndex(i())}
                onMouseDown={(e) => {
                  e.preventDefault();
                  choose(o);
                }}
              >
                {o.selection ? <TextSelect size={13} /> : <FileIcon name={o.label} size={13} />}
                <span class="chat-popup-label">{o.label}</span>
                <span class="chat-popup-hint">{o.hint}</span>
              </button>
            )}
          </For>
        </div>
      </Show>
      <div class="chat-composer" classList={{ running: chat.running }} onClick={() => input.focus()}>
        <Show when={files().length || selection()}>
          <div class="chat-composer-chips">
            <For each={files()}>
              {(f) => (
                <span class="chat-context-chip" title={f}>
                  <FileIcon name={f.split("/").pop() ?? f} size={12} />
                  {f.split("/").pop()}
                  <button title={tr("common.remove")} onClick={() => setFiles((x) => x.filter((p) => p !== f))}>
                    <X size={11} />
                  </button>
                </span>
              )}
            </For>
            <Show when={selection()}>
              {(s) => (
                <span class="chat-context-chip" title={s().text}>
                  <TextSelect size={12} />
                  {s().path.split("/").pop()}:{s().startLine}-{s().endLine}
                  <button title={tr("common.remove")} onClick={() => setSelection(undefined)}>
                    <X size={11} />
                  </button>
                </span>
              )}
            </Show>
          </div>
        </Show>
        <textarea
          ref={input}
          class="chat-input"
          rows={1}
          value={draft()}
          placeholder={chat.running ? (awaitingPermission(chat.conv) ? tr("tools.aiChat.isWaitingForYourAnswerEllipsis", { value: agent()?.name ?? "The agent" }) : tr("tools.aiChat.isWorkingEllipsis", { value: agent()?.name ?? "The agent" })) : tr("tools.aiChat.askToAddContextFor", { value: agent()?.name ?? "the agent" })}
          onInput={onInput}
          onKeyDown={onKeyDown}
          onBlur={() => setTimeout(() => setMenu(null), 120)}
        />
        <div class="chat-composer-bar">
          <button
            class="chat-mini-btn"
            title={tr("tools.aiChat.attachCurrentFileOrSelection")}
            onMouseDown={(e) => e.preventDefault()}
            onClick={(e) => {
              e.stopPropagation();
              setMenuIndex(0);
              setMenu(menu() ? null : "attach");
              input.focus();
            }}
          >
            <AtSign size={13} />
            <span>{tr("tools.aiChat.context")}</span>
          </button>
          <span class="chat-flex" />
          <span class="chat-composer-hint">{chat.running ? tr("tools.aiChat.escToStop") : tr("tools.aiChat.sendNewline")}</span>
          <Show
            when={chat.running}
            fallback={
              <button class="chat-send" title={tr("tools.aiChat.sendEnter")} disabled={!draft().trim()} onClick={(e) => (e.stopPropagation(), submit())}>
                <ArrowUp size={15} stroke-width={2.5} />
              </button>
            }
          >
            <button class="chat-send stop" title={tr("tools.aiChat.stopEsc")} onClick={(e) => (e.stopPropagation(), void interrupt())}>
              <Square size={11} fill="currentColor" />
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}
