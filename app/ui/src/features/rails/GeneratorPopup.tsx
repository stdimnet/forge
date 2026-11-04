/**
 * Tools | Run Rails Generator… (RubyMine's, Ctrl+Alt+G): a popup that completes the
 * generators (from `bin/rails generate` in trusted projects, else Rails' own and the
 * project's `lib/generators`), shows the chosen generator's arguments, and runs
 * `bin/rails generate <generator> <arguments>` (or `destroy`) in the Run tool window.
 * Generators load the application, so the backend refuses them in Safe Mode.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { RefreshCw, Wand2, X } from "lucide-solid";
import { railsApi, type RailsGenerator, type RailsGenerators } from "@/ipc/rails";
import { runRubyCommand } from "@/features/ruby/state";
import { shortcutLabel } from "@/keymap";
import { errorText, loadRailsProject, showRailsBalloon } from "./state";
import { t } from "@/i18n";

const [open, setOpen] = createSignal(false);
const [destroy, setDestroy] = createSignal(false);
const [text, setText] = createSignal("");
const [list, setList] = createSignal<RailsGenerators | null>(null);
const [loading, setLoading] = createSignal(false);
const [selected, setSelected] = createSignal(0);
const [pretend, setPretend] = createSignal(false);
const [force, setForce] = createSignal(false);
const [skip, setSkip] = createSignal(false);

export { open as generatorPopupOpen };

async function load(refresh = false): Promise<void> {
  setLoading(true);
  try {
    setList(await railsApi.generators(refresh));
  } catch (e) {
    setList({ generators: [], source: "static", message: errorText(e) });
  } finally {
    setLoading(false);
  }
}

export function openGeneratorPopup(opts: { destroy?: boolean; prefill?: string } = {}): void {
  setDestroy(!!opts.destroy);
  setText(opts.prefill ?? "");
  setSelected(0);
  setOpen(true);
  void load();
}

export function closeGeneratorPopup(): void {
  setOpen(false);
}

const generatorWord = () => text().trimStart().split(/\s+/)[0] ?? "";
const typingGenerator = () => !/\s/.test(text().trimStart());

/** Run a generator in the Run tool window, then read the project again. */
export async function runGenerator(line: string, isDestroy = false, options: string[] = []): Promise<void> {
  const [generator, ...rest] = line.trim().split(/\s+/);
  if (!generator) return;
  try {
    const cmd = await railsApi.generateCommand(generator, rest.join(" "), options, isDestroy);
    const code = await runRubyCommand(cmd, `rails ${isDestroy ? "destroy" : "generate"} ${line.trim()}`.slice(0, 60));
    if (code === 0) void loadRailsProject(true);
  } catch (e) {
    showRailsBalloon(`Cannot run rails ${isDestroy ? "destroy" : "generate"}`, errorText(e), 8000);
  }
}

