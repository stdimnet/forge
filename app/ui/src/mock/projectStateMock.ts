/**
 * Browser-mode per-project state: the workbench layout and the AI chat history live in
 * memory for the page's lifetime (the desktop app keeps them in Forge's state and data
 * dirs). Two older chats are seeded so the history popup has entries in the demo project.
 */
import { registerMock } from "@/ipc";
import type { ChatRecord, ChatSummary, NamedLayout, ProjectLayout } from "@/ipc/projectState";
import type { SettingValue } from "@/ipc/settings";
import { mockSettingValue, setSetting } from "./settingsMock";

let layout: ProjectLayout | null = null;
let defaultLayout: ProjectLayout | null = null;
const chats = new Map<string, ChatRecord>();

const paragraph = (text: string) => [{ t: "paragraph", inline: [{ t: "text", text }] }];

function seedChat(id: string, minutesAgo: number, prompt: string, answer: string): void {
  chats.set(id, {
    id,
    agentId: "claude-code",
    at: Date.now() - minutesAgo * 60_000,
    title: "",
    conversation: {
      conversationId: `demo-${id}`,
      model: "claude-sonnet-4-5",
      cwd: null,
      turns: [
        {
          id: 1,
          prompt,
          context: { files: [] },
          blocks: [{ kind: "text", text: answer, streaming: false, markdown: paragraph(answer) }],
          status: "done",
          changedFiles: [],
          awaitingPermission: false,
          working: false,
        },
      ],
    },
  });
}

seedChat(
  "demo-orders",
  95,
  "Why does OrderController@store validate twice?",
  "StoreOrderRequest already validates the payload; the extra $request->validate() call in store() repeats the same rules and can go.",
);
seedChat(
  "demo-invoices",
  60 * 26,
  "Add a test for the invoice total with discounts",
  "I added tests/Feature/InvoiceTotalTest.php covering percentage and fixed discounts.",
);

/** Like `forge_core::project_state::chat_title`: the first non-empty line of the first prompt. */
function titleOf(record: ChatRecord): string {
  if (record.title.trim()) return record.title;
  const turns = (record.conversation as { turns?: { prompt?: string }[] }).turns ?? [];
  const line = (turns[0]?.prompt ?? "").split("\n").find((l) => l.trim()) ?? "";
  return line.trim().slice(0, 80) || "New chat";
}

function summary(record: ChatRecord): ChatSummary {
  const turns = (record.conversation as { turns?: unknown[] }).turns?.length ?? 0;
  return { id: record.id, agentId: record.agentId, at: record.at, title: titleOf(record), turns };
}

registerMock("layout_load", () => (layout ? structuredClone(layout) : null));
registerMock("layout_save", ({ layout: next }) => {
  layout = structuredClone(next as ProjectLayout);
});

// Like `forge_core::project_state::save_default_layout`: only the tool window part is kept.
registerMock("layout_default_load", () => (defaultLayout ? structuredClone(defaultLayout) : null));
registerMock("layout_default_save", ({ layout: next }) => {
  defaultLayout = { ...structuredClone(next as ProjectLayout), tabs: {}, openFiles: [], activeFile: null };
});

registerMock("agent_history_list", () => [...chats.values()].map(summary).sort((a, b) => b.at - a.at));
registerMock("agent_history_load", ({ id }) => {
  const record = chats.get(String(id));
  if (!record) throw new Error(`no chat ${id}`);
  return structuredClone(record);
});
registerMock("agent_history_save", ({ record }) => {
  const r = structuredClone(record as ChatRecord);
  if (!((r.conversation as { turns?: unknown[] }).turns?.length ?? 0)) {
    chats.delete(r.id);
    return null;
  }
  chats.set(r.id, r);
  return summary(r);
});
registerMock("agent_history_delete", ({ id }) => {
  chats.delete(String(id));
});

// ---- Window | Layouts: stored in the user setting `workbench.layouts` like the desktop app
// (`forge_core::project_state::named_layouts`): tables of `{ name, layout: <JSON text> }`.
const LAYOUTS = "workbench.layouts";
const windowPart = (l: ProjectLayout): ProjectLayout => ({ ...structuredClone(l), tabs: {}, openFiles: [], activeFile: null });

function namedLayouts(): NamedLayout[] {
  const raw = mockSettingValue(LAYOUTS);
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((entry) => {
    const { name, layout } = (entry ?? {}) as { name?: string; layout?: string };
    try {
      return name?.trim() && layout ? [{ name: name.trim(), layout: windowPart(JSON.parse(layout)) }] : [];
    } catch {
      return [];
    }
  });
}

function storeLayouts(layouts: NamedLayout[]): void {
  setSetting(LAYOUTS, layouts.map((l) => ({ name: l.name, layout: JSON.stringify(l.layout) })) as unknown as SettingValue, "user");
}

const same = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();

registerMock("layouts_list", () => namedLayouts());
registerMock("layouts_save", ({ name, layout, replace }) => {
  const n = String(name ?? "").trim();
  if (!n) throw new Error("the layout name is empty");
  if (n.length > 64) throw new Error("the layout name is longer than 64 characters");
  const layouts = namedLayouts();
  const entry = { name: n, layout: windowPart(layout as ProjectLayout) };
  const at = layouts.findIndex((l) => same(l.name, n));
  if (at >= 0 && !replace) throw new Error(`a layout named "${n}" already exists`);
  if (at >= 0) layouts[at] = entry;
  else layouts.push(entry);
  storeLayouts(layouts);
  return entry;
});
registerMock("layouts_delete", ({ name }) => {
  const layouts = namedLayouts();
  const rest = layouts.filter((l) => !same(l.name, String(name)));
  if (rest.length === layouts.length) return false;
  storeLayouts(rest);
  return true;
});
