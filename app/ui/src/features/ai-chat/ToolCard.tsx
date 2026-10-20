import { createSignal, For, Show, type Component, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import {
  Bot,
  Check,
  ChevronRight,
  Circle,
  CircleCheck,
  CircleDot,
  FileDiff,
  FilePen,
  FilePlus,
  FileText,
  FolderSearch,
  Globe,
  ListTodo,
  LoaderCircle,
  Search,
  SquareTerminal,
  Wrench,
  X,
} from "lucide-solid";
import { openFile } from "@/state/editor";
import { FileIcon } from "@/shell/FileIcon";
import type { ToolBlock } from "./model";
import { DiffDialog, DiffView } from "./DiffView";

type Icon = Component<{ size?: number; "stroke-width"?: number }>;

interface ToolView {
  icon: Icon;
  verb: string;
  subject?: (b: ToolBlock) => JSX.Element;
  meta?: (b: ToolBlock) => JSX.Element;
  body?: Component<{ block: ToolBlock }>;
  /** Body always visible (no collapse), e.g. todo lists. */
  inline?: boolean;
}

const str = (v: unknown) => (typeof v === "string" ? v : "");
const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/** A workspace-relative file that opens on click. */
export function FileChip(props: { path: string }) {
  const rel = () => props.path;
  const name = () => rel().split("/").pop() || rel();
  return (
    <button
      class="chat-file-chip"
      title={`Open ${rel()}`}
      onClick={(e) => {
        e.stopPropagation();
        void openFile(rel()).catch(() => undefined);
      }}
    >
      <FileIcon name={name()} size={13} />
      <span>{name()}</span>
    </button>
  );
}

function Output(props: { text: string; lines?: number; error?: boolean }) {
  const [all, setAll] = createSignal(false);
  const lines = () => props.text.replace(/\s+$/, "").split("\n");
  const max = () => props.lines ?? 12;
  return (
    <div class="chat-tool-output" classList={{ error: props.error }}>
      <pre>{(all() ? lines() : lines().slice(0, max())).join("\n") || "(no output)"}</pre>
      <Show when={!all() && lines().length > max()}>
        <button class="chat-link-btn" onClick={() => setAll(true)}>
          Show all {lines().length} lines
        </button>
      </Show>
    </div>
  );
}

function ResultBody(props: { block: ToolBlock }) {
  return (
    <Show when={props.block.result !== undefined} fallback={<div class="chat-tool-pending">Waiting for result…</div>}>
      <Output text={props.block.result ?? ""} error={props.block.status === "error"} />
    </Show>
  );
}

const noDiff = { hunks: [], added: 0, removed: 0 };

function EditBody(props: { block: ToolBlock }) {
  const [open, setOpen] = createSignal(false);
  const path = () => props.block.path ?? "";
  const diff = () => props.block.diff ?? noDiff;
  return (
    <>
      <DiffView diff={diff()} path={path()} maxLines={16} />
      <Show when={props.block.status === "error" && props.block.result}>
        <Output text={props.block.result!} error lines={4} />
      </Show>
      <div class="chat-tool-actions">
        <button class="chat-mini-btn" onClick={() => setOpen(true)}>
          <FileDiff size={13} />
          <span>Open diff</span>
        </button>
        <button class="chat-mini-btn" onClick={() => void openFile(path()).catch(() => undefined)}>
          <FileText size={13} />
          <span>Open file</span>
        </button>
      </div>
      <Show when={open()}>
        <DiffDialog path={path()} diff={diff()} onClose={() => setOpen(false)} />
      </Show>
    </>
  );
}

interface Todo {
  content: string;
  status: "pending" | "in_progress" | "completed";
  activeForm?: string;
}
const todosOf = (b: ToolBlock): Todo[] => (Array.isArray(b.input.todos) ? (b.input.todos as Todo[]) : []);

function TodoBody(props: { block: ToolBlock }) {
  return (
    <ul class="chat-todos">
      <For each={todosOf(props.block)}>
        {(t) => (
          <li class={`chat-todo ${t.status}`}>
            <span class="chat-todo-icon">
              {t.status === "completed" ? <CircleCheck size={14} /> : t.status === "in_progress" ? <CircleDot size={14} /> : <Circle size={14} />}
            </span>
            <span>{t.status === "in_progress" && t.activeForm ? t.activeForm : t.content}</span>
          </li>
        )}
      </For>
    </ul>
  );
}

function TaskBody(props: { block: ToolBlock }) {
  return (
    <div class="chat-subtools">
      <For each={props.block.children}>
        {(c) => (
          <div class="chat-subtool">
            <StatusIcon status={c.status} />
            <span class="chat-subtool-name">{c.name}</span>
            <span class="muted chat-ellipsis">{c.subject}</span>
          </div>
        )}
      </For>
      <Show when={props.block.result}>
        <Output text={props.block.result!} lines={6} />
      </Show>
    </div>
  );
}

const fileSubject = (b: ToolBlock) => (b.path !== undefined ? <FileChip path={b.path} /> : null);

const editMeta = (b: ToolBlock) => {
  const s = b.diff ?? noDiff;
  return (
    <span class="chat-diffstat">
      <span class="add">+{s.added}</span>
      <Show when={s.removed}>
        <span class="del">−{s.removed}</span>
      </Show>
    </span>
  );
};

const VIEWS: Record<string, ToolView> = {
  Read: {
    icon: FileText,
    verb: "Read",
    subject: fileSubject,
    meta: (b) => (b.result !== undefined && b.status === "done" ? plural(b.resultLines, "line") : null),
    body: ResultBody,
  },
  Edit: { icon: FilePen, verb: "Edit", subject: fileSubject, meta: editMeta, body: EditBody },
  MultiEdit: { icon: FilePen, verb: "Edit", subject: fileSubject, meta: editMeta, body: EditBody },
  Write: { icon: FilePlus, verb: "Write", subject: fileSubject, meta: editMeta, body: EditBody },
  Bash: {
    icon: SquareTerminal,
    verb: "",
    subject: (b) => <code class="chat-tool-cmd">$ {str(b.input.command)}</code>,
    body: ResultBody,
  },
  Grep: {
    icon: Search,
    verb: "Grep",
    subject: (b) => <code class="chat-tool-cmd">{str(b.input.pattern)}</code>,
    meta: (b) => (b.result !== undefined ? plural(b.resultLines, "result") : null),
    body: ResultBody,
  },
  Glob: {
    icon: FolderSearch,
    verb: "Glob",
    subject: (b) => <code class="chat-tool-cmd">{str(b.input.pattern)}</code>,
    meta: (b) => (b.result !== undefined ? plural(b.resultLines, "file") : null),
    body: ResultBody,
  },
  TodoWrite: {
    icon: ListTodo,
    verb: "Update todos",
    meta: (b) => {
      const t = todosOf(b);
      return `${t.filter((x) => x.status === "completed").length}/${t.length} done`;
    },
    body: TodoBody,
    inline: true,
  },
  WebFetch: {
    icon: Globe,
    verb: "Fetch",
    subject: (b) => <span class="chat-ellipsis">{str(b.input.url).replace(/^https?:\/\//, "")}</span>,
    body: ResultBody,
  },
  WebSearch: {
    icon: Globe,
    verb: "Search web",
    subject: (b) => <span class="chat-ellipsis">“{str(b.input.query)}”</span>,
    body: ResultBody,
  },
  Task: {
    icon: Bot,
    verb: "Agent",
    subject: (b) => <span class="chat-ellipsis">{str(b.input.description)}</span>,
    meta: (b) => (b.children.length ? plural(b.children.length, "tool use") : null),
    body: TaskBody,
  },
};

const fallbackView = (name: string): ToolView => ({
  icon: Wrench,
  verb: name.replace(/^mcp__/, "").replace(/__/g, " · "),
  subject: (b) => {
    const first = Object.values(b.input).find((v) => typeof v === "string") as string | undefined;
    return first ? <span class="chat-ellipsis muted">{first}</span> : null;
  },
  body: ResultBody,
});

function StatusIcon(props: { status: ToolBlock["status"] }) {
  return (
    <span class={`chat-tool-status ${props.status}`}>
      {props.status === "running" ? <LoaderCircle size={13} class="spin" /> : props.status === "error" ? <X size={13} /> : <Check size={13} />}
    </span>
  );
}

export function ToolCard(props: { block: ToolBlock; defaultOpen?: boolean }) {
  const view = () => {
    const v = VIEWS[props.block.name] ?? fallbackView(props.block.name);
    // Older todo lists collapse once a newer one exists, like a live checklist.
    return props.block.superseded && v.inline ? { ...v, inline: false } : v;
  };
  const [open, setOpen] = createSignal(props.defaultOpen ?? false);
  const expanded = () => view().inline || open();
  return (
    <div
      class={`chat-tool-card ${props.block.status}`}
      classList={{ open: expanded() }}
      data-tool={props.block.name}
    >
      <div
        class="chat-tool-header"
        role="button"
        tabIndex={0}
        aria-expanded={expanded()}
        onClick={() => !view().inline && setOpen(!open())}
        onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && !view().inline && setOpen(!open())}
      >
        <Show when={!view().inline} fallback={<span class="chat-tool-chevron" />}>
          <ChevronRight size={13} class="chat-tool-chevron" />
        </Show>
        <span class="chat-tool-icon">
          <Dynamic component={view().icon} size={14} />
        </span>
        <Show when={view().verb}>
          <span class="chat-tool-verb">{view().verb}</span>
        </Show>
        <span class="chat-tool-subject">{view().subject?.(props.block)}</span>
        <span class="chat-tool-meta">{view().meta?.(props.block)}</span>
        <StatusIcon status={props.block.status} />
      </div>
      <Show when={expanded() && view().body}>
        <div class="chat-tool-body">
          <Dynamic component={view().body} block={props.block} />
        </div>
      </Show>
    </div>
  );
}
