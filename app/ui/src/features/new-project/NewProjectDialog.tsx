/**
 * New Project dialog, laid out like PHPStorm's: generators grouped by pack on the left,
 * the form for the selected one on the right, Create / Cancel at the bottom. While a
 * project is created the form turns into a progress view with the tool output.
 */
import { createEffect, For, Match, onCleanup, Show, Switch } from "solid-js";
import {
  CircleAlert,
  CircleCheck,
  CircleHelp,
  Folder,
  LoaderCircle,
  RefreshCw,
  TerminalSquare,
  TriangleAlert,
} from "lucide-solid";
import { isTauri } from "@/ipc";
import type { GeneratorOption } from "@/ipc/project";
import { ProjectTypeIcon } from "@/shell/ProjectTypeIcon";
import { onDialogCancel } from "@/shell/window/dialogs";
import {
  browseLocation,
  canCreate,
  cancelCreation,
  closeNewProject,
  createProject,
  generators,
  isOpen,
  loadError,
  location,
  name,
  NEW_PROJECT_DIALOG,
  options,
  plan,
  progress,
  refreshError,
  refreshing,
  refreshOption,
  selectGenerator,
  selected,
  selectedId,
  setDialogShown,
  setLocation,
  setName,
  setOption,
  useFallback,
} from "./store";
import { groupGenerators, isOptionVisible, joinPath, previewLines, progressPercent, toolName, validateName } from "./wizard";
import { t } from "@/i18n";

function OptionField(props: { option: GeneratorOption }) {
  const value = () => options()[props.option.id] ?? props.option.default;
  const segmented = () =>
    (props.option.choices?.length ?? 0) <= 4 && (props.option.choices ?? []).every((c) => c.label.length <= 14);
  return (
    <Switch>
      <Match when={props.option.kind === "bool"}>
        <label class="np-check">
          <input
            type="checkbox"
            checked={value() === true}
            onChange={(e) => setOption(props.option.id, e.currentTarget.checked)}
          />
          <span>
            {props.option.label}
            <Show when={props.option.description}>
              <span class="np-check-hint">{props.option.description}</span>
            </Show>
          </span>
        </label>
      </Match>
      <Match when={props.option.kind === "enum"}>
        <div class="np-row">
          <span class="np-label">{props.option.label}:</span>
          <Show
            when={segmented()}
            fallback={
              <div class="np-select-row">
                <select
                  class="np-select"
                  value={String(value())}
                  data-option={props.option.id}
                  onChange={(e) => setOption(props.option.id, e.currentTarget.value)}
                >
                  <For each={props.option.choices}>{(c) => <option value={c.value}>{c.label}</option>}</For>
                </select>
                <Show when={props.option.refresh}>
                  <button
                    class="np-browse np-refresh"
                    classList={{ spinning: refreshing() === props.option.id }}
                    title={props.option.description ?? "Refresh"}
                    aria-label={`Refresh ${props.option.label}`}
                    disabled={!!refreshing()}
                    onClick={() => void refreshOption(props.option.id)}
                  >
                    <RefreshCw size={14} />
                  </button>
                </Show>
              </div>
            }
          >
            <div class="np-segmented" role="radiogroup" aria-label={props.option.label}>
              <For each={props.option.choices}>
                {(c) => (
                  <button
                    role="radio"
                    aria-checked={value() === c.value}
                    classList={{ active: value() === c.value }}
                    onClick={() => setOption(props.option.id, c.value)}
                  >
                    {c.label}
                  </button>
                )}
              </For>
            </div>
          </Show>
        </div>
        <Show when={refreshError()[props.option.id]}>
          <div class="np-row np-hint-row">
            <span class="np-label" />
            <span class="np-hint np-error">
              <CircleAlert size={13} /> {refreshError()[props.option.id]}
            </span>
          </div>
        </Show>
      </Match>
      <Match when={props.option.kind === "string"}>
        <div class="np-row">
          <span class="np-label">{props.option.label}:</span>
          <input
            class="np-input"
            value={String(value())}
            placeholder={props.option.placeholder}
            data-option={props.option.id}
            spellcheck={false}
            onInput={(e) => setOption(props.option.id, e.currentTarget.value)}
          />
        </div>
      </Match>
    </Switch>
  );
}

