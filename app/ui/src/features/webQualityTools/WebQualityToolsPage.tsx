/**
 * Settings | Languages & Frameworks | Code Quality Tools, like PHPStorm's ESLint,
 * Prettier, Stylelint and Biome pages on one page: per tool Disabled / Automatic / Manual
 * configuration, what was found in the project, the manual package, configuration file,
 * working directories and options, "Run for files", on save, on Reformat Code, and Validate.
 * Values go through the dialog draft, so Apply, Cancel and Reset work.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { ChevronDown, ChevronRight, CircleCheck, CircleX, LoaderCircle } from "lucide-solid";
import { webQualityToolsApi, type WebTool, type WebToolConfig, type WebToolInfo, type WebToolMode, type WebValidation } from "@/ipc/webQualityTools";
import { findSpec } from "@/state/settings";
import { draft, draftValue, openSettings, setDraft } from "@/features/settings/dialogState";
import { Hl, settingMatches } from "@/features/settings/dialogSearch";
import { SettingRow } from "@/features/settings/SchemaPage";
import { ALL_TOOLS } from "./run";
import { t as tr } from "@/i18n";
import "@/features/phpQualityTools/phpQualityTools.css";
import "./webQualityTools.css";

const key = (tool: WebTool, option: string) => `web.${tool}.${option}`;
const ON_SAVE: Record<WebTool, string> = {
  eslint: "actionsOnSave.eslint",
  prettier: "actionsOnSave.prettier.enabled",
  stylelint: "actionsOnSave.stylelint",
  biome: "actionsOnSave.biome",
};
const FILES: Record<WebTool, string> = {
  eslint: "web.eslint.files",
  prettier: "actionsOnSave.prettier.files",
  stylelint: "web.stylelint.files",
  biome: "web.biome.files",
};
const MODES: { value: WebToolMode; label: string }[] = [
  { value: "disabled", label: tr("langs.webQualityTools.disabled") },
  { value: "automatic", label: tr("langs.webQualityTools.automaticConfiguration") },
  { value: "manual", label: tr("langs.webQualityTools.manualConfiguration") },
];

/** The options of a tool, in the order the page shows them. */
const MANUAL: Record<WebTool, string[]> = {
  eslint: ["path", "config", "workingDirectories", "options"],
  prettier: ["path", "config", "options"],
  stylelint: ["path", "config", "options"],
  biome: ["path", "config", "options"],
};
const EXTRA: Record<WebTool, string[]> = {
  eslint: [],
  prettier: ["onReformat"],
  stylelint: [],
  biome: ["onReformat", "priority"],
};

const modeOf = (tool: WebTool): WebToolMode => (draftValue<string>(key(tool, "mode")) as WebToolMode) ?? "automatic";

/** The configuration as the dialog shows it (not applied yet), for Validate. */
function draftConfig(tool: WebTool): WebToolConfig {
  const text = (k: string) => (findSpec(k) ? (draftValue<string>(k) ?? "") : "");
  return {
    tool,
    mode: modeOf(tool),
    package: text(key(tool, "path")),
    config: text(key(tool, "config")),
    options: text(key(tool, "options")),
    workingDirs: text(key(tool, "workingDirectories")),
    files: text(FILES[tool]),
    onSave: draftValue<boolean>(ON_SAVE[tool]) === true,
    onReformat: draftValue<boolean>(key(tool, "onReformat")) === true,
  };
}

function ValidateRow(props: { tool: WebTool }) {
  const [state, setState] = createSignal<"idle" | "running" | WebValidation>("idle");
  const run = async () => {
    setState("running");
    try {
      setState(await webQualityToolsApi.validate(draftConfig(props.tool)));
    } catch (e) {
      setState({ ok: false, message: String(e).replace(/^Error: /, ""), commandLine: "" });
    }
  };
  const result = () => {
    const s = state();
    return typeof s === "object" ? s : null;
  };
  return (
    <div class="qt-validate">
      <button class="btn-secondary set-btn" disabled={state() === "running"} onClick={() => void run()}>
        {tr("langs.webQualityTools.validate")}
      </button>
      <Show when={state() === "running"}>
        <LoaderCircle size={14} class="qt-spin" />
      </Show>
      <Show when={result()}>
        {(r) => (
          <span class="qt-validation" classList={{ ok: r().ok, failed: !r().ok }} title={r().commandLine}>
            {r().ok ? <CircleCheck size={14} /> : <CircleX size={14} />}
            <span>{r().ok ? tr("langs.webQualityTools.ok", { message: r().message }) : r().message}</span>
          </span>
        )}
      </Show>
    </div>
  );
}

