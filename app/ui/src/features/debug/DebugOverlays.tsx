/**
 * Floating debugger UI: Incoming Connection From Xdebug, Evaluate, Breakpoints dialog and
 * the breakpoint properties popup of the gutter.
 */
import { createEffect, createMemo, createResource, createSignal, For, on, onMount, Show } from "solid-js";
import { ChevronDown, ChevronRight, CircleDot, Minus, Plus, Zap, X } from "lucide-solid";
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
} from "./overlays";
import { addWatch, breakpoints, breakpointsMuted, incoming, pausedSession, removeBreakpoint, selectedFrame, setIncoming, toggleMuted, updateBreakpoint } from "./store";
import { VariableNode } from "./VariablesTree";

/** Escape closes the dialog it was pressed in (element-scoped, not a global shortcut). */
const onEscape = (close: () => void) => (e: KeyboardEvent) => {
  if (e.key !== "Escape") return;
  e.preventDefault();
  e.stopPropagation();
  close();
};

function DialogFrame(props: { title: string; class?: string; onClose: () => void; children: unknown; footer: unknown }) {
  return (
    <div class="dbg-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()} onKeyDown={onEscape(props.onClose)}>
      <div class={`dbg-dialog ${props.class ?? ""}`} role="dialog" aria-label={props.title}>
        <header class="dbg-dialog-title">
          <span>{props.title}</span>
          <button class="icon-btn" title="Close" onClick={props.onClose}>
            <X size={14} />
          </button>
        </header>
        <div class="dbg-dialog-body">{props.children as never}</div>
        <footer class="dbg-dialog-actions">{props.footer as never}</footer>
      </div>
    </div>
  );
}

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
      title="Incoming Connection From Xdebug"
      class="dbg-incoming"
      onClose={close}
      footer={
        <>
          <button class="btn-secondary" onClick={close}>
            Ignore
          </button>
          <button class="btn-primary" onClick={() => void accept()} disabled={!mapping()}>
            Accept
          </button>
        </>
      }
    >
      <div class="dbg-incoming-head">
        <Zap size={18} class="dbg-incoming-icon" />
        <p>
          Xdebug from <b>{props.session.serverName}</b> wants to start a debug session, but no server with this name is configured
          and the script is not part of the project.
        </p>
      </div>
      <dl class="dbg-kv">
        <dt>Server name:</dt>
        <dd>{props.session.serverName === "CLI" ? "CLI (no SERVER_NAME)" : props.session.host ?? props.session.serverName}</dd>
        <dt>Server port:</dt>
        <dd>{props.session.port ?? "—"}</dd>
        <dt>File path on server:</dt>
        <dd class="dbg-mono">{remote()}</dd>
        <dt>IDE key:</dt>
        <dd>{props.session.ideKey || "—"}</dd>
      </dl>
      <div class="dbg-section-title">Select a project file to map the server file to</div>
      <div class="dbg-candidates" role="listbox">
        <Show when={!candidates.loading} fallback={<div class="muted dbg-pad">Searching project files…</div>}>
          <For each={candidates() ?? []} fallback={<div class="muted dbg-pad">No project file named {basename(remote())}</div>}>
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
            <span class="muted">Path mapping:</span>
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
  let input!: HTMLTextAreaElement;
  const evaluate = async () => {
    const s = pausedSession();
    const e = expr().trim();
    if (!s || !e) return;
    setBusy(true);
    setError(null);
    try {
      setResult(await debugApi.eval(s.id, e, selectedFrame()?.level ?? 0));
    } catch (err) {
      setResult(null);
      setError(String(err).replace(/^Error: /, ""));
    } finally {
      setBusy(false);
    }
  };
  onMount(() => {
    input.focus();
    input.select();
    if (props.expression.trim()) void evaluate();
  });
  const close = () => setEvaluateRequest(null);
  return (
    <DialogFrame
      title="Evaluate"
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
            Add to Watches
          </button>
          <button class="btn-secondary" onClick={close}>
            Close
          </button>
        </>
      }
    >
      <label class="dbg-label" for="dbg-eval-expr">
        Expression:
      </label>
      <div class="dbg-eval-line">
        <textarea
          id="dbg-eval-expr"
          ref={input}
          class="dbg-eval-expr"
          rows={2}
          spellcheck={false}
          value={expr()}
          onInput={(e) => setExpr(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void evaluate();
            }
          }}
        />
        <button class="btn-primary" disabled={busy() || !pausedSession()} onClick={() => void evaluate()}>
          Evaluate
        </button>
      </div>
      <div class="dbg-label">Result:</div>
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
          <div class="muted dbg-pad">{busy() ? "Collecting data…" : "Press Enter to evaluate"}</div>
        </Show>
      </div>
    </DialogFrame>
  );
}

// ---- Breakpoints dialog -----------------------------------------------------------------

function bpLabel(bp: Breakpoint): string {
  return bp.kind === "exception" ? bp.exception ?? "*" : `${basename(bp.file ?? "")}:${bp.line}`;
}

