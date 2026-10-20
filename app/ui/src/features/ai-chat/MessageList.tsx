import { createEffect, createSignal, For, Match, Show, Switch } from "solid-js";
import { Brain, ChevronRight, CircleStop, FileText, TextSelect, TriangleAlert } from "lucide-solid";
import { FileIcon } from "@/shell/FileIcon";
import { openFile } from "@/state/editor";
import { chat } from "./store";
import { changedFilesOf, formatDuration, formatTokens, type Block, type Turn } from "./model";
import { Markdown } from "./Markdown";
import { ToolCard } from "./ToolCard";
import { PermissionCard } from "./PermissionCard";

function Thinking(props: { block: Block & { kind: "thinking" } }) {
  const [open, setOpen] = createSignal(false);
  const label = () =>
    props.block.streaming
      ? "Thinking…"
      : props.block.durationMs
        ? `Thought for ${Math.max(1, Math.round(props.block.durationMs / 1000))}s`
        : "Thought";
  return (
    <div class="chat-thinking" classList={{ open: open(), streaming: props.block.streaming }}>
      <button class="chat-thinking-toggle" onClick={() => setOpen(!open())}>
        <Brain size={13} />
        <span>{label()}</span>
        <ChevronRight size={12} class="chat-tool-chevron" />
      </button>
      <Show when={open()}>
        <div class="chat-thinking-text">{props.block.text}</div>
      </Show>
    </div>
  );
}

function UserMessage(props: { turn: Turn }) {
  return (
    <div class="chat-user">
      <div class="chat-user-bubble">
        <div class="chat-user-text">{props.turn.prompt}</div>
        <Show when={props.turn.context.files.length || props.turn.context.selection}>
          <div class="chat-user-context">
            <For each={props.turn.context.files}>
              {(f) => (
                <span class="chat-context-chip static">
                  <FileIcon name={f.split("/").pop() ?? f} size={12} />
                  {f.split("/").pop()}
                </span>
              )}
            </For>
            <Show when={props.turn.context.selection}>
              {(s) => (
                <span class="chat-context-chip static">
                  <TextSelect size={12} />
                  {s().path.split("/").pop()}:{s().startLine}-{s().endLine}
                </span>
              )}
            </Show>
          </div>
        </Show>
      </div>
    </div>
  );
}

function TurnFooter(props: { turn: Turn }) {
  const files = () => changedFilesOf(props.turn, chat.conv.cwd);
  const s = () => props.turn.summary;
  const tokens = () => {
    const u = s()?.usage;
    return u ? u.inputTokens + u.outputTokens + (u.cacheReadTokens ?? 0) + (u.cacheWriteTokens ?? 0) : 0;
  };
  return (
    <>
      <Show when={props.turn.status !== "running" && files().length}>
        <div class="chat-changed">
          <div class="chat-changed-title">
            <FileText size={13} />
            {files().length} file{files().length === 1 ? "" : "s"} changed
          </div>
          <For each={files()}>
            {(f) => (
              <button class="chat-changed-file" onClick={() => void openFile(f).catch(() => undefined)}>
                <FileIcon name={f.split("/").pop() ?? f} size={13} />
                <span class="vcs-modified">{f.split("/").pop()}</span>
                <span class="muted chat-ellipsis">{f.split("/").slice(0, -1).join("/")}</span>
              </button>
            )}
          </For>
        </div>
      </Show>
      <Show when={props.turn.status === "interrupted"}>
        <div class="chat-turn-note">
          <CircleStop size={13} /> Interrupted
        </div>
      </Show>
      <Show when={s()}>
        <div class="chat-turn-footer">
          <Show when={s()!.durationMs != null}>
            <span>{formatDuration(s()!.durationMs!)}</span>
          </Show>
          <Show when={tokens()}>
            <span>{formatTokens(tokens())} tokens</span>
          </Show>
          <Show when={s()!.costUsd != null}>
            <span>${s()!.costUsd!.toFixed(s()!.costUsd! < 0.1 ? 3 : 2)}</span>
          </Show>
        </div>
      </Show>
    </>
  );
}

function BlockView(props: { block: Block; superseded: boolean }) {
  const b = props.block;
  return (
    <Switch>
      <Match when={b.kind === "text" && b}>{(t) => <Markdown text={t().text} streaming={t().streaming} />}</Match>
      <Match when={b.kind === "thinking" && b}>{(t) => <Thinking block={t()} />}</Match>
      <Match when={b.kind === "tool" && b}>{(t) => <ToolCard block={t()} cwd={chat.conv.cwd} superseded={props.superseded} />}</Match>
      <Match when={b.kind === "error" && b}>
        {(e) => (
          <div class="chat-error">
            <TriangleAlert size={14} />
            <span>{e().message}</span>
          </div>
        )}
      </Match>
      <Match when={b.kind === "permission" && b}>{(p) => <PermissionCard block={p()} />}</Match>
    </Switch>
  );
}

const lastTodoIndex = (turn: Turn) => {
  for (let i = turn.blocks.length - 1; i >= 0; i--) {
    const b = turn.blocks[i];
    if (b.kind === "tool" && b.name === "TodoWrite") return i;
  }
  return -1;
};

export function MessageList() {
  let scroller!: HTMLDivElement;
  let stick = true;
  // Follow the stream while the user is at the bottom.
  createEffect(() => {
    JSON.stringify(chat.conv.turns.length && chat.conv.turns[chat.conv.turns.length - 1]);
    if (stick) queueMicrotask(() => (scroller.scrollTop = scroller.scrollHeight));
  });
  return (
    <div
      class="chat-messages"
      ref={scroller}
      onScroll={() => (stick = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 40)}
    >
      <For each={chat.conv.turns}>
        {(turn) => (
          <div class="chat-turn" data-status={turn.status}>
            <UserMessage turn={turn} />
            <div class="chat-assistant">
              <For each={turn.blocks}>{(b, i) => <BlockView block={b} superseded={i() < lastTodoIndex(turn)} />}</For>
              <Show
                when={
                  turn.status === "running" &&
                  !turn.blocks.some((b) => (b.kind === "text" && b.streaming) || (b.kind === "permission" && b.status === "pending"))
                }
              >
                <div class="chat-working">
                  <span class="chat-dots">
                    <i />
                    <i />
                    <i />
                  </span>
                  Working…
                </div>
              </Show>
              <TurnFooter turn={turn} />
            </div>
          </div>
        )}
      </For>
    </div>
  );
}