function Form() {
  const nameError = () => validateName(name());
  const formError = () => nameError() ?? plan()?.error;
  const visible = () => {
    const gen = selected();
    return gen ? gen.options.filter((o) => isOptionVisible(o, options(), gen)) : [];
  };
  const checks = () => visible().filter((o) => o.kind === "bool");
  const fields = () => visible().filter((o) => o.kind !== "bool");
  const missing = () => plan()?.missingTools ?? [];
  const fallbackName = () => generators().find((g) => g.id === plan()?.fallback)?.name;
  let nameEl!: HTMLInputElement;
  createEffect(() => {
    if (isOpen() && selectedId()) queueMicrotask(() => nameEl?.focus());
  });
  return (
    <div class="np-form">
      <div class="np-heading">
        <ProjectTypeIcon type={selected()?.icon ?? "folder"} size={30} />
        <div>
          <div class="np-title">{selected()?.name}</div>
          <div class="np-desc">{selected()?.description}</div>
        </div>
      </div>

      <div class="np-row">
        <span class="np-label">{t("newProject.name")}</span>
        <input
          ref={nameEl}
          class="np-input"
          classList={{ invalid: !!nameError() }}
          value={name()}
          spellcheck={false}
          onInput={(e) => setName(e.currentTarget.value)}
        />
      </div>
      <div class="np-row">
        <span class="np-label">{t("newProject.location")}</span>
        <div class="np-path">
          <input
            class="np-input"
            value={location()}
            spellcheck={false}
            onInput={(e) => setLocation(e.currentTarget.value)}
          />
          <button
            class="np-browse"
            title={isTauri() ? t("newProject.browse", { shortcut: "Shift+Enter" }) : t("newProject.browseUnavailable")}
            disabled={!isTauri()}
            onClick={() => void browseLocation()}
          >
            <Folder size={15} />
          </button>
        </div>
      </div>
      <div class="np-row np-hint-row">
        <span class="np-label" />
        <Show
          when={formError()}
          fallback={
            <span class="np-hint">
              {t("newProject.createdIn")} <span class="np-hint-path">{joinPath(location(), name())}</span>
            </span>
          }
        >
          <span class="np-hint np-error">
            <CircleAlert size={13} /> {formError()}
          </span>
        </Show>
      </div>

      <Show when={fields().length || checks().length}>
        <div class="np-sep" />
        <For each={fields()}>{(o) => <OptionField option={o} />}</For>
        <Show when={checks().length}>
          <div class="np-row np-checks-row">
            <span class="np-label" />
            <div class="np-checks">
              <For each={checks()}>{(o) => <OptionField option={o} />}</For>
            </div>
          </div>
        </Show>
      </Show>

      <Show when={missing().length}>
        <div class="np-warning">
          <TriangleAlert size={15} class="np-warning-icon" />
          <div>
            <div>
              {t("newProject.missingTools", { tools: missing().map(toolName).join(", "), count: missing().length })}
              <Show when={fallbackName()} fallback=".">
                {t("newProject.orOffline")}
              </Show>
            </div>
            <Show when={fallbackName()}>
              <button class="np-link" onClick={useFallback}>
                {t("newProject.useFallback", { name: fallbackName() })}
              </button>
            </Show>
          </div>
        </div>
      </Show>

      <div class="np-preview">
        <div class="np-preview-title">
          <TerminalSquare size={13} /> {selected()?.tools.length ? t("newProject.commands") : t("newProject.steps")}
        </div>
        <pre class="np-preview-body">
          <For each={previewLines(plan(), location())}>
            {(line) => (
              <div class={`np-preview-${line.kind}`}>
                <Show when={line.kind !== "file"} fallback={<span class="np-file-indent">{"  "}</span>}>
                  <span class="np-prompt">{line.kind === "template" ? "+" : "$"}</span>{" "}
                </Show>
                {line.text}
              </div>
            )}
          </For>
        </pre>
      </div>
    </div>
  );
}

