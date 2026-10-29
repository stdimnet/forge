/**
 * The dialogs of Move, Copy, Safe Delete, Change Signature and Make Static
 * (`moveRefactorings.ts`), worded like PHPStorm: Move Class, Copy Class, Move Members, Move
 * and Copy (files), Safe Delete, Usages Detected, Change Signature and Make Method Static.
 */
import { createEffect, createMemo, createSignal, For, on, Show, type JSX } from "solid-js";
import { ArrowDown, ArrowUp, Minus, Plus, TriangleAlert } from "lucide-solid";
import type { ParamRow } from "./moveRefactorings";
import {
  directoryFor,
  moveDialog,
  optionsOf,
  rowsOf,
  signaturePreview,
  validateSignature,
  type ChangeSignatureRequest,
  type CopyClassRequest,
  type CopyFilesRequest,
  type MakeStaticRequest,
  type MoveClassRequest,
  type MoveDialog,
  type MoveFilesRequest,
  type MoveFunctionRequest,
  type MoveMembersRequest,
  type SafeDeleteRequest,
  type UsagesDetectedRequest,
} from "./moveRefactorings";
import { VISIBILITIES } from "./extract";

/** Escape cancels, Enter confirms (not on buttons or in the parameter table). */
export function keys(cancel: () => void, ok: () => void): JSX.EventHandler<HTMLDivElement, KeyboardEvent> {
  return (e) => {
    const target = e.target as HTMLElement;
    if (e.key === "Escape") cancel();
    else if (e.key === "Enter" && target.tagName !== "BUTTON" && target.tagName !== "SELECT") ok();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
}

export const focusSelect = (el: HTMLInputElement) =>
  queueMicrotask(() => {
    el.focus();
    el.select();
  });

export function Frame(props: { title: string; class: string; onKeyDown: JSX.EventHandler<HTMLDivElement, KeyboardEvent>; children: JSX.Element; focus?: boolean }) {
  return (
    <div class="popup-backdrop modal refactoring-backdrop">
      <div
        class={`dialog move-dialog ${props.class}`}
        role="dialog"
        aria-label={props.title}
        tabIndex={-1}
        ref={(el) => props.focus && queueMicrotask(() => el.focus())}
        onKeyDown={props.onKeyDown}
      >
        <div class="dialog-title">{props.title}</div>
        {props.children}
      </div>
    </div>
  );
}

export function Buttons(props: { ok: string; disabled?: boolean; preview?: () => void; cancel: () => void; run: () => void }) {
  return (
    <div class="dialog-buttons">
      <Show when={props.preview}>
        <button class="btn-secondary" disabled={props.disabled} onClick={() => props.preview?.()}>
          Preview
        </button>
        <span class="dialog-buttons-gap" />
      </Show>
      <button class="btn-secondary" onClick={() => props.cancel()}>
        Cancel
      </button>
      <button class="btn-primary" disabled={props.disabled} onClick={() => props.run()}>
        {props.ok}
      </button>
    </div>
  );
}

export function Check(props: { label: string; checked: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <label class="form-label check">
      <input type="checkbox" checked={props.checked} disabled={props.disabled} onChange={(e) => props.onChange(e.currentTarget.checked)} />
      {props.label}
    </label>
  );
}

const NAMESPACE = /^(\\?[A-Za-z_\u0080-￿][\w\u0080-￿]*)(\\[A-Za-z_\u0080-￿][\w\u0080-￿]*)*$/;
export const validNamespace = (ns: string) => ns.trim() === "" || NAMESPACE.test(ns.trim());
export const IDENT = /^[A-Za-z_\u0080-￿][\w\u0080-￿]*$/;

/** Namespace and directory fields: the directory follows the namespace (PSR-4) until edited. */
export function NamespaceFields(props: {
  namespace: string;
  setNamespace: (v: string) => void;
  directory: string;
  setDirectory: (v: string) => void;
  label: string;
  autofocus?: boolean;
}) {
  const [touched, setTouched] = createSignal(false);
  let seq = 0;
  createEffect(
    on(
      () => props.namespace,
      (ns, prev) => {
        if (prev === undefined || touched() || !validNamespace(ns)) return;
        const n = ++seq;
        void directoryFor(ns.trim().replace(/^\\/, "")).then((dir) => {
          if (n === seq && dir != null && !touched()) props.setDirectory(dir);
        });
      },
    ),
  );
  return (
    <>
      <label class="form-label">{props.label}</label>
      <input
        ref={(el) => props.autofocus && focusSelect(el)}
        class="move-input"
        value={props.namespace}
        spellcheck={false}
        aria-label="Namespace"
        onInput={(e) => props.setNamespace(e.currentTarget.value)}
      />
      <label class="form-label">Target destination directory:</label>
      <input
        class="move-input"
        value={props.directory}
        spellcheck={false}
        aria-label="Directory"
        onInput={(e) => {
          setTouched(true);
          props.setDirectory(e.currentTarget.value);
        }}
      />
    </>
  );
}

export function MoveClassDialog(props: { req: MoveClassRequest }) {
  const t = props.req.target;
  const [ns, setNs] = createSignal(t.namespace);
  const [dir, setDir] = createSignal(t.directory);
  const [comments, setComments] = createSignal(false);
  const valid = () => validNamespace(ns());
  const close = (preview: boolean | null) =>
    props.req.done(
      preview === null ? null : { namespace: ns().trim().replace(/^\\/, ""), directory: dir().trim(), searchInComments: comments(), preview },
    );
  const run = () => valid() && close(false);
  return (
    <Frame title="Move Class" class="move-class-dialog" onKeyDown={keys(() => close(null), run)}>
      <div class="dialog-body">
        <NamespaceFields
          namespace={ns()}
          setNamespace={setNs}
          directory={dir()}
          setDirectory={setDir}
          label={`Move ${t.classKind} ${t.name} to namespace:`}
          autofocus
        />
        <Show when={!valid()}>
          <div class="dialog-error">'{ns().trim()}' is not a valid namespace name</div>
        </Show>
        <Show when={!t.sole}>
          <div class="refactor-note">The file declares more than {t.name}: the class moves to a new file.</div>
        </Show>
        <Check label="Search in comments and strings" checked={comments()} onChange={setComments} />
      </div>
      <Buttons ok="Refactor" disabled={!valid()} preview={() => valid() && close(true)} cancel={() => close(null)} run={run} />
    </Frame>
  );
}

export function CopyClassDialog(props: { req: CopyClassRequest }) {
  const t = props.req.target;
  const [name, setName] = createSignal(t.name);
  const [ns, setNs] = createSignal(t.namespace);
  const [dir, setDir] = createSignal(t.directory);
  const valid = () => IDENT.test(name().trim()) && validNamespace(ns());
  const close = (ok: boolean) =>
    props.req.done(ok ? { name: name().trim(), namespace: ns().trim().replace(/^\\/, ""), directory: dir().trim() } : null);
  const run = () => valid() && close(true);
  return (
    <Frame title="Copy Class" class="move-class-dialog" onKeyDown={keys(() => close(false), run)}>
      <div class="dialog-body">
        <div class="refactor-note">
          Copy {t.classKind} <code>{t.fqn}</code>
        </div>
        <label class="form-label">New name:</label>
        <input ref={focusSelect} class="move-input" value={name()} spellcheck={false} aria-label="New name" onInput={(e) => setName(e.currentTarget.value)} />
        <NamespaceFields namespace={ns()} setNamespace={setNs} directory={dir()} setDirectory={setDir} label="To namespace:" />
      </div>
      <Buttons ok="OK" disabled={!valid()} cancel={() => close(false)} run={run} />
    </Frame>
  );
}

export function MoveMembersDialog(props: { req: MoveMembersRequest }) {
  const t = props.req.target;
  const [selected, setSelected] = createSignal<Set<string>>(new Set(t.selected ? [t.selected] : t.members.map((m) => m.name)));
  const [target, setTarget] = createSignal("");
  const valid = () => selected().size > 0 && NAMESPACE.test(target().trim());
  const toggle = (name: string, on: boolean) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (on) next.add(name);
      else next.delete(name);
      return next;
    });
  const close = (preview: boolean | null) =>
    props.req.done(preview === null ? null : { members: [...selected()], target: target().trim().replace(/^\\/, ""), preview });
  const run = () => valid() && close(false);
  return (
    <Frame title="Move Members" class="move-members-dialog" onKeyDown={keys(() => close(null), run)}>
      <div class="dialog-body">
        <div class="refactor-note">
          Move members from: <code>{t.classFqn}</code>
        </div>
        <label class="form-label">To (fully qualified name):</label>
        <input
          ref={focusSelect}
          class="move-input"
          value={target()}
          placeholder="App\Support\Helpers"
          spellcheck={false}
          aria-label="Target class"
          onInput={(e) => setTarget(e.currentTarget.value)}
        />
        <fieldset class="refactor-group">
          <legend>Members to be moved (static only)</legend>
          <div class="move-members-list" role="list">
            <For each={t.members}>
              {(m) => (
                <label class="move-member" role="listitem">
                  <input type="checkbox" checked={selected().has(m.name)} onChange={(e) => toggle(m.name, e.currentTarget.checked)} />
                  <span class={`move-member-kind ${m.kind}`}>{m.kind === "method" ? "m" : m.kind === "property" ? "f" : "c"}</span>
                  <code class="move-member-signature">{m.signature}</code>
                </label>
              )}
            </For>
          </div>
        </fieldset>
      </div>
      <Buttons ok="Refactor" disabled={!valid()} preview={() => valid() && close(true)} cancel={() => close(null)} run={run} />
    </Frame>
  );
}

