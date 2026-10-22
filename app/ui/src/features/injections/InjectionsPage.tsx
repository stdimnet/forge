/**
 * Settings | Editor | Language Injections, like PHPStorm's page: the built-in injection
 * rules (from Rust, `injections_rules`) with a check box each, grouped by the host language,
 * and the places turned off with Un-inject Language. Edits go to the dialog draft
 * (`editor.injections.disabled` / `.uninjected`), so Apply and Cancel work.
 */
import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import { Braces, Code, CodeXml, Database, FileCode, Palette, Regex } from "lucide-solid";
import type { JSX } from "solid-js";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { injectionsApi, type InjectionRule } from "@/ipc/injections";
import { DISABLED_KEY, UNINJECTED_KEY } from "./highlight";

const ICONS: Record<string, (p: { size: number }) => JSX.Element> = {
  sql: Database,
  html: Code,
  css: Palette,
  json: Braces,
  regexp: Regex,
  javascript: FileCode,
  xml: CodeXml,
};

export function InjectionsPage() {
  const [rules] = createResource(() => injectionsApi.rules().catch(() => [] as InjectionRule[]));
  const [selected, setSelected] = createSignal<string | null>(null);
  const disabled = () => draftValue<string[]>(DISABLED_KEY) ?? [];
  const uninjected = () => draftValue<string[]>(UNINJECTED_KEY) ?? [];
  const isOn = (id: string) => !disabled().includes(id);
  const toggle = (id: string, on: boolean) => {
    const rest = disabled().filter((d) => d !== id);
    setDraft(DISABLED_KEY, on ? rest : [...rest, id]);
  };
  const hosts = createMemo(() => {
    const groups: { label: string; rules: InjectionRule[] }[] = [];
    for (const r of rules() ?? []) {
      let g = groups.find((x) => x.label === r.hostLabel);
      if (!g) groups.push((g = { label: r.hostLabel, rules: [] }));
      g.rules.push(r);
    }
    return groups;
  });
  const current = () => (rules() ?? []).find((r) => r.id === selected());

  return (
    <div class="inj-page">
      <div class="set-desc inj-intro">
        Strings that get another language's highlighting and editing. A <code>// language=SQL</code> comment before a
        string always injects; Alt+Enter in a string offers Inject language or reference.
      </div>
      <div class="set-list inj-table-box">
        <table class="set-table inj-table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Language</th>
              <th>Scope</th>
            </tr>
          </thead>
          <tbody>
            <For each={hosts()}>
              {(group) => (
                <>
                  <tr class="inj-group-row">
                    <td colSpan={3}>{group.label}</td>
                  </tr>
                  <For each={group.rules}>
                    {(rule) => {
                      const Icon = ICONS[rule.language] ?? FileCode;
                      return (
                        <tr
                          data-rule={rule.id}
                          classList={{ selected: selected() === rule.id, off: !isOn(rule.id) }}
                          onClick={() => setSelected(rule.id)}
                        >
                          <td class="inj-name">
                            <input
                              type="checkbox"
                              checked={isOn(rule.id)}
                              aria-label={rule.name}
                              onClick={(e) => e.stopPropagation()}
                              onChange={(e) => toggle(rule.id, e.currentTarget.checked)}
                            />
                            <Icon size={13} />
                            <span>{rule.name}</span>
                          </td>
                          <td>{rule.languageLabel}</td>
                          <td class="inj-scope">Built-in</td>
                        </tr>
                      );
                    }}
                  </For>
                </>
              )}
            </For>
          </tbody>
        </table>
      </div>
      <div class="inj-details">
        <Show when={current()} fallback={<span class="set-desc">Select a rule to see where it applies.</span>}>
          {(r) => (
            <>
              <div class="inj-details-title">
                {r().hostLabel}: {r().name}
              </div>
              <div class="set-desc">{r().places}</div>
            </>
          )}
        </Show>
      </div>
      <div class="inj-uninjected">
        <span class="set-desc">
          {uninjected().length
            ? `${uninjected().length} string${uninjected().length === 1 ? "" : "s"} un-injected in this project`
            : "No strings un-injected in this project"}
        </span>
        <Show when={uninjected().length}>
          <button class="set-link inj-restore" onClick={() => setDraft(UNINJECTED_KEY, [])}>
            Restore All
          </button>
        </Show>
      </div>
    </div>
  );
}