function BreakpointDetails(props: { bp: Breakpoint }) {
  const [condition, setCondition] = createSignal(props.bp.condition ?? "");
  const [exception, setException] = createSignal(props.bp.exception ?? "");
  createEffect(on(() => props.bp.id, () => {
    setCondition(props.bp.condition ?? "");
    setException(props.bp.exception ?? "");
  }));
  const save = (patch: Partial<Breakpoint>) => void updateBreakpoint({ ...props.bp, ...patch });
  return (
    <div class="dbg-bp-details">
      <div class="dbg-bp-details-title">{props.bp.kind === "exception" ? `PHP Exception Breakpoint: ${bpLabel(props.bp)}` : `${props.bp.file}:${props.bp.line}`}</div>
      <label class="dbg-check">
        <input type="checkbox" checked={props.bp.enabled} onChange={(e) => save({ enabled: e.currentTarget.checked })} />
        Enabled
      </label>
      <label class="dbg-check">
        <input type="checkbox" checked disabled />
        Suspend execution
      </label>
      <Show when={props.bp.kind === "exception"}>
        <label class="dbg-label" for="dbg-bp-exc">
          Exception class:
        </label>
        <input
          id="dbg-bp-exc"
          class="dbg-input dbg-mono"
          value={exception()}
          placeholder="\RuntimeException or * for any"
          onInput={(e) => setException(e.currentTarget.value)}
          onChange={() => save({ exception: exception().trim() || "*" })}
        />
      </Show>
      <Show when={props.bp.kind === "line"}>
        <label class="dbg-label" for="dbg-bp-cond">
          Condition:
        </label>
        <textarea
          id="dbg-bp-cond"
          class="dbg-input dbg-mono"
          rows={3}
          spellcheck={false}
          value={condition()}
          placeholder="e.g. $order->total > 100"
          onInput={(e) => setCondition(e.currentTarget.value)}
          onChange={() => save({ condition: condition().trim() || undefined })}
        />
        <p class="muted dbg-hint">The script suspends only when the PHP expression evaluates to true.</p>
      </Show>
    </div>
  );
}

function BreakpointsDialog(props: { select?: number }) {
  const [selected, setSelected] = createSignal<number | null>(props.select ?? breakpoints()[0]?.id ?? null);
  const [collapsed, setCollapsed] = createSignal<Record<string, boolean>>({});
  const lines = () => breakpoints().filter((b) => b.kind === "line").sort((a, b) => (a.file ?? "").localeCompare(b.file ?? "") || (a.line ?? 0) - (b.line ?? 0));
  const exceptions = () => breakpoints().filter((b) => b.kind === "exception");
  const current = () => breakpoints().find((b) => b.id === selected());
  const close = () => setBreakpointsDialog(null);
  const [adding, setAdding] = createSignal(false);
  const addException = async () => {
    setAdding(false);
    const bp = await debugApi.addBreakpoint({ kind: "exception", exception: "\\RuntimeException" }).catch(() => null);
    if (bp) setSelected(bp.id);
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
              <span class="dbg-bp-dot" classList={{ off: !b.enabled, cond: !!b.condition?.trim() }} />
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
      title="Breakpoints"
      class="dbg-breakpoints"
      onClose={close}
      footer={
        <>
          <label class="dbg-check dbg-left">
            <input type="checkbox" checked={breakpointsMuted()} onChange={() => void toggleMuted()} />
            Mute breakpoints
          </label>
          <button class="btn-primary" onClick={close}>
            Done
          </button>
        </>
      }
    >
      <div class="dbg-bp-split">
        <div class="dbg-bp-list">
          <div class="dbg-bp-toolbar">
            <div class="dbg-add-wrap">
              <button class="icon-btn" title="Add" onClick={() => setAdding((v) => !v)}>
                <Plus size={14} />
              </button>
              <Show when={adding()}>
                <div class="dbg-menu">
                  <button onClick={() => void addException()}>PHP Exception Breakpoint</button>
                </div>
              </Show>
            </div>
            <button class="icon-btn" title="Remove" disabled={selected() == null} onClick={remove}>
              <Minus size={14} />
            </button>
          </div>
          <div class="dbg-bp-tree">
            {group("line", "PHP Line Breakpoints", lines)}
            {group("exception", "PHP Exception Breakpoints", exceptions)}
          </div>
        </div>
        <div class="dbg-bp-right">
          <Show when={current()} keyed fallback={<div class="muted dbg-pad">Select a breakpoint to edit its properties.</div>}>
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
  const [condition, setCondition] = createSignal(bp()?.condition ?? "");
  let input!: HTMLInputElement;
  const close = () => {
    const b = bp();
    if (b && (b.condition ?? "") !== condition().trim()) void updateBreakpoint({ ...b, condition: condition().trim() || undefined });
    setBreakpointPopup(null);
  };
  onMount(() => input?.focus());
  const style = () => ({ left: `${Math.min(props.x + 6, window.innerWidth - 380)}px`, top: `${Math.min(props.y - 8, window.innerHeight - 200)}px` });
  return (
    <div class="dbg-popup-layer" onMouseDown={(e) => e.target === e.currentTarget && close()} onKeyDown={onEscape(() => setBreakpointPopup(null))}>
      <Show when={bp()}>
        {(b) => (
          <div class="dbg-bp-popup" style={style()} role="dialog" aria-label="Breakpoint properties">
            <div class="dbg-bp-popup-title">
              {basename(b().file ?? "")}:{b().line}
            </div>
            <label class="dbg-check">
              <input type="checkbox" checked={b().enabled} onChange={(e) => void updateBreakpoint({ ...b(), enabled: e.currentTarget.checked })} />
              Enabled
            </label>
            <label class="dbg-label" for="dbg-pop-cond">
              Condition:
            </label>
            <input
              id="dbg-pop-cond"
              ref={input}
              class="dbg-input dbg-mono"
              value={condition()}
              placeholder="PHP expression, e.g. $total > 100"
              spellcheck={false}
              onInput={(e) => setCondition(e.currentTarget.value)}
              onKeyDown={(e) => e.key === "Enter" && close()}
            />
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
                Done
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
    </>
  );
}