export function MoveFunctionDialog(props: { req: MoveFunctionRequest }) {
  const t = props.req.target;
  const [file, setFile] = createSignal(t.path);
  const valid = () => /\.(php|phtml|inc)$/i.test(file().trim()) && file().trim() !== t.path;
  const close = (preview: boolean | null) => props.req.done(preview === null ? null : { file: file().trim(), preview });
  const run = () => valid() && close(false);
  return (
    <Frame title="Move Function" class="move-class-dialog" onKeyDown={keys(() => close(null), run)}>
      <div class="dialog-body">
        <div class="refactor-note">
          Move function <code>{t.fqn}()</code>
        </div>
        <label class="form-label">To file:</label>
        <input ref={focusSelect} class="move-input" value={file()} spellcheck={false} aria-label="To file" onInput={(e) => setFile(e.currentTarget.value)} />
      </div>
      <Buttons ok="Refactor" disabled={!valid()} preview={() => valid() && close(true)} cancel={() => close(null)} run={run} />
    </Frame>
  );
}

export function MoveFilesDialog(props: { req: MoveFilesRequest }) {
  const [dir, setDir] = createSignal(props.req.directory);
  const [refs, setRefs] = createSignal(true);
  const [comments, setComments] = createSignal(false);
  const close = (ok: boolean) =>
    props.req.done(ok ? { directory: dir().trim(), searchForReferences: refs() && props.req.php, searchInComments: comments() && props.req.php } : null);
  return (
    <Frame title="Move" class="move-class-dialog" onKeyDown={keys(() => close(false), () => close(true))}>
      <div class="dialog-body">
        <div class="refactor-note">Move {props.req.what}</div>
        <label class="form-label">To directory:</label>
        <input ref={focusSelect} class="move-input" value={dir()} spellcheck={false} aria-label="To directory" onInput={(e) => setDir(e.currentTarget.value)} />
        <Show when={props.req.php}>
          <Check label="Search for references" checked={refs()} onChange={setRefs} />
          <Check label="Search in comments and strings" checked={comments()} disabled={!refs()} onChange={setComments} />
        </Show>
      </div>
      <Buttons ok="Refactor" cancel={() => close(false)} run={() => close(true)} />
    </Frame>
  );
}

