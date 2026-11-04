/**
 * The dialogs of Inspect Code, Run Inspection by Name and Code Cleanup, like PHPStorm's:
 * "Specify Inspection Scope" (whole project, module, file, directory, custom scope; the
 * profile), the Run Inspection by Name popup and the Code Cleanup preview with a diff per
 * file.
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { Search } from "lucide-solid";
import type { CleanupPlan, InspectionSpec, InspectRequest, InspectScope } from "@/ipc/inspections";
import { activeTab } from "@/state/editor";
import { workspace } from "@/state/workspace";
import { openSettings } from "@/features/settings/dialogState";
import { ScopeSelect } from "@/features/scopes/ScopeSelect";
import { scopeFilter } from "@/features/scopes/model";
import { InlineDiff } from "@/features/git/InlineDiff";
import { INSPECTIONS_PAGE } from "./draft";
import { findInspections, loadInspections, profiles } from "./model";
import { t } from "@/i18n";

export interface ScopeDialogRequest {
  title: string;
  /** Text of the OK button (`Analyze`, `OK`). */
  confirm: string;
  /** Run Inspection by Name: the inspection, instead of a profile. */
  inspection?: InspectionSpec;
  done: (request: Omit<InspectRequest, "runId"> | null) => void;
}

const [scopeDialog, setScopeDialog] = createSignal<ScopeDialogRequest | null>(null);
const [byNamePopup, setByNamePopup] = createSignal<((spec: InspectionSpec | null) => void) | null>(null);
const [cleanupPreview, setCleanupPreview] = createSignal<{ plan: CleanupPlan; done: (apply: boolean) => void } | null>(null);

export { scopeDialog, byNamePopup, cleanupPreview };

export function askScope(req: Omit<ScopeDialogRequest, "done">): Promise<Omit<InspectRequest, "runId"> | null> {
  return new Promise((resolve) =>
    setScopeDialog({
      ...req,
      done: (r) => {
        setScopeDialog(null);
        resolve(r);
      },
    }),
  );
}

export function askInspectionByName(): Promise<InspectionSpec | null> {
  return new Promise((resolve) =>
    setByNamePopup(() => (spec: InspectionSpec | null) => {
      setByNamePopup(null);
      resolve(spec);
    }),
  );
}

export function askCleanup(plan: CleanupPlan): Promise<boolean> {
  return new Promise((resolve) =>
    setCleanupPreview({
      plan,
      done: (apply) => {
        setCleanupPreview(null);
        resolve(apply);
      },
    }),
  );
}

const dirOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "");

type Kind = "project" | "module" | "file" | "directory" | "scope";

function keys(e: KeyboardEvent, ok: () => void, cancel: () => void): void {
  if (e.key === "Escape") cancel();
  else if (e.key === "Enter" && !(e.target instanceof HTMLSelectElement)) ok();
  else return;
  e.preventDefault();
  e.stopPropagation();
}

