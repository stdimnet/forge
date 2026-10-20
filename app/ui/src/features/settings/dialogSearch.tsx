/**
 * The search of the Settings dialog: what the dialog sends to Rust (pages, the settings
 * schema, the texts it shows) and helpers to render the answer.
 */
import { For } from "solid-js";
import { settingsPages } from "@/features/settingsPages";
import { settingsState } from "@/state/settings";
import { isOpen, query } from "./dialogState";
import { pluginText } from "./PluginsPage";
import { createGlobalSettingsSearch, segmentsOf } from "./search";

/** Every text the dialog may highlight: node titles and the schema's labels. */
function highlightTexts(): string[] {
  const texts = new Set<string>();
  for (const p of settingsPages()) texts.add(p.title).add(p.group);
  for (const s of settingsState.schema) {
    texts.add(s.title);
    if (s.description) texts.add(s.description);
    if (s.section) texts.add(s.section);
  }
  return [...texts];
}

export const dialogSearch = createGlobalSettingsSearch(() =>
  isOpen()
    ? {
        query: query(),
        pages: settingsPages().map((p) => ({
          id: p.id,
          title: p.title,
          group: p.group,
          parent: p.parent,
          keywords: p.keywords,
          text: p.id === "plugins" ? settingsState.plugins.map(pluginText).join("\n") : "",
        })),
        settings: settingsState.schema,
        highlight: highlightTexts(),
      }
    : null,
);

/** Text with search hits highlighted. */
export function Hl(props: { text: string }) {
  return (
    <For each={segmentsOf(dialogSearch(), props.text)}>
      {(s) => (s.hit ? <mark class="set-hl">{s.text}</mark> : s.text)}
    </For>
  );
}

/** Whether a setting row matches the dialog's search. */
export const settingMatches = (key: string) => dialogSearch().response.settings.includes(key);

/** Whether a tree node matches the dialog's search itself. */
export const nodeMatches = (id: string) => dialogSearch().response.matches.includes(id);