export function CopyFilesDialog(props: { req: CopyFilesRequest }) {
  const [name, setName] = createSignal(props.req.name);
  const [dir, setDir] = createSignal(props.req.directory);
  const valid = () => !!name().trim() && !/[\\/\0]/.test(name().trim()) && name().trim() !== "." && name().trim() !== "..";
  const close = (ok: boolean) => props.req.done(ok ? { name: name().trim(), directory: dir().trim() } : null);
  const run = () => valid() && close(true);
  return (
    <Frame title="Copy" class="move-class-dialog" onKeyDown={keys(() => close(false), run)}>
      <div class="dialog-body">
        <div class="refactor-note">Copy {props.req.what}</div>
        <label class="form-label">New name:</label>
        <input ref={focusSelect} class="move-input" value={name()} spellcheck={false} aria-label="New name" onInput={(e) => setName(e.currentTarget.value)} />
        <label class="form-label">To directory:</label>
        <input class="move-input" value={dir()} spellcheck={false} aria-label="To directory" onInput={(e) => setDir(e.currentTarget.value)} />
      </div>
      <Buttons ok="OK" disabled={!valid()} cancel={() => close(false)} run={run} />
    </Frame>
  );
}

export function SafeDeleteDialog(props: { req: SafeDeleteRequest }) {
  const [comments, setComments] = createSignal(false);
  const close = (ok: boolean) => props.req.done(ok ? { searchInComments: comments() } : null);
  return (
    <Frame title="Safe Delete" class="safe-delete-dialog" focus onKeyDown={keys(() => close(false), () => close(true))}>
      <div class="dialog-body">
        <div>Delete {props.req.what}?</div>
        <Check label="Search in comments and strings" checked={comments()} onChange={setComments} />
      </div>
      <Buttons ok="OK" cancel={() => close(false)} run={() => close(true)} />
    </Frame>
  );
}