export function GeneratorPopup() {
  let input: HTMLInputElement | undefined;
  const matches = createMemo<RailsGenerator[]>(() => {
    const all = list()?.generators ?? [];
    if (!typingGenerator()) return all.filter((g) => g.name === generatorWord());
    const q = generatorWord().toLowerCase();
    const starts = all.filter((g) => g.name.toLowerCase().startsWith(q));
    const contains = all.filter((g) => !g.name.toLowerCase().startsWith(q) && g.name.toLowerCase().includes(q));
    return [...starts, ...contains];
  });
  const current = () => (list()?.generators ?? []).find((g) => g.name === generatorWord()) ?? (typingGenerator() ? matches()[selected()] : undefined);
  const complete = (g: RailsGenerator) => {
    setText(`${g.name} `);
    setSelected(0);
    input?.focus();
  };
  const options = () => [pretend() && "--pretend", force() && "--force", skip() && "--skip"].filter(Boolean) as string[];
  const run = () => {
    const pick = typingGenerator() ? matches()[selected()] : undefined;
    const line = pick && pick.name !== generatorWord() ? pick.name : text();
    if (!line.trim()) return;
    closeGeneratorPopup();
    void runGenerator(line, destroy(), options());
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const n = matches().length;
    if (e.key === "Escape") {
      e.preventDefault();
      closeGeneratorPopup();
    } else if (e.key === "ArrowDown" && n) {
      e.preventDefault();
      setSelected((i) => (i + 1) % n);
    } else if (e.key === "ArrowUp" && n) {
      e.preventDefault();
      setSelected((i) => (i - 1 + n) % n);
    } else if (e.key === "Tab" && n && typingGenerator()) {
      e.preventDefault();
      complete(matches()[selected()]);
    } else if (e.key === "Enter") {
      e.preventDefault();
      run();
    }
  };
  const verb = () => (destroy() ? "destroy" : "generate");
  return (
    <Show when={open()}>
      <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeGeneratorPopup()}>
        <div class="popup rails-gen-popup" role="dialog" aria-label={destroy() ? t("langs.rails.destroyGeneratedCode") : t("langs.rails.runRailsGenerator")}>
          <div class="cli-popup-header">
            <Wand2 size={15} class="cli-popup-icon" />
            <span class="cli-popup-title">{destroy() ? t("langs.rails.destroyGeneratedCode") : t("langs.rails.runRailsGenerator")}</span>
            <span class="muted rails-gen-source">{list()?.source === "rails" ? t("langs.rails.generatorsFromBinRailsGenerate") : t("langs.rails.railsAndProjectGenerators")}</span>
            <span class="kbd cli-popup-shortcut">{shortcutLabel("rails.generate")}</span>
            <button class="icon-btn" title={t("langs.rails.reloadGenerators")} onClick={() => void load(true)}>
              <RefreshCw size={13} classList={{ spin: loading() }} />
            </button>
            <button class="icon-btn" title={t("common.closeEsc")} onClick={closeGeneratorPopup}>
              <X size={14} />
            </button>
          </div>
          <Show when={list()?.message}>
            <div class="cli-popup-notice">{list()!.message}</div>
          </Show>
          <div class="rails-gen-input">
            <span class="rails-gen-prompt">rails {verb()}</span>
            <input
              ref={(el) => {
                input = el;
                queueMicrotask(() => el.focus());
              }}
              value={text()}
              placeholder={t("langs.rails.modelArticleTitleStringBody")}
              spellcheck={false}
              onInput={(e) => {
                setText(e.currentTarget.value);
                setSelected(0);
              }}
              onKeyDown={onKeyDown}
            />
          </div>
          <Show when={current()}>
            {(g) => (
              <div class="rails-gen-usage">
                <span class="rails-gen-usage-label muted">{t("langs.rails.usageLabel")}</span>
                <code>
                  rails {verb()} {g().name} {g().usage}
                </code>
                <span class="muted">{g().description}</span>
              </div>
            )}
          </Show>
          <div class="rails-gen-list">
            <For each={matches().slice(0, 200)}>
              {(g, i) => (
                <div
                  class="rails-gen-item"
                  classList={{ selected: selected() === i() }}
                  data-generator={g.name}
                  onMouseEnter={() => setSelected(i())}
                  onClick={() => complete(g)}
                >
                  <span class="rails-gen-name">{g.name}</span>
                  <span class="rails-gen-group">{g.group}</span>
                  <span class="muted rails-gen-desc">{g.description || g.usage}</span>
                </div>
              )}
            </For>
            <Show when={!matches().length && !loading()}>
              <div class="rails-gen-empty muted">{t("langs.rails.noMatchingGenerators")}</div>
            </Show>
          </div>
          <div class="rails-gen-options">
            <label>
              <input type="checkbox" checked={pretend()} onChange={(e) => setPretend(e.currentTarget.checked)} /> {t("langs.rails.pretendPretend")}
            </label>
            <label>
              <input type="checkbox" checked={force()} onChange={(e) => setForce(e.currentTarget.checked)} /> {t("langs.rails.overwriteFilesForce")}
            </label>
            <label>
              <input type="checkbox" checked={skip()} onChange={(e) => setSkip(e.currentTarget.checked)} /> {t("langs.rails.skipExistingSkip")}
            </label>
          </div>
          <div class="cli-popup-footer muted">
            <span>{t("langs.rails.tabToCompleteToChoose")}</span>
          </div>
        </div>
      </div>
    </Show>
  );
}