function ModeRow(props: { tool: WebTool }) {
  const k = key(props.tool, "mode");
  return (
    <div class="set-row wqt-modes" classList={{ hit: settingMatches(k), modified: k in draft }} data-setting={k}>
      <For each={MODES}>
        {(m) => (
          <label class="set-check">
            <input type="radio" name={k} checked={modeOf(props.tool) === m.value} onChange={() => setDraft(k, m.value)} />
            <span>
              <Hl text={m.label} />
            </span>
          </label>
        )}
      </For>
    </div>
  );
}

/** What automatic configuration found. */
function Found(props: { info: WebToolInfo | undefined }) {
  return (
    <Show when={props.info}>
      {(info) => (
        <div class="set-desc wqt-found">
          <Show when={info().path} fallback={<span class="qt-missing">{tr("langs.webQualityTools.nodeModulesIsNotInstalled", { package: info().package })}</span>}>
            <span>
              {tr("langs.webQualityTools.package", { path: info().path, value: info().version ? ` (${info().version})` : "" })}
            </span>
          </Show>
          <span class="wqt-sep">·</span>
          <span>{tr("langs.webQualityTools.configuration", { value: info().configFile ?? tr("langs.webQualityTools.noneInTheProjectRoot") })}</span>
        </div>
      )}
    </Show>
  );
}

function ToolBlock(props: { tool: WebTool; info: WebToolInfo | undefined }) {
  const t = props.tool;
  const label = () => props.info?.label ?? t;
  const mode = () => modeOf(t);
  const [open, setOpen] = createSignal(mode() !== "disabled");
  const manualKeys = MANUAL[t].map((o) => key(t, o));
  const keys = [key(t, "mode"), ...manualKeys, FILES[t], ON_SAVE[t], ...EXTRA[t].map((o) => key(t, o))];
  const anyHit = () => keys.some(settingMatches);
  const modified = () => keys.some((k) => k in draft);
  const row = (k: string) => <Show when={findSpec(k)}>{(s) => <SettingRow spec={s()} />}</Show>;
  const status = () => {
    if (mode() === "disabled") return "Disabled";
    if (mode() === "manual") return draftValue<string>(key(t, "path")) || props.info?.path || "Manual configuration";
    return props.info?.path ? `${props.info.path}${props.info.version ? ` ${props.info.version}` : ""}` : null;
  };
  return (
    <section class="qt-tool" classList={{ hit: anyHit(), modified: modified(), on: mode() !== "disabled" }} data-tool={t}>
      <header class="qt-tool-header">
        <button class="icon-btn qt-expand" title={open() ? tr("common.collapse") : tr("common.expand")} onClick={() => setOpen(!open())}>
          {open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </button>
        <span class="qt-tool-name" onClick={() => setOpen(!open())}>
          <Hl text={label()} />
        </span>
        <span class="qt-tool-status">
          <Show when={status()} fallback={<span class="qt-missing">{tr("common.notInstalledInTheProject")}</span>}>
            {(s) => <span title={s()}>{s()}</span>}
          </Show>
        </span>
      </header>
      <Show when={open() || anyHit()}>
        <div class="qt-tool-body">
          <ModeRow tool={t} />
          <Show when={mode() === "automatic"}>
            <Found info={props.info} />
          </Show>
          <Show when={mode() === "manual"}>
            <For each={manualKeys}>{(k) => row(k)}</For>
          </Show>
          <Show when={mode() !== "disabled"}>
            {row(FILES[t])}
            {row(ON_SAVE[t])}
            <For each={EXTRA[t]}>{(o) => row(key(t, o))}</For>
            <ValidateRow tool={t} />
          </Show>
        </div>
      </Show>
    </section>
  );
}

export function WebQualityToolsPage() {
  const [info] = createResource(() => webQualityToolsApi.info().catch(() => undefined));
  const general = ["web.tools.onTheFly", "web.tools.timeout"];
  return (
    <div class="set-page qt-page wqt-page">
      <Show when={findSpec("web.tools.onTheFly")} fallback={<div class="set-empty">{tr("langs.webQualityTools.noSettingsToShowThe")}</div>}>
        <div class="set-desc qt-intro">
          {tr("langs.webQualityTools.eslintPrettierStylelintAndBiome", { value: info()?.node ? ` (${info()!.node})` : "" })}
          <button class="set-link" onClick={() => openSettings("lang.node")}>
            {tr("langs.webQualityTools.nodeJsSettingsEllipsis")}
          </button>
        </div>
        <For each={general}>{(k) => <Show when={findSpec(k)}>{(s) => <SettingRow spec={s()} />}</Show>}</For>
        <div class="qt-tools">
          <For each={ALL_TOOLS}>{(t) => <ToolBlock tool={t} info={info()?.tools.find((x) => x.tool === t)} />}</For>
        </div>
      </Show>
    </div>
  );
}