export function UsagesDetectedDialog(props: { req: UsagesDetectedRequest }) {
  return (
    <Frame title="Usages Detected" class="safe-delete-dialog" focus onKeyDown={keys(() => props.req.done(null), () => props.req.done("view"))}>
      <div class="dialog-body usages-detected">
        <TriangleAlert size={28} class="usages-detected-icon" />
        <div class="usages-detected-text">
          <For each={props.req.message.split("\n")}>{(line) => <div>{line}</div>}</For>
        </div>
      </div>
      <div class="dialog-buttons">
        <button class="btn-primary" onClick={() => props.req.done("view")}>
          View Usages
        </button>
        <button class="btn-secondary" onClick={() => props.req.done("delete")}>
          Delete Anyway
        </button>
        <span class="dialog-buttons-gap" />
        <button class="btn-secondary" onClick={() => props.req.done(null)}>
          Cancel
        </button>
      </div>
    </Frame>
  );
}

export function ChangeSignatureDialog(props: { req: ChangeSignatureRequest }) {
  const info = props.req.info;
  const [name, setName] = createSignal(info.name);
  const [visibility, setVisibility] = createSignal<string | null>(info.kind === "method" ? (info.visibility ?? "public") : null);
  const [returnType, setReturnType] = createSignal(info.returnType ?? "");
  const [rows, setRows] = createSignal<ParamRow[]>(rowsOf(info));
  const [selected, setSelected] = createSignal(0);
  const error = createMemo(() => validateSignature(name(), rows()));
  const preview = createMemo(() => signaturePreview(info, name(), visibility(), returnType(), rows()));
  const update = (i: number, patch: Partial<ParamRow>) => setRows(rows().map((r, j) => (j === i ? { ...r, ...patch } : r)));
  const move = (delta: number) => {
    const i = selected();
    const j = i + delta;
    const list = [...rows()];
    if (j < 0 || j >= list.length) return;
    [list[i], list[j]] = [list[j], list[i]];
    setRows(list);
    setSelected(j);
  };
  const add = () => {
    const taken = new Set(rows().map((r) => r.name));
    let n = "param";
    for (let i = 2; taken.has(n); i++) n = `param${i}`;
    const list = [...rows()];
    // Before a variadic parameter, which has to stay last.
    const at = list.length && list[list.length - 1].variadic ? list.length - 1 : list.length;
    list.splice(at, 0, { original: null, name: n, type: "", defaultValue: "", callValue: "", byRef: false, variadic: false });
    setRows(list);
    setSelected(at);
  };
  const remove = () => {
    const list = rows().filter((_, j) => j !== selected());
    setRows(list);
    setSelected(Math.max(0, Math.min(selected(), list.length - 1)));
  };
  const close = (previewIt: boolean | null) =>
    props.req.done(previewIt === null ? null : { ...optionsOf(name(), visibility(), returnType(), rows()), preview: previewIt });
  const run = () => !error() && close(false);
  const removedUsed = () =>
    info.parameters.map((p, i) => ({ p, i })).filter(({ p, i }) => p.used && !rows().some((r) => r.original === i));
  return (
    <Frame title="Change Signature" class="change-signature-dialog" onKeyDown={keys(() => close(null), run)}>
      <div class="dialog-body">
        <div class="change-signature-head">
          <Show when={info.kind === "method"}>
            <label class="form-label">
              Visibility:
              <select value={visibility() ?? "public"} onChange={(e) => setVisibility(e.currentTarget.value)} aria-label="Visibility">
                <For each={VISIBILITIES}>{(v) => <option value={v}>{v}</option>}</For>
              </select>
            </label>
          </Show>
          <label class="form-label">
            Return type:
            <input class="move-input mono" value={returnType()} spellcheck={false} aria-label="Return type" onInput={(e) => setReturnType(e.currentTarget.value)} />
          </label>
          <label class="form-label">
            Name:
            <input
              ref={focusSelect}
              class="move-input mono"
              value={name()}
              spellcheck={false}
              aria-label="Name"
              disabled={info.constructor}
              onInput={(e) => setName(e.currentTarget.value)}
            />
          </label>
        </div>
        <fieldset class="refactor-group">
          <legend>Parameters</legend>
          <div class="signature-params">
            <div class="signature-table" role="grid" aria-label="Parameters">
              <div class="signature-row head" role="row">
                <span>Type</span>
                <span>Name</span>
                <span>Default value</span>
                <span title="The value passed at call sites for a new parameter">Value at call sites</span>
              </div>
              <For each={rows()} fallback={<div class="refactor-note signature-empty">No parameters</div>}>
                {(r, i) => (
                  <div class="signature-row" role="row" classList={{ selected: selected() === i(), added: r.original === null }} onMouseDown={() => setSelected(i())}>
                    <input value={r.type} spellcheck={false} aria-label="Type" onFocus={() => setSelected(i())} onInput={(e) => update(i(), { type: e.currentTarget.value })} />
                    <span class="signature-name">
                      <span class="refactor-name-prefix">{r.variadic ? "...$" : "$"}</span>
                      <input value={r.name} spellcheck={false} aria-label="Name" onFocus={() => setSelected(i())} onInput={(e) => update(i(), { name: e.currentTarget.value.replace(/^\$/, "") })} />
                    </span>
                    <input
                      value={r.defaultValue}
                      spellcheck={false}
                      aria-label="Default value"
                      disabled={r.variadic}
                      onFocus={() => setSelected(i())}
                      onInput={(e) => update(i(), { defaultValue: e.currentTarget.value })}
                    />
                    <input
                      value={r.callValue}
                      spellcheck={false}
                      aria-label="Value at call sites"
                      disabled={r.original !== null}
                      placeholder={r.original !== null ? "" : r.defaultValue || ""}
                      onFocus={() => setSelected(i())}
                      onInput={(e) => update(i(), { callValue: e.currentTarget.value })}
                    />
                  </div>
                )}
              </For>
            </div>
            <div class="extract-params-buttons">
              <button class="icon-btn" title="Add" aria-label="Add" onClick={add}>
                <Plus size={14} />
              </button>
              <button class="icon-btn" title="Remove" aria-label="Remove" disabled={!rows().length} onClick={remove}>
                <Minus size={14} />
              </button>
              <button class="icon-btn" title="Move Up" aria-label="Move Up" disabled={selected() === 0} onClick={() => move(-1)}>
                <ArrowUp size={14} />
              </button>
              <button class="icon-btn" title="Move Down" aria-label="Move Down" disabled={selected() >= rows().length - 1} onClick={() => move(1)}>
                <ArrowDown size={14} />
              </button>
            </div>
          </div>
        </fieldset>
        <For each={removedUsed()}>{({ p }) => <div class="refactor-note warning">Parameter ${p.name} is used in the body and is removed.</div>}</For>
        <div class="refactor-signature-label">Signature Preview</div>
        <pre class="refactor-signature">{preview()}</pre>
        <Show when={error()}>
          <div class="dialog-error">{error()}</div>
        </Show>
      </div>
      <Buttons ok="Refactor" disabled={!!error()} preview={() => !error() && close(true)} cancel={() => close(null)} run={run} />
    </Frame>
  );
}

