/**
 * Floating debugger UI: Incoming Connection From Xdebug, Evaluate, Breakpoints dialog, the
 * breakpoint properties popup of the gutter and the Attach dialogs.
 */
import { createEffect, createMemo, createResource, createSignal, For, on, onMount, Show } from "solid-js";
import { ChevronDown, ChevronRight, CircleDot, Minus, Plus, Zap } from "lucide-solid";
import { searchApi } from "@/ipc/search";
import { formatForKeymap, shortcutLabel } from "@/keymap/store";
import { workspace } from "@/state/workspace";
import { revealLocation } from "@/features/lsp/navigation";
import { debugApi, type Breakpoint, type Property, type SessionInfo } from "./api";
import { basename } from "./format";
import { serverFor, suggestMapping, uriToPath } from "./mapping";
import {
  breakpointPopup,
  breakpointsDialog,
  evaluateRequest,
  setBreakpointPopup,
  setBreakpointsDialog,
  setEvaluateRequest,
  validationDialog,
} from "./overlays";
import { ValidationDialog } from "./ValidationDialog";
import {
  addWatch,
  breakpoints,
  breakpointsMuted,
  incoming,
  pausedSession,
  rememberExpression,
  removeBreakpoint,
  selectedFrame,
  setIncoming,
  toggleMuted,
  updateBreakpoint,
} from "./store";
import { ExpressionInput } from "./ExpressionInput";
import { VariableNode } from "./VariablesTree";
import { AttachDialogs } from "./AttachDialogs";
import { debugAdapters, findDebugAdapter } from "./adapters";

import { DialogFrame, onEscape } from "./DialogFrame";
import { t } from "@/i18n";

// ---- Incoming connection ----------------------------------------------------------------

function IncomingDialog(props: { session: SessionInfo }) {
  const remote = () => uriToPath(props.session.fileUri);
  const [query] = createSignal(basename(remote()));
  const [candidates] = createResource(query, async (q) => {
    const hits = await searchApi.findFiles(q, 50).catch(() => []);
    const rp = remote();
    // Prefer files whose path is a suffix of the server path.
    return hits
      .map((h) => h.path)
      .filter((p) => basename(p) === basename(rp))
      .sort((a, b) => Number(rp.endsWith(`/${b}`)) - Number(rp.endsWith(`/${a}`)) || a.length - b.length);
  });
  const [choice, setChoice] = createSignal<string | null>(null);
  createEffect(on(candidates, (c) => c && c.length && choice() == null && setChoice(c[0])));
  const mapping = createMemo(() => (choice() ? suggestMapping(remote(), choice()!) : null));
  const close = () => {
    void debugApi.ignoreConnection(props.session.id).catch(() => {});
    setIncoming(null);
  };
  const accept = async () => {
    const existing = await debugApi.servers().catch(() => []);
    const server = serverFor(props.session, mapping(), existing);
    setIncoming(null);
    await debugApi.acceptConnection(props.session.id, server).catch((e) => console.warn(e));
  };
  const root = () => workspace()?.root ?? "";
  return (
    <DialogFrame
      title={t("debug.debugOverlays.incomingConnectionFromXdebug")}
      class="dbg-incoming"
      onClose={close}
      footer={
        <>
          <button class="btn-secondary" onClick={close}>
            {t("debug.debugOverlays.ignore")}
          </button>
          <button class="btn-primary" onClick={() => void accept()} disabled={!mapping()}>
            {t("debug.debugOverlays.accept")}
          </button>
        </>
      }
    >
      <div class="dbg-incoming-head">
        <Zap size={18} class="dbg-incoming-icon" />
        <p>
          {t("debug.debugOverlays.unknownServer", { server: props.session.serverName })}
        </p>
      </div>
      <dl class="dbg-kv">
        <dt>{t("debug.debugOverlays.serverName")}</dt>
        <dd>{props.session.serverName === "CLI" ? t("debug.debugOverlays.cliNoServerName") : props.session.host ?? props.session.serverName}</dd>
        <dt>{t("debug.debugOverlays.serverPort")}</dt>
        <dd>{props.session.port ?? "—"}</dd>
        <dt>{t("debug.debugOverlays.filePathOnServer")}</dt>
        <dd class="dbg-mono">{remote()}</dd>
        <dt>{t("debug.debugOverlays.ideKey")}</dt>
        <dd>{props.session.ideKey || "—"}</dd>
      </dl>
      <div class="dbg-section-title">{t("debug.debugOverlays.selectAProjectFileTo")}</div>
      <div class="dbg-candidates" role="listbox">
        <Show when={!candidates.loading} fallback={<div class="muted dbg-pad">{t("debug.debugOverlays.searchingProjectFiles")}</div>}>
          <For each={candidates() ?? []} fallback={<div class="muted dbg-pad">{t("debug.debugOverlays.noProjectFileNamed", { name: basename(remote()) })}</div>}>
            {(p) => (
              <label class="dbg-candidate" classList={{ selected: choice() === p }}>
                <input type="radio" name="dbg-map" checked={choice() === p} onChange={() => setChoice(p)} />
                <span class="dbg-mono">{p}</span>
              </label>
            )}
          </For>
        </Show>
      </div>
      <Show when={mapping()}>
        {(m) => (
          <div class="dbg-mapping-preview">
            <span class="muted">{t("debug.debugOverlays.pathMapping")}</span>
            <span class="dbg-mono">{m().remote}</span>
            <span class="muted">→</span>
            <span class="dbg-mono">{m().local ? `${root()}/${m().local}` : root() || "<project root>"}</span>
          </div>
        )}
      </Show>
    </DialogFrame>
  );
}

