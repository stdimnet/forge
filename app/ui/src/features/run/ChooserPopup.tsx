/**
 * "Run…" / "Debug…" popup (Alt+Shift+F10 / Alt+Shift+F9): numbered configurations with
 * speed search; Enter runs, Shift+Enter debugs, 0 opens Edit Configurations.
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { Settings2 } from "lucide-solid";
import type { RunConfiguration } from "@/ipc/run";
import { allConfigurations, selectConfiguration } from "./configStore";
import { closeChooser, configurationChooser, openRunConfigurations } from "./dialogState";
import { debugConfiguration, runConfiguration } from "./runStore";
import { ConfigIcon } from "./RunWidget";
import { t } from "@/i18n";

export function ChooserPopup() {
  return (
    <Show when={configurationChooser()} keyed>
      {(mode) => <Chooser mode={mode} />}
    </Show>
  );
}

function Chooser(props: { mode: "run" | "debug" }) {
  let el!: HTMLDivElement;
  const [query, setQuery] = createSignal("");
  const [index, setIndex] = createSignal(1);
  const items = createMemo(() => {
    const q = query().toLowerCase();
    return allConfigurations().filter((c) => !q || c.name.toLowerCase().includes(q));
  });
  onMount(() => el.focus());
  const choose = (cfg: RunConfiguration | null, debug: boolean) => {
    closeChooser();
    if (!cfg) return openRunConfigurations();
    selectConfiguration(cfg.name);
    void (debug ? debugConfiguration(cfg) : runConfiguration(cfg));
  };
  const onKeyDown = (e: KeyboardEvent) => {
    const count = items().length + 1;
    if (e.key === "Escape") closeChooser();
    else if (e.key === "ArrowDown") setIndex((i) => (i + 1) % count);
    else if (e.key === "ArrowUp") setIndex((i) => (i - 1 + count) % count);
    else if (e.key === "Enter") choose(index() === 0 ? null : items()[index() - 1], props.mode === "debug" || e.shiftKey);
    else if (/^[0-9]$/.test(e.key) && !query()) {
      const n = Number(e.key);
      if (n === 0) choose(null, false);
      else if (items()[n - 1]) choose(items()[n - 1], props.mode === "debug");
    } else if (e.key === "Backspace") setQuery((q) => q.slice(0, -1));
    else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      setQuery((q) => q + e.key);
      setIndex(1);
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };
  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeChooser()}>
      <div ref={el} class="popup run-chooser" tabIndex={-1} role="listbox" onKeyDown={onKeyDown}>
        <div class="run-chooser-title">
          <span>{props.mode === "debug" ? t("run.chooserPopup.debug") : t("run.chooserPopup.run")}</span>
          <Show when={query()}>
            <span class="run-chooser-query">{query()}</span>
          </Show>
        </div>
        <div class="run-dd-item" classList={{ selected: index() === 0 }} onMouseEnter={() => setIndex(0)} onClick={() => choose(null, false)}>
          <span class="run-chooser-num">0</span>
          <span class="run-config-type-icon muted">
            <Settings2 size={14} />
          </span>
          <span class="run-dd-name">{t("run.chooserPopup.editConfigurations")}</span>
        </div>
        <For each={items()}>
          {(cfg, i) => (
            <div
              class="run-dd-item"
              classList={{ selected: index() === i() + 1 }}
              onMouseEnter={() => setIndex(i() + 1)}
              onClick={(e) => choose(cfg, props.mode === "debug" || e.shiftKey)}
            >
              <span class="run-chooser-num">{i() < 9 ? i() + 1 : ""}</span>
              <ConfigIcon cfg={cfg} />
              <span class="run-dd-name">{cfg.name}</span>
            </div>
          )}
        </For>
        <div class="run-chooser-hint muted">{props.mode === "run" ? t("run.chooserPopup.holdShiftToDebug") : t("run.chooserPopup.enterToDebug")}</div>
      </div>
    </div>
  );
}
