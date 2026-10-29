/**
 * Run widget of the main toolbar: the configuration dropdown (configurations + "Edit
 * Configurations…"), Run / Rerun, Debug and Stop, like PHPStorm's New UI.
 */
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { Portal, Dynamic } from "solid-js/web";
import { Bug, ChevronDown, Play, RotateCcw, Settings2, Square } from "lucide-solid";
import type { RunConfiguration } from "@/ipc/run";
import { shortcutLabel } from "@/keymap";
import { workspace } from "@/state/workspace";
import { configurations, isTemporary, selectConfiguration, selectedConfiguration, temporary } from "./configStore";
import { debugConfiguration, isRunning, runConfiguration, runningFor, stopConfiguration } from "./runStore";
import { openRunConfigurations } from "./dialogState";
import { findType } from "./types";

const keyHint = (title: string, action: string) => {
  const k = shortcutLabel(action);
  return k ? `${title} (${k})` : title;
};

export function ConfigIcon(props: { cfg: RunConfiguration; size?: number }) {
  const type = () => findType(props.cfg.type);
  return (
    <span class="run-config-type-icon" classList={{ temporary: isTemporary(props.cfg) }} style={{ color: type()?.color }}>
      <Dynamic component={type()?.icon ?? Play} size={props.size ?? 14} />
    </span>
  );
}

const [dropdownOpen, setDropdownOpen] = createSignal(false);
export { dropdownOpen, setDropdownOpen };

function Dropdown(props: { anchor: HTMLElement }) {
  let el!: HTMLDivElement;
  const rect = props.anchor.getBoundingClientRect();
  const onDoc = (e: MouseEvent) => {
    const t = e.target as Element;
    if (!t.closest(".run-dropdown") && !t.closest(".run-config")) setDropdownOpen(false);
  };
  onMount(() => {
    document.addEventListener("mousedown", onDoc, true);
    el.focus();
  });
  onCleanup(() => document.removeEventListener("mousedown", onDoc, true));
  const pick = (cfg: RunConfiguration) => {
    selectConfiguration(cfg.name);
    setDropdownOpen(false);
  };
  const Item = (p: { cfg: RunConfiguration }) => (
    <div
      class="run-dd-item"
      classList={{ selected: selectedConfiguration()?.name === p.cfg.name }}
      role="option"
      data-config={p.cfg.name}
      onClick={() => pick(p.cfg)}
    >
      <ConfigIcon cfg={p.cfg} />
      <span class="run-dd-name">{p.cfg.name}</span>
      <Show when={runningFor(p.cfg)}>
        <span class="run-dd-live" title="Running" />
      </Show>
      <span class="run-dd-actions">
        <button
          class="icon-btn run-btn"
          title={`Run '${p.cfg.name}'`}
          onClick={(e) => {
            e.stopPropagation();
            pick(p.cfg);
            void runConfiguration(p.cfg);
          }}
        >
          <Play size={13} fill="currentColor" />
        </button>
        <button
          class="icon-btn debug-btn"
          title={`Debug '${p.cfg.name}'`}
          onClick={(e) => {
            e.stopPropagation();
            pick(p.cfg);
            void debugConfiguration(p.cfg);
          }}
        >
          <Bug size={13} />
        </button>
      </span>
    </div>
  );
  return (
    <Portal>
      <div
        ref={el}
        class="popup run-dropdown"
        tabIndex={-1}
        role="listbox"
        style={{ top: `${rect.bottom + 4}px`, left: `${Math.max(8, rect.left)}px` }}
        onKeyDown={(e) => e.key === "Escape" && (e.stopPropagation(), setDropdownOpen(false))}
      >
        <Show when={configurations().length || temporary().length} fallback={<div class="run-dd-empty muted">No run configurations yet</div>}>
          <For each={configurations()}>{(cfg) => <Item cfg={cfg} />}</For>
          <Show when={temporary().length}>
            <div class="run-dd-sep" />
            <For each={temporary()}>{(cfg) => <Item cfg={cfg} />}</For>
          </Show>
        </Show>
        <div class="run-dd-sep" />
        <div
          class="run-dd-item"
          role="option"
          onClick={() => {
            setDropdownOpen(false);
            openRunConfigurations(selectedConfiguration()?.name);
          }}
        >
          <span class="run-config-type-icon muted">
            <Settings2 size={14} />
          </span>
          <span class="run-dd-name">Edit Configurations…</span>
        </div>
      </div>
    </Portal>
  );
}

export function RunWidget() {
  let pill!: HTMLButtonElement;
  const cfg = () => selectedConfiguration();
  const running = () => runningFor(cfg());
  return (
    <Show when={workspace()}>
      <button
        ref={pill}
        class="pill-btn run-config"
        classList={{ open: dropdownOpen() }}
        title="Run/Debug Configurations"
        onClick={() => (cfg() || configurations().length ? setDropdownOpen((o) => !o) : openRunConfigurations())}
      >
        <Show when={cfg()} fallback={<span class="muted">Add Configuration…</span>}>
          {(c) => (
            <>
              <ConfigIcon cfg={c()} />
              <span class="run-config-name">{c().name}</span>
            </>
          )}
        </Show>
        <ChevronDown size={14} class="muted" />
      </button>
      <Show when={dropdownOpen()}>
        <Dropdown anchor={pill} />
      </Show>
      <button
        class="icon-btn run-btn"
        title={cfg() ? keyHint(isRunning(running()) ? `Rerun '${cfg()!.name}'` : `Run '${cfg()!.name}'`, "Run") : keyHint("Run", "Run")}
        onClick={() => void runConfiguration()}
      >
        <Show when={running()} fallback={<Play size={16} fill="currentColor" />}>
          <RotateCcw size={16} />
        </Show>
      </button>
      <button class="icon-btn debug-btn" title={cfg() ? keyHint(`Debug '${cfg()!.name}'`, "Debug") : keyHint("Debug", "Debug")} onClick={() => void debugConfiguration()}>
        <Bug size={16} />
      </button>
      <Show when={running()}>
        {(s) => (
          <button
            class="icon-btn stop-btn"
            title={keyHint(`Stop '${cfg()?.name ?? s().name}'`, "Stop")}
            onClick={() => void stopConfiguration(cfg())}
          >
            <Square size={13} fill="currentColor" />
          </button>
        )}
      </Show>
    </Show>
  );
}