// ---- Evaluate ---------------------------------------------------------------------------

function EvaluateDialog(props: { expression: string }) {
  const [expr, setExpr] = createSignal(props.expression);
  const [result, setResult] = createSignal<Property | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  let input: HTMLInputElement | undefined;
  const evaluate = async () => {
    const s = pausedSession();
    const e = expr().trim();
    if (!s || !e) return;
    rememberExpression(e);
    setBusy(true);
    setError(null);
    try {
      setResult(await debugApi.eval(s.id, e, selectedFrame()?.level ?? 0, "repl"));
    } catch (err) {
      setResult(null);
      setError(String(err).replace(/^Error: /, ""));
    } finally {
      setBusy(false);
    }
  };
  onMount(() => {
    input?.focus();
    input?.select();
    if (props.expression.trim()) void evaluate();
  });
  const close = () => setEvaluateRequest(null);
  return (
    <DialogFrame
      title={t("debug.debugOverlays.evaluate")}
      class="dbg-evaluate"
      onClose={close}
      footer={
        <>
          <button
            class="btn-secondary dbg-left"
            disabled={!expr().trim()}
            onClick={() => {
              addWatch(expr());
              close();
            }}
          >
            {t("debug.debugOverlays.addToWatches")}
          </button>
          <button class="btn-secondary" onClick={close}>
            {t("common.close")}
          </button>
        </>
      }
    >
      <label class="dbg-label" for="dbg-eval-expr">
        {t("debug.debugOverlays.expression")}
      </label>
      <div class="dbg-eval-line">
        <ExpressionInput
          id="dbg-eval-expr"
          class="dbg-eval-expr"
          value={expr()}
          onInput={setExpr}
          onSubmit={() => void evaluate()}
          placeholder={pausedSession()?.adapter ? t("debug.debugOverlays.expressionEGCounterOrigin") : "PHP expression, e.g. $order->total * 1.19"}
          ref={(el) => (input = el)}
        />
        <button class="btn-primary" disabled={busy() || !pausedSession()} onClick={() => void evaluate()}>
          {t("debug.debugOverlays.evaluate")}
        </button>
      </div>
      <div class="dbg-label">{t("debug.debugOverlays.result")}</div>
      <div class="dbg-eval-result dbg-tree">
        <Show when={error()}>
          <div class="tree-row dbg-row dbg-error">
            <span class="dbg-error-badge">!</span>
            <span>{error()}</span>
          </div>
        </Show>
        <Show when={result()} keyed>
          {(r) => <VariableNode p={{ ...r, name: "result" }} depth={0} initiallyOpen={r.hasChildren} />}
        </Show>
        <Show when={!result() && !error()}>
          <div class="muted dbg-pad">{busy() ? t("debug.debugOverlays.collectingData") : t("debug.debugOverlays.pressEnterToEvaluate")}</div>
        </Show>
      </div>
    </DialogFrame>
  );
}

// ---- Breakpoints dialog -----------------------------------------------------------------

function bpLabel(bp: Breakpoint): string {
  switch (bp.kind) {
    case "exception":
      return bp.adapter ? (bp.description ?? bp.exception ?? "") : (bp.exception ?? "*");
    case "function":
      return `${bp.function ?? ""}()`;
    case "data":
      return `${bp.description ?? bp.dataId ?? ""} (${bp.accessType ?? "write"})`;
    default:
      return `${basename(bp.file ?? "")}:${bp.line}`;
  }
}