export function ScopeDialog(props: { req: ScopeDialogRequest }) {
  const file = () => activeTab()?.path;
  const [kind, setKind] = createSignal<Kind>("project");
  const [scopeName, setScopeName] = createSignal("Project Files");
  const [tests, setTests] = createSignal(true);
  const [profile, setProfile] = createSignal(profiles()?.selected ?? "");
  onMount(() => {
    if (!profiles()) void loadInspections().then(() => setProfile(profiles()?.selected ?? ""));
  });
  const project = () => workspace()?.name ?? "project";
  const scope = (): InspectScope | null => {
    switch (kind()) {
      case "project":
        return { kind: "project", includeTests: tests(), label: t("tools.inspections.project", { project: project() }) };
      case "module":
        return { kind: "module", includeTests: tests(), label: t("tools.inspections.module", { project: project() }) };
      case "file":
        return file() ? { kind: "file", path: file(), label: t("tools.inspections.file", { file: file() }) } : null;
      case "directory": {
        const d = dirOf(file() ?? "");
        return { kind: "directory", path: d, includeTests: tests(), label: t("tools.inspections.directory", { value: d || project() }) };
      }
      case "scope": {
        const filter = scopeFilter(scopeName());
        return filter ? { kind: "scope", filter, label: t("tools.inspections.scope", { scopeName: scopeName() }) } : null;
      }
    }
  };
  const ok = () => {
    const s = scope();
    if (!s) return;
    props.req.done({ scope: s, profile: props.req.inspection ? undefined : profile(), only: props.req.inspection ? [props.req.inspection.id] : undefined });
  };
  const cancel = () => props.req.done(null);
  const radio = (k: Kind, label: string, disabled = false) => (
    <label class="insp-radio" classList={{ disabled }}>
      <input type="radio" name="insp-scope" checked={kind() === k} disabled={disabled} onChange={() => setKind(k)} />
      {label}
    </label>
  );
  return (
    <div class="popup-backdrop modal">
      <div class="dialog insp-scope-dialog" role="dialog" aria-label={props.req.title} onKeyDown={(e) => keys(e, ok, cancel)}>
        <div class="dialog-title">{props.req.title}</div>
        <div class="dialog-body">
          <fieldset class="insp-scope-group">
            <legend>{t("tools.inspections.inspectionScope")}</legend>
            {radio("project", "Whole project")}
            {radio("module", `Module '${project()}'`)}
            {radio("file", file() ? `File '${file()}'` : "File", !file())}
            {radio("directory", `Directory '${dirOf(file() ?? "") || project()}'`, !file())}
            <div class="insp-custom-scope">
              {radio("scope", "Custom scope:")}
              <ScopeSelect value={scopeName()} onChange={(n) => (setScopeName(n), setKind("scope"))} ariaLabel="Custom scope" />
            </div>
            <label class="insp-radio insp-tests" classList={{ disabled: kind() === "file" || kind() === "scope" }}>
              <input type="checkbox" checked={tests()} disabled={kind() === "file" || kind() === "scope"} onChange={(e) => setTests(e.currentTarget.checked)} />
              {t("tools.inspections.includeTestSources")}
            </label>
          </fieldset>
          <Show
            when={!props.req.inspection}
            fallback={
              <div class="insp-by-name-info">
                <span class="insp-form-label">{t("tools.inspections.inspectionLabel")}</span> <strong>{props.req.inspection!.name}</strong>
                <span class="insp-muted"> ({props.req.inspection!.group.replace(/\//g, " | ")})</span>
              </div>
            }
          >
            <div class="insp-profile-row">
              <label class="insp-form-label" for="insp-run-profile">
                {t("tools.inspections.inspectionProfileLabel")}
              </label>
              <select id="insp-run-profile" class="set-input set-select" value={profile()} onChange={(e) => setProfile(e.currentTarget.value)}>
                <For each={profiles()?.profiles ?? []}>
                  {(p) => (
                    <option value={p.name}>
                      {p.name} {p.level === "project" ? t("tools.inspections.project2") : t("tools.inspections.ide")}
                    </option>
                  )}
                </For>
              </select>
              <button class="link-btn" onClick={() => (cancel(), openSettings(INSPECTIONS_PAGE))}>
                {t("common.configureEllipsis")}
              </button>
            </div>
          </Show>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={cancel}>
            {t("common.cancel")}
          </button>
          <button class="btn-primary" disabled={!scope()} onClick={ok}>
            {props.req.confirm}
          </button>
        </div>
      </div>
    </div>
  );
}

export function RunByNamePopup(props: { done: (spec: InspectionSpec | null) => void }) {
  const [q, setQ] = createSignal("");
  const [index, setIndex] = createSignal(0);
  const list = createMemo(() => findInspections(q()));
  onMount(() => {
    if (!profiles()) void loadInspections();
  });
  const pick = (i = index()) => {
    const s = list()[i];
    if (s) props.done(s);
  };
  return (
    <div class="popup-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.done(null)}>
      <div
        class="popup insp-by-name"
        role="dialog"
        aria-label={t("tools.inspections.runInspectionByName")}
        onKeyDown={(e) => {
          if (e.key === "Escape") props.done(null);
          else if (e.key === "Enter") pick();
          else if (e.key === "ArrowDown") setIndex((i) => Math.min(i + 1, list().length - 1));
          else if (e.key === "ArrowUp") setIndex((i) => Math.max(i - 1, 0));
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="insp-by-name-title">{t("tools.inspections.enterInspectionNameLabel")}</div>
        <div class="search-field">
          <Search size={14} class="muted" />
          <input
            ref={(el) => queueMicrotask(() => el.focus())}
            value={q()}
            onInput={(e) => {
              setQ(e.currentTarget.value);
              setIndex(0);
            }}
            spellcheck={false}
            aria-label={t("tools.inspections.inspectionName")}
          />
        </div>
        <div class="insp-by-name-list" role="listbox">
          <For each={list()} fallback={<div class="insp-empty">{t("tools.inspections.noInspectionsFound")}</div>}>
            {(s, i) => (
              <div
                class="insp-by-name-item"
                classList={{ selected: i() === index() }}
                role="option"
                aria-selected={i() === index()}
                onMouseEnter={() => setIndex(i())}
                onClick={() => pick(i())}
              >
                <span>{s.name}</span>
                <span class="insp-muted">{s.group.replace(/\//g, " | ")}</span>
              </div>
            )}
          </For>
        </div>
      </div>
    </div>
  );
}

export function CleanupPreview(props: { plan: CleanupPlan; done: (apply: boolean) => void }) {
  const [file, setFile] = createSignal(props.plan.files[0]?.path ?? "");
  const current = () => props.plan.files.find((f) => f.path === file());
  const total = () => props.plan.files.reduce((n, f) => n + f.fixes.length, 0);
  return (
    <div class="popup-backdrop modal">
      <div class="dialog insp-cleanup" role="dialog" aria-label={t("tools.inspections.codeCleanup")} onKeyDown={(e) => e.key === "Escape" && props.done(false)}>
        <div class="dialog-title">
          {t("tools.inspections.codeCleanupFixInFile", { total: total(), value: total(), count: props.plan.files.length, value2: props.plan.files.length })}
        </div>
        <div class="insp-cleanup-body">
          <div class="insp-cleanup-files">
            <For each={props.plan.files} fallback={<div class="insp-empty">{t("tools.inspections.nothingToCleanUp")}</div>}>
              {(f) => (
                <div class="insp-cleanup-file" classList={{ selected: f.path === file() }} onClick={() => setFile(f.path)}>
                  <div>{f.path.split("/").pop()}</div>
                  <div class="insp-muted">
                    {t("tools.inspections.fixes", { count: f.fixes.length, path: f.path })}
                  </div>
                </div>
              )}
            </For>
          </div>
          <div class="insp-cleanup-diff">
            <Show when={current()} keyed>
              {(f) => (
                <>
                  <div class="insp-cleanup-fixes">{[...new Set(f.fixes)].join(", ")}</div>
                  <InlineDiff path={f.path} left={{ title: t("tools.inspections.before"), content: f.before }} right={{ title: t("tools.inspections.afterCodeCleanup"), content: f.after }} />
                </>
              )}
            </Show>
          </div>
        </div>
        <div class="dialog-buttons">
          <Show when={props.plan.skipped}>
            <span class="dialog-checkbox insp-muted">{t("tools.inspections.overlappingFixesLeftOut", { skipped: props.plan.skipped })}</span>
          </Show>
          <button class="btn-secondary" onClick={() => props.done(false)}>
            {t("common.cancel")}
          </button>
          <button class="btn-primary" disabled={!props.plan.files.length} onClick={() => props.done(true)}>
            {t("common.apply")}
          </button>
        </div>
      </div>
    </div>
  );
}
