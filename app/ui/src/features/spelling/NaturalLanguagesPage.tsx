/**
 * Settings | Editor | Natural Languages: the checked language and the dictionaries in use
 * (bundled, custom with their word counts or load errors), with links to the Spelling and
 * Grammar and Style pages.
 */
import { createResource, For, Show } from "solid-js";
import { BookA, CircleAlert } from "lucide-solid";
import { spellApi } from "@/ipc/spell";
import { setSelected } from "@/features/settings/dialogState";
import { getSetting } from "@/state/settings";

export const NATURAL_LANGUAGES_PAGE = "editor.natural-languages";
export const SPELLING_PAGE = "editor.natural-languages.spelling";
export const GRAMMAR_PAGE = "editor.natural-languages.grammar";

export function NaturalLanguagesPage() {
  const [dictionaries] = createResource(
    () => getSetting<string[]>("spelling.customDictionaries", []).join("\n"),
    () => spellApi.dictionaries().catch(() => []),
  );
  return (
    <div class="set-page spell-page">
      <section class="set-section">
        <div class="set-section-title">
          <span>Languages</span>
        </div>
        <div class="set-section-body indented spell-section-body">
          <div class="spell-language">
            <BookA size={14} />
            <span>English (USA)</span>
            <span class="spell-muted">Spelling and grammar</span>
          </div>
          <div class="set-desc">
            Comments, string literals, declared names, Markdown and plain text files and commit messages are checked. Add
            more languages as Hunspell dictionaries under Spelling.
          </div>
        </div>
      </section>
      <section class="set-section">
        <div class="set-section-title">
          <span>Dictionaries</span>
        </div>
        <div class="set-section-body indented spell-section-body">
          <div class="set-table-box">
            <table class="set-table spell-dictionaries">
              <thead>
                <tr>
                  <th>Dictionary</th>
                  <th>Words</th>
                  <th>Source</th>
                </tr>
              </thead>
              <tbody>
                <For each={dictionaries() ?? []}>
                  {(d) => (
                    <tr data-dictionary={d.name}>
                      <td>{d.name}</td>
                      <td>
                        <Show when={!d.error} fallback={<span class="spell-error">—</span>}>
                          {d.words.toLocaleString("en-US")}
                        </Show>
                      </td>
                      <td class="spell-source" title={d.error ?? d.path ?? undefined}>
                        <Show when={d.error} fallback={d.builtin ? "Bundled" : d.path}>
                          <span class="spell-error">
                            <CircleAlert size={12} /> {d.error}
                          </span>
                        </Show>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
          <div class="spell-links">
            <a class="set-link" href="#" onClick={(e) => (e.preventDefault(), setSelected(SPELLING_PAGE))}>
              Spelling
            </a>
            <a class="set-link" href="#" onClick={(e) => (e.preventDefault(), setSelected(GRAMMAR_PAGE))}>
              Grammar and Style
            </a>
          </div>
        </div>
      </section>
    </div>
  );
}