const isPhpFile = (file: string | undefined) => /\.(php|phtml|inc)$/i.test(file ?? "");

const ACCESS_TYPES = [
  { value: "write", label: t("debug.debugOverlays.write") },
  { value: "read", label: t("debug.debugOverlays.read") },
  { value: "readWrite", label: t("debug.debugOverlays.readAndWrite") },
];

/** A field saved when it loses focus or Enter is pressed. */
function LazyInput(props: { id: string; value: string; placeholder?: string; disabled?: boolean; mono?: boolean; onSave: (v: string) => void; inputRef?: (el: HTMLInputElement) => void }) {
  const [v, setV] = createSignal(props.value);
  createEffect(on(() => props.value, (x) => setV(x)));
  return (
    <input
      id={props.id}
      ref={(el) => props.inputRef?.(el)}
      class="dbg-input"
      classList={{ "dbg-mono": props.mono !== false }}
      value={v()}
      placeholder={props.placeholder}
      disabled={props.disabled}
      spellcheck={false}
      onInput={(e) => setV(e.currentTarget.value)}
      onChange={() => props.onSave(v())}
      onKeyDown={(e) => e.key === "Enter" && props.onSave(v())}
    />
  );
}

/** Enabled / Suspend / Condition / Log / Remove once hit / Pass count of a breakpoint. */
export function BreakpointProperties(props: { bp: Breakpoint; compact?: boolean; conditionRef?: (el: HTMLInputElement) => void }) {
  const save = (patch: Partial<Breakpoint>) => void updateBreakpoint({ ...props.bp, ...patch });
  const text = (v: string) => v.trim() || undefined;
  const id = (n: string) => `dbg-bp-${n}-${props.compact ? "pop" : "dlg"}`;
  // An exception filter of a debug adapter: only on or off.
  const filter = () => props.bp.kind === "exception" && !!props.bp.adapter;
  return (
    <Show
      when={!filter()}
      fallback={
        <div class="dbg-bp-props">
          <label class="dbg-check">
            <input type="checkbox" checked={props.bp.enabled} onChange={(e) => save({ enabled: e.currentTarget.checked })} />
            {t("debug.debugOverlays.enabled")}
          </label>
          <p class="muted dbg-hint">
            {findDebugAdapter(props.bp.adapter)?.label ?? props.bp.adapter} suspends the program on "{props.bp.description ?? props.bp.exception}" while
            this is enabled. The filters come from the debug adapter.
          </p>
        </div>
      }
    >
    <div class="dbg-bp-props">
      <label class="dbg-check">
        <input type="checkbox" checked={props.bp.enabled} onChange={(e) => save({ enabled: e.currentTarget.checked })} />
        {t("debug.debugOverlays.enabled")}
      </label>
      <label class="dbg-check">
        <input type="checkbox" checked={props.bp.suspend !== false} onChange={(e) => save({ suspend: e.currentTarget.checked })} />
        {t("debug.debugOverlays.suspendExecution")}
      </label>
      <Show when={props.bp.kind === "exception" && !props.bp.adapter}>
        <label class="dbg-label" for={id("exc")}>
          {t("debug.debugOverlays.exceptionClass")}
        </label>
        <LazyInput id={id("exc")} value={props.bp.exception ?? ""} placeholder={t("debug.debugOverlays.runtimeexceptionOrForAny")} onSave={(v) => save({ exception: v.trim() || "*" })} />
      </Show>
      <Show when={props.bp.kind === "function"}>
        <label class="dbg-label" for={id("fn")}>
          {t("debug.debugOverlays.function")}
        </label>
        <LazyInput id={id("fn")} value={props.bp.function ?? ""} placeholder={t("debug.debugOverlays.mainMyclassMethodStdVector")} onSave={(v) => v.trim() && save({ function: v.trim() })} />
      </Show>
      <Show when={props.bp.kind === "data"}>
        <div class="dbg-form-row">
          <label for={id("access")}>{t("debug.debugOverlays.suspendOn")}</label>
          <select id={id("access")} class="dbg-input" value={props.bp.accessType ?? "write"} onChange={(e) => save({ accessType: e.currentTarget.value })}>
            {ACCESS_TYPES.map((a) => (
              <option value={a.value}>{a.label}</option>
            ))}
          </select>
        </div>
      </Show>
      <Show when={props.bp.kind !== "exception"}>
        <label class="dbg-label" for={id("cond")}>
          {t("debug.debugOverlays.condition")}
        </label>
        <LazyInput
          id={id("cond")}
          inputRef={props.conditionRef}
          value={props.bp.condition ?? ""}
          placeholder={isPhpFile(props.bp.file) ? "e.g. $order->total > 100" : "e.g. counter > 5"}
          onSave={(v) => save({ condition: text(v) })}
        />
      </Show>
      <div class="dbg-bp-group-title">{t("debug.debugOverlays.log")}</div>
      <div class="dbg-bp-log">
        <label class="dbg-check">
          <input type="checkbox" checked={!!props.bp.logMessage} onChange={(e) => save({ logMessage: e.currentTarget.checked })} />
          {t("debug.breakpoints.hitMessage")}
        </label>
        <Show when={!props.compact}>
          <label class="dbg-check">
            <input type="checkbox" checked={!!props.bp.logStack} onChange={(e) => save({ logStack: e.currentTarget.checked })} />
            {t("debug.debugOverlays.stackTrace")}
          </label>
        </Show>
        <label class="dbg-check" for={id("log")}>
          <input
            type="checkbox"
            checked={!!props.bp.logExpression?.trim()}
            onChange={(e) => {
              if (!e.currentTarget.checked) save({ logExpression: undefined });
              else document.getElementById(id("log"))?.focus();
            }}
          />
          {t("debug.debugOverlays.evaluateAndLog")}
        </label>
        <LazyInput
          id={id("log")}
          value={props.bp.logExpression ?? ""}
          placeholder={isPhpFile(props.bp.file) || props.bp.kind === "exception" ? "e.g. $order->status" : "e.g. counter or counter={counter}"}
          onSave={(v) => save({ logExpression: text(v) })}
        />
      </div>
      <Show when={!props.compact}>
        <label class="dbg-check">
          <input type="checkbox" checked={!!props.bp.temporary} onChange={(e) => save({ temporary: e.currentTarget.checked })} />
          {t("debug.debugOverlays.removeOnceHit")}
        </label>
        <div class="dbg-form-row dbg-pass">
          <label for={id("pass")}>{t("debug.debugOverlays.passCount")}</label>
          <input
            id={id("pass")}
            class="dbg-input dbg-port"
            type="number"
            min={0}
            value={props.bp.passCount ?? ""}
            onChange={(e) => {
              const n = Math.floor(Number(e.currentTarget.value));
              save({ passCount: n > 1 ? n : undefined });
            }}
          />
          <span class="muted">{t("debug.debugOverlays.suspendFromThisHitOn")}</span>
        </div>
        <p class="muted dbg-hint">
          <Show
            when={props.bp.suspend === false}
            fallback={
              props.bp.kind === "line" && isPhpFile(props.bp.file)
                ? t("debug.debugOverlays.conditionsAreEvaluatedByXdebug")
                : "Conditions and hit counts are evaluated by the debug adapter. In a logged expression, {expression} is replaced by its value."
            }
          >
            {t("debug.debugOverlays.theProgramDoesNotStop")}
          </Show>
        </p>
      </Show>
    </div>
    </Show>
  );
}

