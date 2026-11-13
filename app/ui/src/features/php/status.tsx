/** Status bar widgets: indexing progress and the PHP language level. */
import { createSignal, Show } from "solid-js";
import { listen } from "@/ipc";
import { runCommand } from "@/state/commands";
import { isPluginActive } from "@/state/project";
import { phpApi, type IndexProgress, type IndexStats } from "./api";
import { defaultPhpSettings, LEVEL_FROM_INTERPRETER, phpSettings } from "./settings";
import { languageLevel } from "./composer";
import { t } from "@/i18n";

const [progress, setProgress] = createSignal<IndexProgress | null>(null);
const [lastStats, setLastStats] = createSignal<IndexStats | null>(null);

export { progress as indexProgress, lastStats };

export const percent = (p: IndexProgress) => (p.total ? Math.floor((p.done / p.total) * 100) : 0);

let running: Promise<IndexStats | null> | null = null;
let listening = false;

/** Index the workspace, reporting progress in the status bar. */
export function indexWorkspace(): Promise<IndexStats | null> {
  if (running) return running;
  if (!listening) {
    listening = true;
    void listen<IndexProgress>("php://index-progress", (p) => {
      if (running) setProgress(p);
    });
  }
  setProgress({ done: 0, total: 0 });
  running = phpApi
    .indexWorkspace()
    .then((s) => {
      setLastStats(s);
      return s;
    })
    .catch((e) => {
      console.warn("PHP indexing failed", e);
      return null;
    })
    .finally(() => {
      running = null;
      setProgress(null);
    });
  return running;
}

export function IndexingStatus() {
  return (
    <Show when={progress()}>
      {(p) => (
        <span class="status-item php-indexing" title={t("php.indexing.tooltip", { done: p().done, total: p().total })}>
          <span>{t("php.indexing", { percent: percent(p()) })}</span>
          <span class="php-progress">
            <span class="php-progress-bar" style={{ width: `${percent(p())}%` }} />
          </span>
        </span>
      )}
    </Show>
  );
}

/** The level in effect; "From the CLI interpreter" falls back to the default until known. */
const level = () => {
  const l = languageLevel()?.level ?? phpSettings.languageLevel;
  return l === LEVEL_FROM_INTERPRETER ? defaultPhpSettings.languageLevel : l;
};

export function PhpVersionStatus() {
  return (
    <Show when={isPluginActive("forge.php")}>
      <button
        class="status-item"
        data-status="php-version"
        title={
          languageLevel()?.fromComposer
            ? t("php.level.composer", { level: level() })
            : languageLevel()?.fromInterpreter
              ? t("php.level.interpreter", { level: level() })
              : t("php.level", { level: level() })
        }
        onClick={() => void runCommand("settings.open")}
      >
        PHP {level()}
      </button>
    </Show>
  );
}