export function MakeStaticDialog(props: { req: MakeStaticRequest }) {
  const info = props.req.info;
  const [mode, setMode] = createSignal<"object" | "fields">(info.needsObject || !info.fields.length ? "object" : "fields");
  const [param, setParam] = createSignal(info.parameter);
  const valid = () => !info.usesThis || mode() === "fields" || IDENT.test(param().trim().replace(/^\$/, ""));
  const close = (preview: boolean | null) =>
    props.req.done(
      preview === null
        ? null
        : {
            objectParameter: info.usesThis && mode() === "object" ? param().trim().replace(/^\$/, "") : null,
            fieldParameters: info.usesThis && mode() === "fields",
            preview,
          },
    );
  const run = () => valid() && close(false);
  return (
    <Frame title="Make Method Static" class="move-class-dialog" focus onKeyDown={keys(() => close(null), run)}>
      <div class="dialog-body">
        <div>
          Make method <code>{`${info.className}::${info.name}()`}</code> static
        </div>
        <Show when={info.usesThis} fallback={<div class="refactor-note">The method does not use $this; its calls become {info.className}::{info.name}().</div>}>
          <div class="refactor-radios vertical" role="radiogroup">
            <label class="form-label check">
              <input type="radio" name="make-static" checked={mode() === "object"} onChange={() => setMode("object")} />
              Add object as a parameter with name:
            </label>
            <div class="refactor-name make-static-param">
              <span class="refactor-name-prefix">$</span>
              <input value={param()} spellcheck={false} aria-label="Parameter name" disabled={mode() !== "object"} onInput={(e) => setParam(e.currentTarget.value)} />
            </div>
            <label class="form-label check" title={info.needsObject ? "The method calls methods or writes fields of $this" : undefined}>
              <input type="radio" name="make-static" checked={mode() === "fields"} disabled={info.needsObject || !info.fields.length} onChange={() => setMode("fields")} />
              Add parameters for fields: {info.fields.map((f) => `$${f}`).join(", ") || "none"}
            </label>
          </div>
        </Show>
      </div>
      <Buttons ok="Refactor" disabled={!valid()} preview={() => valid() && close(true)} cancel={() => close(null)} run={run} />
    </Frame>
  );
}

/** The open Move / Copy / Safe Delete / Change Signature / Make Static dialog. */
export function MoveDialogHost() {
  const d = (): MoveDialog | null => moveDialog();
  return (
    <Show when={d()} keyed>
      {(x) => {
        switch (x.kind) {
          case "moveClass":
            return <MoveClassDialog req={x.req} />;
          case "copyClass":
            return <CopyClassDialog req={x.req} />;
          case "moveMembers":
            return <MoveMembersDialog req={x.req} />;
          case "moveFunction":
            return <MoveFunctionDialog req={x.req} />;
          case "moveFiles":
            return <MoveFilesDialog req={x.req} />;
          case "copyFiles":
            return <CopyFilesDialog req={x.req} />;
          case "safeDelete":
            return <SafeDeleteDialog req={x.req} />;
          case "usagesDetected":
            return <UsagesDetectedDialog req={x.req} />;
          case "changeSignature":
            return <ChangeSignatureDialog req={x.req} />;
          case "makeStatic":
            return <MakeStaticDialog req={x.req} />;
        }
      }}
    </Show>
  );
}