function detailsTitle(bp: Breakpoint): string {
  switch (bp.kind) {
    case "exception":
      return bp.adapter ? `${findDebugAdapter(bp.adapter)?.label ?? bp.adapter} Exception Breakpoint: ${bpLabel(bp)}` : t("debug.debugOverlays.phpExceptionBreakpointBplabel", { bpLabel: bpLabel(bp) });
    case "function":
      return t("debug.debugOverlays.functionBreakpointBplabel", { bpLabel: bpLabel(bp) });
    case "data":
      return t("debug.debugOverlays.watchpointBplabel", { bpLabel: bpLabel(bp) });
    default:
      return `${bp.file}:${bp.line}`;
  }
}

function BreakpointDetails(props: { bp: Breakpoint }) {
  return (
    <div class="dbg-bp-details">
      <div class="dbg-bp-details-title">{detailsTitle(props.bp)}</div>
      <BreakpointProperties bp={props.bp} />
    </div>
  );
}

function BreakpointsDialog(props: { select?: number }) {
  const [selected, setSelected] = createSignal<number | null>(props.select ?? breakpoints()[0]?.id ?? null);
  const [collapsed, setCollapsed] = createSignal<Record<string, boolean>>({});
  const byLocation = (a: Breakpoint, b: Breakpoint) => (a.file ?? "").localeCompare(b.file ?? "") || (a.line ?? 0) - (b.line ?? 0);
  const lines = () => breakpoints().filter((b) => b.kind === "line" && isPhpFile(b.file)).sort(byLocation);
  const otherLines = () => breakpoints().filter((b) => b.kind === "line" && !isPhpFile(b.file)).sort(byLocation);
  const functions = () => breakpoints().filter((b) => b.kind === "function");
  const watchpoints = () => breakpoints().filter((b) => b.kind === "data");
  const exceptions = () => breakpoints().filter((b) => b.kind === "exception" && !b.adapter);
  /** Adapter types with stored exception filters. */
  const filterAdapters = () => [...new Set(breakpoints().filter((b) => b.kind === "exception" && b.adapter).map((b) => b.adapter!))];
  const current = () => breakpoints().find((b) => b.id === selected());
  const close = () => setBreakpointsDialog(null);
  const [adding, setAdding] = createSignal(false);
  const addException = async () => {
    setAdding(false);
    const bp = await debugApi.addBreakpoint({ kind: "exception", exception: "\\RuntimeException" }).catch(() => null);
    if (bp) setSelected(bp.id);
  };
  const addFunction = async () => {
    setAdding(false);
    const bp = await debugApi.addBreakpoint({ kind: "function", function: "main" }).catch(() => null);
    if (bp) {
      setSelected(bp.id);
      queueMicrotask(() => (document.getElementById("dbg-bp-fn-dlg") as HTMLInputElement | null)?.select());
    }
  };
  const remove = () => {
    const id = selected();
    if (id == null) return;
    const all = breakpoints();
    const idx = all.findIndex((b) => b.id === id);
    void removeBreakpoint(id);
    setSelected(all[idx + 1]?.id ?? all[idx - 1]?.id ?? null);
  };
  const group = (key: string, title: string, items: () => Breakpoint[]) => (
    <>
      <div class="tree-row dbg-bp-group" onClick={() => setCollapsed((c) => ({ ...c, [key]: !c[key] }))}>
        <span class="tree-chevron">{collapsed()[key] ? <ChevronRight size={13} /> : <ChevronDown size={13} />}</span>
        <input
          type="checkbox"
          checked={items().length > 0 && items().every((b) => b.enabled)}
          ref={(el) => createEffect(() => (el.indeterminate = items().some((b) => b.enabled) && !items().every((b) => b.enabled)))}
          disabled={!items().length}
          onClick={(e) => e.stopPropagation()}
          onChange={(e) => {
            const on = e.currentTarget.checked;
            for (const b of items()) if (b.enabled !== on) void updateBreakpoint({ ...b, enabled: on });
          }}
        />
        <CircleDot size={13} class="dbg-bp-icon" />
        <span>{title}</span>
      </div>
      <Show when={!collapsed()[key]}>
        <For each={items()}>
          {(b) => (
            <div
              class="tree-row dbg-bp-item"
              classList={{ selected: selected() === b.id, disabled: !b.enabled }}
              onClick={() => setSelected(b.id)}
              onDblClick={() => {
                if (b.file && b.line) {
                  close();
                  void revealLocation(b.file, { line: b.line - 1, character: 0 });
                }
              }}
              data-bp={b.id}
            >
              <input
                type="checkbox"
                checked={b.enabled}
                onClick={(e) => e.stopPropagation()}
                onChange={(e) => void updateBreakpoint({ ...b, enabled: e.currentTarget.checked })}
              />
              <span
                class="dbg-bp-dot"
                classList={{ off: !b.enabled, cond: !!b.condition?.trim(), log: b.suspend === false, temp: !!b.temporary }}
              />
              <span>{bpLabel(b)}</span>
              <Show when={b.file}>
                <span class="muted dbg-bp-dir">{b.file!.split("/").slice(0, -1).join("/")}</span>
              </Show>
            </div>
          )}
        </For>
      </Show>
    </>
  );
  return (
    <DialogFrame
      title={t("debug.debugOverlays.breakpoints")}
      class="dbg-breakpoints"
      onClose={close}
      footer={
        <>
          <label class="dbg-check dbg-left">
            <input type="checkbox" checked={breakpointsMuted()} onChange={() => void toggleMuted()} />
            {t("debug.debugOverlays.muteBreakpoints")}
          </label>
          <button class="btn-primary" onClick={close}>
            {t("debug.debugOverlays.done")}
          </button>
        </>
      }
    >
      <div class="dbg-bp-split">
        <div class="dbg-bp-list">
          <div class="dbg-bp-toolbar">
            <div class="dbg-add-wrap">
              <button class="icon-btn" title={t("common.add")} onClick={() => setAdding((v) => !v)}>
                <Plus size={14} />
              </button>
              <Show when={adding()}>
                <div class="dbg-menu">
                  <Show when={debugAdapters().length > 0}>
                    <button onClick={() => void addFunction()}>{t("debug.debugOverlays.functionBreakpoint")}</button>
                  </Show>
                  <button onClick={() => void addException()}>{t("debug.debugOverlays.phpExceptionBreakpoint")}</button>
                </div>
              </Show>
            </div>
            <button class="icon-btn" title={t("common.remove")} disabled={selected() == null} onClick={remove}>
              <Minus size={14} />
            </button>
          </div>
          <div class="dbg-bp-tree">
            {group("line", t("debug.breakpoints.phpLine"), lines)}
            <Show when={otherLines().length}>{group("lines", t("debug.breakpoints.line"), otherLines)}</Show>
            <Show when={functions().length}>{group("function", t("debug.breakpoints.function"), functions)}</Show>
            <Show when={watchpoints().length}>{group("data", t("debug.breakpoints.watchpoints"), watchpoints)}</Show>
            {group("exception", t("debug.breakpoints.phpException"), exceptions)}
            <For each={filterAdapters()}>
              {(type) =>
                group(`exc-${type}`, t("debug.breakpoints.exception", { adapter: findDebugAdapter(type)?.label ?? type }), () =>
                  breakpoints().filter((b) => b.kind === "exception" && b.adapter === type),
                )
              }
            </For>
          </div>
        </div>
        <div class="dbg-bp-right">
          <Show when={current()} keyed fallback={<div class="muted dbg-pad">{t("debug.debugOverlays.selectABreakpointToEdit")}</div>}>
            {(bp) => <BreakpointDetails bp={bp} />}
          </Show>
        </div>
      </div>
    </DialogFrame>
  );
}

