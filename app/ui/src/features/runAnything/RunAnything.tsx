/**
 * The Run Anything popup (double Ctrl), laid out like PHPStorm's: a title ("Run Anything",
 * "Debug Anything" while Shift is held), the input, suggestions grouped by Recent, Run
 * configurations, npm and Composer scripts, PHP files, plugin commands and the typed
 * command, and a hint line. Suggestions are collected and ranked in Rust
 * (`run_anything_suggest`); `?` lists what can be typed.
 */
import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Bug, CircleHelp, FileCode, Feather, Hexagon, History, Package, Play, Settings2, SquareTerminal } from "lucide-solid";
import { runToolsApi, type ProviderCommands, type RunAnythingGroup, type RunAnythingItem } from "@/ipc/runTools";
import { Highlight } from "@/features/search/SearchEverywhere";
import { allConfigurations } from "@/features/run/configStore";
import { findType, type IconComponent } from "@/features/run/types";
import { usesFramework } from "@/state/project";
import { workspace } from "@/state/workspace";
import { execute, history, loadHistory } from "./actions";
import { collectProviderCommands } from "./providers";
import { closeRunAnything, raQuery, setRaQuery } from "./state";
import { t } from "@/i18n";

const ICONS: Record<string, IconComponent> = {
  recent: History,
  npm: Hexagon,
  composer: Package,
  php: FileCode,
  command: SquareTerminal,
  help: CircleHelp,
  artisan: Feather,
};

function ItemIcon(props: { item: RunAnythingItem }): JSX.Element {
  const type = () => (props.item.typeId ? findType(props.item.typeId) : undefined);
  return (
    <span class="se-icon" style={{ color: type()?.color }}>
      <Dynamic component={type()?.icon ?? ICONS[props.item.icon] ?? (props.item.icon === "configuration" ? Settings2 : SquareTerminal)} size={14} />
    </span>
  );
}

type Row = { type: "header"; group: RunAnythingGroup } | { type: "item"; item: RunAnythingItem };

export function RunAnything() {
  let input!: HTMLInputElement;
  let list!: HTMLDivElement;
  const [groups, setGroups] = createSignal<RunAnythingGroup[]>([]);
  const [selected, setSelected] = createSignal(0);
  const [debug, setDebug] = createSignal(false);
  const [providers, setProviders] = createSignal<ProviderCommands[]>([]);
  let seq = 0;

  async function refresh() {
    const id = ++seq;
    try {
      const res = await runToolsApi.suggest({
        query: raQuery(),
        recent: history(),
        configurations: allConfigurations().map((c) => ({ name: c.name, type: c.type, typeName: findType(c.type)?.name })),
        providers: providers(),
        laravel: usesFramework("laravel"),
      });
      if (id !== seq) return;
      setGroups(res.groups);
      setSelected(0);
    } catch (err) {
      if (id === seq) setGroups([]);
      console.warn("Run Anything suggestions failed", err);
    }
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  createEffect(
    on([raQuery, history, providers], () => {
      clearTimeout(timer);
      timer = setTimeout(() => void refresh(), 50);
    }),
  );
  onCleanup(() => clearTimeout(timer));

  onMount(() => {
    input.focus();
    input.select();
    void loadHistory();
    void collectProviderCommands().then(setProviders);
  });

  const rows = createMemo<Row[]>(() => groups().flatMap((g): Row[] => [{ type: "header", group: g }, ...g.items.map((item) => ({ type: "item" as const, item }))]));
  const selectable = createMemo(() => rows().flatMap((r, i) => (r.type === "item" ? [i] : [])));
  const current = () => {
    const row = rows()[selectable()[selected()]];
    return row?.type === "item" ? row.item : undefined;
  };

  createEffect(() => {
    const idx = selectable()[selected()];
    list?.querySelector(`[data-row="${idx}"]`)?.scrollIntoView({ block: "nearest" });
  });

  async function activate(item: RunAnythingItem | undefined, withDebug: boolean) {
    if (!item) return;
    if (item.action.kind === "insert") {
      setRaQuery(item.action.text);
      input.focus();
      return;
    }
    closeRunAnything();
    await execute(item.action, withDebug);
  }

  function onKeyDown(e: KeyboardEvent) {
    const n = selectable().length;
    if (e.key === "Shift") setDebug(true);
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        if (n) setSelected((s) => (s + 1) % n);
        break;
      case "ArrowUp":
        e.preventDefault();
        if (n) setSelected((s) => (s - 1 + n) % n);
        break;
      case "PageDown":
        e.preventDefault();
        setSelected((s) => Math.min(n - 1, s + 10));
        break;
      case "PageUp":
        e.preventDefault();
        setSelected((s) => Math.max(0, s - 10));
        break;
      case "Enter":
        e.preventDefault();
        void activate(current(), e.shiftKey);
        break;
      case "Escape":
        e.preventDefault();
        closeRunAnything();
        break;
    }
  }

  const empty = () => (raQuery().trim() ? "Nothing found" : "Type a command, a configuration or a script name");

  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeRunAnything()}>
      <div
        class="popup se-popup ra-popup"
        classList={{ debug: debug() }}
        role="dialog"
        aria-label={t("tools.runAnything.runAnything")}
        onKeyDown={onKeyDown}
        onKeyUp={(e) => e.key === "Shift" && setDebug(false)}
      >
        <div class="ra-header">
          <span class="ra-title">{debug() ? t("tools.runAnything.debugAnything") : t("tools.runAnything.runAnything")}</span>
          <span class="ra-context muted" title={t("tools.runAnything.commandsRunInTheProject")}>
            {workspace()?.name ?? t("common.project")}
          </span>
        </div>
        <div class="se-search ra-search">
          <span class="ra-mode">{debug() ? <Bug size={15} /> : <Play size={15} />}</span>
          <input
            ref={input}
            class="se-input"
            value={raQuery()}
            spellcheck={false}
            autocomplete="off"
            placeholder={t("tools.runAnything.runAnyCommandConfigurationOr")}
            onInput={(e) => setRaQuery(e.currentTarget.value)}
            onBlur={() => setDebug(false)}
          />
        </div>
        <div class="se-results ra-results" ref={list}>
          <Show when={rows().length} fallback={<div class="se-empty">{empty()}</div>}>
            <For each={rows()}>
              {(row, i) => {
                if (row.type === "header") return <div class="se-group">{row.group.title}</div>;
                const item = row.item;
                const isSel = () => selectable()[selected()] === i();
                return (
                  <div
                    class="se-row ra-row"
                    classList={{ selected: isSel() }}
                    data-row={i()}
                    onMouseMove={() => {
                      const idx = selectable().indexOf(i());
                      if (idx >= 0) setSelected(idx);
                    }}
                    onClick={(e) => void activate(item, e.shiftKey)}
                  >
                    <ItemIcon item={item} />
                    <span class="se-label">
                      <Highlight text={item.label} positions={item.matches} />
                    </span>
                    <Show when={item.detail}>
                      <span class="se-detail">{item.detail}</span>
                    </Show>
                  </div>
                );
              }}
            </For>
          </Show>
        </div>
        <div class="se-footer ra-footer">
          <span>{debug() ? t("tools.runAnything.releaseShiftToRun") : t("tools.runAnything.pressShiftToDebug")}</span>
          <span class="ra-footer-sep">·</span>
          <span>{t("tools.runAnything.typeForHelp")}</span>
        </div>
      </div>
    </div>
  );
}
