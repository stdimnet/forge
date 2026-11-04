/**
 * Settings | Tools | External Tools, like PHPStorm's page: the tools by group with check
 * boxes (unchecked tools are hidden from menus and the keymap), a toolbar to add, remove,
 * edit, copy and move them, and the Create Tool / Edit Tool dialog. Edits go to the
 * settings draft (Apply / Cancel).
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { ArrowDown, ArrowUp, ChevronDown, ChevronRight, Copy, Minus, Pencil, Plus, Wrench } from "lucide-solid";
import type { ExternalTool } from "@/ipc/runTools";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { EditToolDialog } from "./ToolDialogs";
import { decodeTools, encodeTools, groupName, newTool, TOOLS_KEY } from "./model";
import { t as tr } from "@/i18n";

interface Editing {
  /** Index of the edited tool; -1 creates one. */
  index: number;
  tool: ExternalTool;
}

export function ExternalToolsPage() {
  const tools = () => decodeTools(draftValue(TOOLS_KEY));
  const setTools = (next: ExternalTool[]) => setDraft(TOOLS_KEY, encodeTools(next));
  const [selected, setSelected] = createSignal<number | null>(tools().length ? 0 : null);
  const [collapsed, setCollapsed] = createSignal<string[]>([]);
  const [editing, setEditing] = createSignal<Editing | null>(null);

  const groups = createMemo(() => {
    const out: { name: string; items: { tool: ExternalTool; index: number }[] }[] = [];
    tools().forEach((tool, index) => {
      const g = out.find((x) => x.name === groupName(tool));
      if (g) g.items.push({ tool, index });
      else out.push({ name: groupName(tool), items: [{ tool, index }] });
    });
    return out;
  });
  const current = () => {
    const i = selected();
    return i === null ? undefined : tools()[i];
  };
  const update = (index: number, patch: Partial<ExternalTool>) => setTools(tools().map((t, i) => (i === index ? { ...t, ...patch } : t)));

  const create = () => setEditing({ index: -1, tool: newTool(current() ? groupName(current()!) : undefined) });
  const edit = (index = selected()) => {
    if (index === null) return;
    setEditing({ index, tool: tools()[index] });
  };
  const remove = () => {
    const i = selected();
    if (i === null) return;
    const rest = tools().filter((_, j) => j !== i);
    setTools(rest);
    setSelected(rest.length ? Math.min(i, rest.length - 1) : null);
  };
  const copy = () => {
    const t = current();
    if (!t) return;
    const taken = tools().map((x) => x.name);
    let name = `${t.name} (copy)`;
    for (let n = 2; taken.includes(name); n++) name = `${t.name} (copy ${n})`;
    setTools([...tools(), { ...t, name, outputFilters: [...t.outputFilters] }]);
    setSelected(tools().length - 1);
  };
  /** Move within its group (tools are listed by group). */
  const move = (delta: number) => {
    const i = selected();
    if (i === null) return;
    const list = tools();
    const group = groupName(list[i]);
    let j = i + delta;
    while (j >= 0 && j < list.length && groupName(list[j]) !== group) j += delta;
    if (j < 0 || j >= list.length) return;
    const next = [...list];
    [next[i], next[j]] = [next[j], next[i]];
    setTools(next);
    setSelected(j);
  };
  const finish = (tool: ExternalTool | null) => {
    const e = editing();
    setEditing(null);
    if (!tool || !e) return;
    if (e.index < 0) {
      setTools([...tools(), tool]);
      setSelected(tools().length - 1);
    } else {
      update(e.index, tool);
    }
  };
  const groupEnabled = (name: string) => tools().some((t) => groupName(t) === name && t.enabled);
  const setGroupEnabled = (name: string, enabled: boolean) => setTools(tools().map((t) => (groupName(t) === name ? { ...t, enabled } : t)));

  return (
    <div class="et-page">
      <div class="set-list et-list">
        <div class="set-list-toolbar">
          <button class="icon-btn" title={tr("common.add")} aria-label={tr("common.add")} onClick={create}>
            <Plus size={14} />
          </button>
          <button class="icon-btn" title={tr("common.remove")} aria-label={tr("common.remove")} disabled={!current()} onClick={remove}>
            <Minus size={14} />
          </button>
          <button class="icon-btn" title={tr("common.edit")} aria-label={tr("common.edit")} disabled={!current()} onClick={() => edit()}>
            <Pencil size={14} />
          </button>
          <button class="icon-btn" title={tr("common.copy")} aria-label={tr("common.copy")} disabled={!current()} onClick={copy}>
            <Copy size={14} />
          </button>
          <button class="icon-btn" title={tr("common.moveUp")} aria-label={tr("common.moveUp")} disabled={!current()} onClick={() => move(-1)}>
            <ArrowUp size={14} />
          </button>
          <button class="icon-btn" title={tr("common.moveDown")} aria-label={tr("common.moveDown")} disabled={!current()} onClick={() => move(1)}>
            <ArrowDown size={14} />
          </button>
        </div>
        <div class="et-tree" role="tree">
          <For each={groups()} fallback={<div class="set-list-empty">{tr("tools.externalTools.noToolsConfiguredClickTo")}</div>}>
            {(g) => {
              const open = () => !collapsed().includes(g.name);
              return (
                <>
                  <div class="et-tree-row et-tree-group" data-group={g.name}>
                    <span
                      class="et-chevron"
                      onClick={() => setCollapsed((c) => (c.includes(g.name) ? c.filter((x) => x !== g.name) : [...c, g.name]))}
                    >
                      {open() ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
                    </span>
                    <input type="checkbox" checked={groupEnabled(g.name)} onChange={(e) => setGroupEnabled(g.name, e.currentTarget.checked)} />
                    <span>{g.name}</span>
                  </div>
                  <Show when={open()}>
                    <For each={g.items}>
                      {(it) => (
                        <div
                          class="et-tree-row et-tree-tool"
                          classList={{ selected: selected() === it.index }}
                          data-tool={it.tool.name}
                          onClick={() => setSelected(it.index)}
                          onDblClick={() => edit(it.index)}
                        >
                          <input
                            type="checkbox"
                            checked={it.tool.enabled}
                            onClick={(e) => e.stopPropagation()}
                            onChange={(e) => update(it.index, { enabled: e.currentTarget.checked })}
                          />
                          <Wrench size={13} class="muted" />
                          <span>{it.tool.name}</span>
                        </div>
                      )}
                    </For>
                  </Show>
                </>
              );
            }}
          </For>
        </div>
      </div>
      <Show when={current()}>
        {(t) => (
          <div class="et-summary set-desc">
            <div>
              <b>{t().name}</b>
              {t().description ? ` — ${t().description}` : ""}
            </div>
            <code>
              {t().program} {t().arguments}
            </code>
          </div>
        )}
      </Show>
      <Show when={editing()}>
        {(e) => (
          <EditToolDialog
            tool={e().tool}
            creating={e().index < 0}
            groups={groups().map((g) => g.name)}
            others={tools().filter((_, i) => i !== e().index)}
            onDone={finish}
          />
        )}
      </Show>
    </div>
  );
}
