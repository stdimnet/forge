/**
 * Settings | Languages & Frameworks | PHP | Quality Tools, like PHPStorm: one block per tool
 * with its On/Off switch (the inspection), where it was found, and when expanded its
 * configuration: run with (directly or a PHP interpreter), executable, configuration file,
 * level / ruleset, other parameters, timeout and severity, with Validate. Values go through
 * the dialog draft, so Apply, Cancel and Reset work.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { ChevronDown, ChevronRight, CircleCheck, CircleX, LoaderCircle } from "lucide-solid";
import { qualityToolsApi, type QualityTool, type QualityToolInfo, type ToolConfig, type Validation } from "@/ipc/qualityTools";
import { findSpec } from "@/state/settings";
import { draft, draftValue, setDraft } from "@/features/settings/dialogState";
import { Hl, settingMatches } from "@/features/settings/dialogSearch";
import { SettingRow } from "@/features/settings/SchemaPage";
import { ALL_TOOLS } from "./run";

const key = (tool: QualityTool, option: string) => `php.tools.${tool}.${option}`;

/** The configuration as the dialog shows it (not applied yet), for Validate. */
function draftConfig(tool: QualityTool): ToolConfig {
  const text = (o: string) => (findSpec(key(tool, o)) ? (draftValue<string>(key(tool, o)) ?? "") : "");
  return {
    tool,
    inspection: draftValue<boolean>(key(tool, "inspection")) === true,
    path: text("path"),
    fixerPath: tool === "phpcs" ? text("phpcbfPath") : "",
    interpreter: text("interpreter"),
    config: text("config"),
    level: text("level"),
    options: text("options"),
    timeoutSecs: draftValue<number>(key(tool, "timeout")) ?? 30,
    severity: (draftValue<string>(key(tool, "severity")) as ToolConfig["severity"]) ?? "tool",
  };
}

/** "Run with": directly, the project interpreter or a configured one. */
function InterpreterRow(props: { tool: QualityTool; interpreters: string[] }) {
  const k = key(props.tool, "interpreter");
  const value = () => draftValue<string>(k) ?? "";
  const known = () => ["", "project", ...props.interpreters].includes(value());
  return (
    <div class="set-row" classList={{ hit: settingMatches(k), modified: k in draft }} data-setting={k}>
      <div class="set-line">
        <label class="set-label">
          <Hl text="Run with" />:
        </label>
        <select class="set-input set-select qt-interpreter" value={value()} onChange={(e) => setDraft(k, e.currentTarget.value)}>
          <option value="">Directly (where the project is)</option>
          <option value="project">Project PHP interpreter</option>
          <For each={props.interpreters}>{(name) => <option value={name}>{name}</option>}</For>
          <Show when={!known()}>
            <option value={value()}>{value()} (not configured)</option>
          </Show>
        </select>
      </div>
      <div class="set-desc">
        Interpreters run the tool where their PHP is and map paths with their path mappings.
      </div>
    </div>
  );
}

function ValidateRow(props: { tool: QualityTool }) {
  const [state, setState] = createSignal<"idle" | "running" | Validation>("idle");
  const run = async () => {
    setState("running");
    try {
      setState(await qualityToolsApi.validate(draftConfig(props.tool)));
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
        Validate
      </button>
      <Show when={state() === "running"}>
        <LoaderCircle size={14} class="qt-spin" />
      </Show>
      <Show when={result()}>
        {(r) => (
          <span class="qt-validation" classList={{ ok: r().ok, failed: !r().ok }} title={r().commandLine}>
            {r().ok ? <CircleCheck size={14} /> : <CircleX size={14} />}
            <span>{r().ok ? `OK, ${r().message}` : r().message}</span>
          </span>
        )}
      </Show>
    </div>
  );
}

function ToolBlock(props: { tool: QualityTool; info: QualityToolInfo | undefined; interpreters: string[] }) {
  const t = props.tool;
  const on = () => draftValue<boolean>(key(t, "inspection")) === true;
  const [open, setOpen] = createSignal(on());
  const label = () => props.info?.label ?? t;
  const spec = (o: string) => findSpec(key(t, o));
  const rows = ["path", ...(t === "phpcs" ? ["phpcbfPath"] : []), "config", "level", "options", "timeout", "severity"];
  const anyHit = () => ["inspection", "interpreter", ...rows].some((o) => settingMatches(key(t, o)));
  const modified = () => ["inspection", "interpreter", ...rows].some((o) => key(t, o) in draft);
  return (
    <section class="qt-tool" classList={{ hit: anyHit(), modified: modified(), on: on() }} data-tool={t}>
      <header class="qt-tool-header">
        <button class="icon-btn qt-expand" title={open() ? "Collapse" : "Expand"} onClick={() => setOpen(!open())}>
          {open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </button>
        <span class="qt-tool-name" onClick={() => setOpen(!open())}>
          <Hl text={label()} />
        </span>
        <span class="qt-tool-status">
          <Show when={props.info}>
            {(info) => (
              <Show when={info().path} fallback={<span class="qt-missing">Not installed in the project</span>}>
                {(p) => <span title={p()}>{p()}</span>}
              </Show>
            )}
          </Show>
        </span>
        <label class="qt-switch" title={on() ? "Inspection is on" : "Inspection is off"} data-setting={key(t, "inspection")}>
          <input type="checkbox" checked={on()} onChange={(e) => setDraft(key(t, "inspection"), e.currentTarget.checked)} />
          <span class="qt-switch-track">
            <span class="qt-switch-thumb" />
          </span>
          <span class="qt-switch-label">{on() ? "ON" : "OFF"}</span>
        </label>
      </header>
      <Show when={open() || anyHit()}>
        <div class="qt-tool-body">
          <Show when={props.info?.configFile && !draftValue<string>(key(t, "config"))}>
            <div class="set-desc qt-found">Configuration file: {props.info!.configFile}</div>
          </Show>
          <InterpreterRow tool={t} interpreters={props.interpreters} />
          <For each={rows}>{(o) => <Show when={spec(o)}>{(s) => <SettingRow spec={s()} />}</Show>}</For>
          <ValidateRow tool={t} />
        </div>
      </Show>
    </section>
  );
}

export function QualityToolsPage() {
  const [info] = createResource(() => qualityToolsApi.info().catch(() => undefined));
  const onTheFly = () => findSpec("php.tools.onTheFly");
  return (
    <div class="set-page qt-page">
      <Show
        when={onTheFly()}
        fallback={<div class="set-empty">No settings to show. The PHP plugin may be disabled.</div>}
      >
        {(s) => (
          <>
            <div class="set-desc qt-intro">
              The tools come from the project (vendor/bin) or a path, and run where the project lives or with a PHP
              interpreter. Their problems show in the editor, the Problems tool window and Code | Inspect Code with Quality
              Tools. They run the project's code, so Forge never runs them in Safe Mode.
            </div>
            <SettingRow spec={s()} />
            <div class="qt-tools">
              <For each={ALL_TOOLS}>
                {(t) => (
                  <ToolBlock tool={t} info={info()?.tools.find((x) => x.tool === t)} interpreters={info()?.interpreters ?? []} />
                )}
              </For>
            </div>
          </>
        )}
      </Show>
    </div>
  );
}