// ---- Gutter popup -----------------------------------------------------------------------

function BreakpointPopupView(props: { id: number; x: number; y: number }) {
  const bp = () => breakpoints().find((b) => b.id === props.id);
  let input: HTMLInputElement | undefined;
  /** Saves a pending field edit (change fires on blur), then closes. */
  const close = () => {
    (document.activeElement as HTMLElement | null)?.blur();
    setBreakpointPopup(null);
  };
  onMount(() => input?.focus());
  const style = () => ({ left: `${Math.min(props.x + 6, window.innerWidth - 380)}px`, top: `${Math.min(props.y - 8, window.innerHeight - 300)}px` });
  return (
    <div class="dbg-popup-layer" onMouseDown={(e) => e.target === e.currentTarget && close()} onKeyDown={onEscape(close)}>
      <Show when={bp()}>
        {(b) => (
          <div class="dbg-bp-popup" style={style()} role="dialog" aria-label={t("debug.debugOverlays.breakpointProperties")}>
            <div class="dbg-bp-popup-title">
              {basename(b().file ?? "")}:{b().line}
              <Show when={b().temporary}>
                <span class="muted"> (removed once hit)</span>
              </Show>
            </div>
            <BreakpointProperties bp={b()} compact conditionRef={(el) => (input = el)} />
            <div class="dbg-bp-popup-actions">
              <button
                class="dbg-link"
                onClick={() => {
                  close();
                  setBreakpointsDialog({ select: b().id });
                }}
              >
                More ({formatForKeymap(shortcutLabel("ViewBreakpoints") || "Ctrl+Shift+F8")})
              </button>
              <button class="btn-primary" onClick={close}>
                {t("debug.debugOverlays.done")}
              </button>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}

export function DebugOverlays() {
  return (
    <>
      <Show when={breakpointPopup()} keyed>
        {(p) => <BreakpointPopupView id={p.id} x={p.x} y={p.y} />}
      </Show>
      <Show when={evaluateRequest()} keyed>
        {(r) => <EvaluateDialog expression={r.expression} />}
      </Show>
      <Show when={breakpointsDialog()} keyed>
        {(d) => <BreakpointsDialog select={d.select} />}
      </Show>
      <Show when={incoming()} keyed>
        {(s) => <IncomingDialog session={s} />}
      </Show>
      <Show when={validationDialog()}>
        <ValidationDialog />
      </Show>
      <AttachDialogs />
    </>
  );
}
