/**
 * The popups and dialogs of Extract / Introduce and Inline (`extract.ts`): the chooser
 * popup ("Expressions", "Multiple occurrences found", the language server's choices), the
 * Introduce Constant / Field / Parameter dialog, the Extract Method dialog and the Inline
 * dialog, worded like PHPStorm.
 */
import { createMemo, createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { ArrowDown, ArrowUp } from "lucide-solid";
import type { FieldInit, InlineMode, ParameterChoice, Visibility } from "@/ipc/refactor";
import {
  FIELD_INIT_LABELS,
  VISIBILITIES,
  type ChooserRequest,
  type ExtractMethodDialogRequest,
  type InlineDialogRequest,
  type IntroduceDialogRequest,
} from "./extract";
import { t } from "@/i18n";

const capitalized = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** Dialog keys: Escape cancels, Enter confirms (not in multi-line inputs). */
function dialogKeys(cancel: () => void, ok: () => void): JSX.EventHandler<HTMLDivElement, KeyboardEvent> {
  return (e) => {
    if (e.key === "Escape") cancel();
    else if (e.key === "Enter" && (e.target as HTMLElement).tagName !== "BUTTON") ok();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };
}

export function ChooserPopup(props: { req: ChooserRequest }) {
  let el!: HTMLDivElement;
  const [selected, setSelected] = createSignal(0);
  const select = (i: number) => {
    setSelected(i);
    props.req.select(i);
  };
  const away = (e: MouseEvent) => {
    if (!el.contains(e.target as Node)) props.req.done(null);
  };
  onMount(() => {
    window.addEventListener("mousedown", away, true);
    const r = el.getBoundingClientRect();
    if (r.bottom > window.innerHeight - 8) el.style.top = `${Math.max(8, props.req.y - r.height - 28)}px`;
    if (r.right > window.innerWidth - 8) el.style.left = `${Math.max(8, window.innerWidth - r.width - 8)}px`;
    el.focus();
    select(0);
  });
  onCleanup(() => window.removeEventListener("mousedown", away, true));
  const n = () => props.req.items.length;
  return (
    <div
      class="intention-popup refactor-chooser"
      ref={el}
      tabIndex={-1}
      role="dialog"
      aria-label={props.req.title}
      style={{ left: `${props.req.x}px`, top: `${props.req.y}px` }}
      onKeyDown={(e) => {
        if (e.key === "ArrowDown") select((selected() + 1) % n());
        else if (e.key === "ArrowUp") select((selected() - 1 + n()) % n());
        else if (e.key === "Enter") props.req.done(selected());
        else if (e.key === "Escape") props.req.done(null);
        else if (/^[1-9]$/.test(e.key) && Number(e.key) <= n()) props.req.done(Number(e.key) - 1);
        else return;
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      <div class="refactor-chooser-title">{props.req.title}</div>
      <div class="intention-list" role="listbox">
        <For each={props.req.items}>
          {(item, i) => (
            <button
              class="intention-row"
              classList={{ selected: i() === selected() }}
              role="option"
              aria-selected={i() === selected()}
              onMouseEnter={() => select(i())}
              onClick={() => props.req.done(i())}
            >
              <span class="intention-title refactor-chooser-code" classList={{ code: !!item.range }}>
                {item.label}
              </span>
              <Show when={item.detail}>
                <span class="refactor-chooser-detail">{item.detail}</span>
              </Show>
            </button>
          )}
        </For>
      </div>
    </div>
  );
}

function NameField(props: { value: string; names: string[]; onInput: (v: string) => void; prefix?: string }) {
  const id = `refactor-names-${Math.random().toString(36).slice(2)}`;
  return (
    <div class="refactor-name">
      <Show when={props.prefix}>
        <span class="refactor-name-prefix">{props.prefix}</span>
      </Show>
      <input
        ref={(el) =>
          queueMicrotask(() => {
            el.focus();
            el.select();
          })
        }
        value={props.value}
        list={id}
        spellcheck={false}
        onInput={(e) => props.onInput(e.currentTarget.value)}
      />
      <datalist id={id}>
        <For each={props.names}>{(n) => <option value={n} />}</For>
      </datalist>
    </div>
  );
}

function VisibilityChoice(props: { value: Visibility; onChange: (v: Visibility) => void; disabled?: boolean }) {
  return (
    <div class="refactor-radios" role="radiogroup" aria-label={t("codeintel.extractDialogs.visibility")}>
      <For each={VISIBILITIES}>
        {(v) => (
          <label class="form-label check">
            <input
              type="radio"
              name="visibility"
              checked={props.value === v}
              disabled={props.disabled}
              onChange={() => props.onChange(v)}
            />
            {capitalized(v)}
          </label>
        )}
      </For>
    </div>
  );
}

function Buttons(props: { ok: string; disabled: boolean; preview?: () => void; cancel: () => void; run: () => void }) {
  return (
    <div class="dialog-buttons">
      <Show when={props.preview}>
        <button class="btn-secondary" disabled={props.disabled} onClick={() => props.preview?.()}>
          {t("codeintel.extractDialogs.preview")}
        </button>
        <span class="dialog-buttons-gap" />
      </Show>
      <button class="btn-secondary" onClick={() => props.cancel()}>
        {t("common.cancel")}
      </button>
      <button class="btn-primary" disabled={props.disabled} onClick={() => props.run()}>
        {props.ok}
      </button>
    </div>
  );
}

const TITLES = {
  constant: "Introduce Constant",
  field: "Introduce Field",
  parameter: "Introduce Parameter",
} as const;

export function IntroduceDialog(props: { req: IntroduceDialogRequest }) {
  const info = props.req.info;
  const kind = props.req.kind;
  const [name, setName] = createSignal(info.names[0] ?? "");
  const [replaceAll, setReplaceAll] = createSignal(true);
  const [visibility, setVisibility] = createSignal<Visibility>(kind === "constant" ? "public" : "private");
  const [init, setInit] = createSignal<FieldInit>(info.initPlaces[0] ?? "currentMethod");
  const [defaultValue, setDefaultValue] = createSignal(false);
  const valid = () => /^\$?[A-Za-z_\u0080-￿][\w\u0080-￿]*$/.test(name().trim());
  const close = (preview: boolean | null) =>
    props.req.done(
      preview === null
        ? null
        : {
            name: name().trim(),
            replaceAll: replaceAll() && info.occurrences > 1,
            visibility: kind === "parameter" || info.global ? null : visibility(),
            init: init(),
            defaultValue: defaultValue(),
            preview,
          },
    );
  const run = () => valid() && close(false);
  return (
    <div class="popup-backdrop modal refactoring-backdrop">
      <div class="dialog introduce-dialog" role="dialog" aria-label={TITLES[kind]} onKeyDown={dialogKeys(() => close(null), run)}>
        <div class="dialog-title">{TITLES[kind]}</div>
        <div class="dialog-body">
          <div class="refactor-expression" title={info.text}>
            <span class="refactor-expression-label">{t("codeintel.extractDialogs.expression")}</span> <code>{info.text}</code>
          </div>
          <div class="form-row">
            <label class="form-label">{t("codeintel.extractDialogs.name")}</label>
            <NameField value={name()} names={info.names} onInput={setName} prefix={kind === "constant" ? undefined : "$"} />
          </div>
          <Show when={kind === "field"}>
            <fieldset class="refactor-group">
              <legend>{t("codeintel.extractDialogs.initializeIn")}</legend>
              <For each={info.initPlaces}>
                {(p) => (
                  <label class="form-label check">
                    <input type="radio" name="init" checked={init() === p} onChange={() => setInit(p)} />
                    {FIELD_INIT_LABELS[p]}
                  </label>
                )}
              </For>
            </fieldset>
          </Show>
          <Show when={kind !== "parameter" && !info.global}>
            <fieldset class="refactor-group">
              <legend>{t("codeintel.extractDialogs.visibility")}</legend>
              <VisibilityChoice value={visibility()} onChange={setVisibility} />
            </fieldset>
          </Show>
          <Show when={kind === "parameter"}>
            <div class="refactor-note">
              {t("tools.codeintel.addsTheParameterTo")} <code>{info.functionName}</code>
              <Show when={!defaultValue() && info.callSites > 0}>
                {t("tools.codeintel.andPassesTheExpressionIn", { count: info.callSites })}
              </Show>
              .
            </div>
            <label
              class="form-label check"
              title={info.constantExpression ? undefined : t("codeintel.extractDialogs.onlyAConstantExpressionCan")}
            >
              <input
                type="checkbox"
                checked={defaultValue()}
                disabled={!info.constantExpression}
                onChange={(e) => setDefaultValue(e.currentTarget.checked)}
              />
              {t("tools.codeintel.optionalParameterTheExpressionIs")}
            </label>
          </Show>
          <Show when={kind === "constant" && info.global}>
            <div class="refactor-note">{t("codeintel.extractDialogs.aGlobalConstantIsDeclared")}</div>
          </Show>
          <Show when={info.occurrences > 1}>
            <label class="form-label check">
              <input type="checkbox" checked={replaceAll()} onChange={(e) => setReplaceAll(e.currentTarget.checked)} />
              {t("tools.codeintel.replaceAllOccurrences", { occurrences: info.occurrences })}
            </label>
          </Show>
        </div>
        <Buttons ok="OK" disabled={!valid()} preview={() => valid() && close(true)} cancel={() => close(null)} run={run} />
      </div>
    </div>
  );
}

export function ExtractMethodDialog(props: { req: ExtractMethodDialogRequest }) {
  const info = props.req.info;
  const [name, setName] = createSignal(info.name);
  const [visibility, setVisibility] = createSignal<Visibility>("private");
  const [isStatic, setStatic] = createSignal(info.staticContext || (info.inClass && !info.usesThis));
  const [params, setParams] = createSignal<ParameterChoice[]>(info.parameters.map((p) => ({ original: p.name, name: p.name })));
  const [selected, setSelected] = createSignal(0);
  const [duplicates, setDuplicates] = createSignal(info.duplicates > 0);
  const typeOf = (original: string) => info.parameters.find((p) => p.name === original)?.typeHint;
  const ident = /^[A-Za-z_\u0080-￿][\w\u0080-￿]*$/;
  const valid = () =>
    ident.test(name().trim()) && params().every((p) => ident.test(p.name)) && new Set(params().map((p) => p.name)).size === params().length;
  const move = (delta: number) => {
    const i = selected();
    const j = i + delta;
    const list = [...params()];
    if (j < 0 || j >= list.length) return;
    [list[i], list[j]] = [list[j], list[i]];
    setParams(list);
    setSelected(j);
  };
  const returns = createMemo(() => {
    switch (info.returns) {
      case "expression":
        return "the value of the expression";
      case "return":
        return "the fragment's return value";
      case "variables":
        return info.outputs.length === 1 ? `$${info.outputs[0]}` : `[${info.outputs.map((o) => `$${o}`).join(", ")}]`;
      default:
        return "nothing";
    }
  });
  const signature = createMemo(() => {
    const list = params()
      .map((p) => `${typeOf(p.original) ? `${typeOf(p.original)} ` : ""}$${p.name}`)
      .join(", ");
    const head = info.inClass ? `${visibility()} ${isStatic() ? "static " : ""}function` : "function";
    return `${head} ${name().trim() || "…"}(${list})`;
  });
  const close = (preview: boolean | null) =>
    props.req.done(
      preview === null
        ? null
        : {
            name: name().trim(),
            visibility: info.inClass ? visibility() : null,
            isStatic: info.inClass && isStatic(),
            parameters: params(),
            replaceDuplicates: duplicates(),
            preview,
          },
    );
  const run = () => valid() && close(false);
  return (
    <div class="popup-backdrop modal refactoring-backdrop">
      <div class="dialog extract-method-dialog" role="dialog" aria-label={info.title} onKeyDown={dialogKeys(() => close(null), run)}>
        <div class="dialog-title">{info.title}</div>
        <div class="dialog-body">
          <div class="form-row">
            <label class="form-label">{t("codeintel.extractDialogs.name")}</label>
            <NameField value={name()} names={[]} onInput={setName} />
          </div>
          <Show when={info.inClass}>
            <div class="extract-method-options">
              <fieldset class="refactor-group">
                <legend>{t("codeintel.extractDialogs.visibility")}</legend>
                <VisibilityChoice value={visibility()} onChange={setVisibility} />
              </fieldset>
              <label class="form-label check" title={info.usesThis ? t("codeintel.extractDialogs.theFragmentUsesThis") : undefined}>
                <input
                  type="checkbox"
                  checked={isStatic()}
                  disabled={info.usesThis || info.staticContext}
                  onChange={(e) => setStatic(e.currentTarget.checked)}
                />
                {t("codeintel.extractDialogs.declareStatic")}
              </label>
            </div>
          </Show>
          <fieldset class="refactor-group">
            <legend>{t("codeintel.extractDialogs.parameters")}</legend>
            <Show when={params().length} fallback={<div class="refactor-note">{t("codeintel.extractDialogs.noParameters")}</div>}>
              <div class="extract-params">
                <div class="extract-params-list" role="listbox" aria-label={t("codeintel.extractDialogs.parameters")}>
                  <For each={params()}>
                    {(p, i) => (
                      <div class="extract-param" classList={{ selected: selected() === i() }} onMouseDown={() => setSelected(i())}>
                        <span class="extract-param-type">{typeOf(p.original) ?? ""}</span>
                        <span class="refactor-name-prefix">$</span>
                        <input
                          value={p.name}
                          spellcheck={false}
                          aria-label={t("codeintel.extractDialogs.parameterOriginal", { original: p.original })}
                          onFocus={() => setSelected(i())}
                          onInput={(e) => {
                            const v = e.currentTarget.value.replace(/^\$/, "");
                            setParams(params().map((q, j) => (j === i() ? { ...q, name: v } : q)));
                          }}
                        />
                      </div>
                    )}
                  </For>
                </div>
                <div class="extract-params-buttons">
                  <button class="icon-btn" title={t("common.moveUp")} aria-label={t("common.moveUp")} disabled={selected() === 0} onClick={() => move(-1)}>
                    <ArrowUp size={14} />
                  </button>
                  <button
                    class="icon-btn"
                    title={t("common.moveDown")}
                    aria-label={t("common.moveDown")}
                    disabled={selected() >= params().length - 1}
                    onClick={() => move(1)}
                  >
                    <ArrowDown size={14} />
                  </button>
                </div>
              </div>
            </Show>
          </fieldset>
          <div class="refactor-note">{t("tools.codeintel.returns", { returns: returns() })}</div>
          <Show when={info.duplicates > 0}>
            <label class="form-label check">
              <input type="checkbox" checked={duplicates()} onChange={(e) => setDuplicates(e.currentTarget.checked)} />
              {t("tools.codeintel.replaceCodeFragmentsFoundThat", { count: info.duplicates })}
            </label>
          </Show>
          <div class="refactor-signature-label">{t("codeintel.extractDialogs.signaturePreview")}</div>
          <pre class="refactor-signature">{signature()}</pre>
        </div>
        <Buttons ok={t("codeintel.refactor")} disabled={!valid()} preview={() => valid() && close(true)} cancel={() => close(null)} run={run} />
      </div>
    </div>
  );
}

export function InlineDialog(props: { req: InlineDialogRequest }) {
  const info = props.req.info;
  const options = (): { mode: InlineMode; label: string }[] => {
    const out: { mode: InlineMode; label: string }[] = [];
    if (!info.readOnly) {
      out.push({
        mode: "all",
        label: t("codeintel.inline.all", { kind: info.kind, count: info.occurrences }),
      });
      if (info.kind !== "variable")
        out.push({
          mode: "allKeep",
          label: t("codeintel.inline.allKeep", { kind: info.kind }),
        });
    }
    if (info.onReference)
      out.push({
        mode: "this",
        label: t("codeintel.inline.this", { kind: info.kind }),
      });
    return out;
  };
  const [mode, setMode] = createSignal<InlineMode>(info.onReference && info.kind !== "variable" ? "this" : (options()[0]?.mode ?? "this"));
  const close = (preview: boolean | null) => props.req.done(preview === null ? null : { mode: mode(), preview });
  return (
    <div class="popup-backdrop modal refactoring-backdrop">
      <div
        class="dialog inline-dialog"
        role="dialog"
        aria-label={info.title}
        tabIndex={-1}
        ref={(e) => queueMicrotask(() => e.focus())}
        onKeyDown={dialogKeys(
          () => close(null),
          () => close(false),
        )}
      >
        <div class="dialog-title">{info.title}</div>
        <div class="dialog-body">
          <div class="refactor-expression">
            {t("codeintel.inline.kind", { kind: info.kind })} <code>{info.name}</code>
          </div>
          <div class="refactor-radios vertical" role="radiogroup">
            <For each={options()}>
              {(o) => (
                <label class="form-label check">
                  <input type="radio" name="inline-mode" checked={mode() === o.mode} onChange={() => setMode(o.mode)} />
                  {o.label}
                </label>
              )}
            </For>
          </div>
        </div>
        <Buttons ok={t("codeintel.refactor")} disabled={false} preview={() => close(true)} cancel={() => close(null)} run={() => close(false)} />
      </div>
    </div>
  );
}