function ProgressView() {
  const p = () => progress()!;
  let log!: HTMLDivElement;
  createEffect(() => {
    void p().lines.length;
    queueMicrotask(() => log && (log.scrollTop = log.scrollHeight));
  });
  return (
    <div class="np-progress">
      <div class="np-progress-head">
        <Switch>
          <Match when={p().phase === "running"}>
            <LoaderCircle size={18} class="np-spin" />
          </Match>
          <Match when={p().phase === "done"}>
            <CircleCheck size={18} class="np-ok" />
          </Match>
          <Match when={p().phase === "failed"}>
            <CircleAlert size={18} class="np-fail" />
          </Match>
        </Switch>
        <div class="np-progress-title">
          {p().phase === "failed"
            ? t("newProject.failed", { name: name() })
            : p().phase === "done"
              ? t("newProject.ready", { name: name() })
              : t("newProject.creating", { name: name() })}
        </div>
      </div>
      <div class="np-progress-step">
        <Show when={p().total > 1}>
          <span class="muted">
            {t("newProject.step", { step: Math.min(p().step + 1, p().total), total: p().total })}{" "}
          </span>
        </Show>
        <span class="np-progress-label">{p().label}</span>
      </div>
      <div class="np-bar" classList={{ failed: p().phase === "failed" }}>
        <div class="np-bar-fill" style={{ width: `${progressPercent(p())}%` }} />
      </div>
      <div class="np-log" ref={log}>
        <For each={p().lines}>
          {(l) => <div classList={{ "np-log-cmd": l.text.startsWith("$ "), "np-log-err": l.stderr }}>{l.text}</div>}
        </For>
      </div>
      <Show when={p().phase === "failed"}>
        <div class="np-error np-progress-error">
          <CircleAlert size={13} /> {p().error}
        </div>
      </Show>
    </div>
  );
}

/** Content of the New Project dialog (the frame comes from `WindowFrame`). */
export function NewProjectDialog() {
  const groups = () => groupGenerators(generators());
  setDialogShown(true);
  onCleanup(() => setDialogShown(false));
  // Title bar close button, click outside, Alt+F4: like Cancel (not while creating).
  onCleanup(onDialogCancel(NEW_PROJECT_DIALOG, closeNewProject));
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      if (progress()) cancelCreation();
      else closeNewProject();
    } else if (e.key === "Enter" && !progress() && !(e.target instanceof HTMLButtonElement)) {
      e.preventDefault();
      if (e.shiftKey) void browseLocation();
      else void createProject();
    }
  };
  return (
    <div class="np-dialog" onKeyDown={onKeyDown}>
      <div class="np-body">
        <aside class="np-sidebar">
          <For each={groups()}>
            {(g) => (
              <div class="np-group">
                <div class="np-group-title">{g.group}</div>
                <For each={g.items}>
                  {(gen) => (
                    <button
                      class="np-item"
                      classList={{ selected: gen.id === selectedId() }}
                      data-generator={gen.id}
                      disabled={!!progress() && gen.id !== selectedId()}
                      onClick={() => selectGenerator(gen.id)}
                    >
                      <ProjectTypeIcon type={gen.icon} size={18} />
                      <span class="np-item-name">{gen.name}</span>
                    </button>
                  )}
                </For>
              </div>
            )}
          </For>
        </aside>
        <main class="np-main">
          <Show when={!loadError()} fallback={<div class="np-error np-load-error">{loadError()}</div>}>
            <Show when={progress()} fallback={<Form />}>
              <ProgressView />
            </Show>
          </Show>
        </main>
      </div>
      <footer class="np-footer">
        <button class="icon-btn" title={t("common.help")}>
          <CircleHelp size={16} />
        </button>
        <div class="np-footer-buttons">
          <Show
            when={progress()}
            fallback={
              <>
                <button class="btn-primary np-btn" disabled={!canCreate()} onClick={() => void createProject()}>
                  {t("newProject.create")}
                </button>
                <button class="btn-secondary np-btn" onClick={closeNewProject}>
                  {t("common.cancel")}
                </button>
              </>
            }
          >
            <Show
              when={progress()?.phase === "failed"}
              fallback={
                <button
                  class="btn-secondary np-btn"
                  disabled={progress()?.phase !== "running"}
                  onClick={cancelCreation}
                >
                  {t("common.cancel")}
                </button>
              }
            >
              <button class="btn-secondary np-btn" onClick={cancelCreation}>
                {t("newProject.back")}
              </button>
              <button class="btn-secondary np-btn" onClick={closeNewProject}>
                {t("common.close")}
              </button>
            </Show>
          </Show>
        </div>
      </footer>
    </div>
  );
}
