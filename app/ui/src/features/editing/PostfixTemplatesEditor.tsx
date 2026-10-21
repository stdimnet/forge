/**
 * Settings editor of `editor.postfix.disabled`: PHPStorm's template tree (one checkbox per
 * template, grouped by language) with the selected template's description and a
 * before/after example. The setting stores the ids of the cleared templates. The templates
 * and their examples come from Rust (`editor_postfix_templates`).
 */
import { createEffect, createMemo, createResource, createSignal, For, Show } from "solid-js";
import { ChevronDown, ChevronRight } from "lucide-solid";
import type { SettingSpec } from "@/ipc/settings";
import type { PostfixTemplateInfo } from "@/ipc/editing";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { Hl } from "@/features/settings/dialogSearch";
import { loadPostfixTemplates } from "./postfix";

export function PostfixTemplatesEditor(props: { spec: SettingSpec }) {
  const [templates] = createResource(loadPostfixTemplates, { initialValue: [] });
  const off = () => draftValue<string[]>(props.spec.key) ?? [];
  const groups = createMemo(() => {
    const out: { group: string; items: PostfixTemplateInfo[] }[] = [];
    for (const t of templates()) {
      const last = out[out.length - 1];
      if (last?.group === t.group) last.items.push(t);
      else out.push({ group: t.group, items: [t] });
    }
    return out;
  });
  const [collapsed, setCollapsed] = createSignal<string[]>([]);
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const selected = () => templates().find((t) => t.id === selectedId()) ?? templates()[0];

  const setOn = (ids: string[], on: boolean) => {
    const rest = off().filter((id) => !ids.includes(id));
    setDraft(props.spec.key, on ? rest : [...rest, ...ids]);
  };
  const toggleGroup = (group: string) =>
    setCollapsed((c) => (c.includes(group) ? c.filter((g) => g !== group) : [...c, group]));

  return (
    <div class="postfix-editor">
      <div class="postfix-tree" role="tree">
        <For each={groups()}>
          {(g) => {
            const ids = () => g.items.map((t) => t.id);
            const onCount = () => ids().filter((id) => !off().includes(id)).length;
            const open = () => !collapsed().includes(g.group);
            return (
              <>
                <div class="postfix-row postfix-group" role="treeitem" aria-expanded={open()}>
                  <button class="postfix-chevron" onClick={() => toggleGroup(g.group)} title={open() ? "Collapse" : "Expand"}>
                    {open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                  </button>
                  <input
                    type="checkbox"
                    checked={onCount() === ids().length}
                    ref={(el) => createEffect(() => (el.indeterminate = onCount() > 0 && onCount() < ids().length))}
                    onChange={(e) => setOn(ids(), e.currentTarget.checked)}
                  />
                  <span onClick={() => toggleGroup(g.group)}>
                    <Hl text={g.group} />
                  </span>
                </div>
                <Show when={open()}>
                  <For each={g.items}>
                    {(t) => (
                      <div
                        class="postfix-row postfix-template"
                        classList={{ selected: selected()?.id === t.id }}
                        role="treeitem"
                        data-template={t.id}
                        onClick={() => setSelectedId(t.id)}
                      >
                        <input
                          type="checkbox"
                          checked={!off().includes(t.id)}
                          onClick={(e) => e.stopPropagation()}
                          onChange={(e) => setOn([t.id], e.currentTarget.checked)}
                        />
                        <span class="postfix-key">
                          .<Hl text={t.key} />
                        </span>
                        <span class="postfix-desc">
                          <Hl text={t.description} />
                        </span>
                      </div>
                    )}
                  </For>
                </Show>
              </>
            );
          }}
        </For>
      </div>
      <Show when={selected()}>
        {(t) => (
          <div class="postfix-preview">
            <div class="postfix-preview-title">Description</div>
            <div class="postfix-preview-text">{t().description}.</div>
            <div class="postfix-preview-title">Before</div>
            <pre class="postfix-code">{t().before}</pre>
            <div class="postfix-preview-title">After</div>
            <pre class="postfix-code">{t().after}</pre>
          </div>
        )}
      </Show>
    </div>
  );
}
