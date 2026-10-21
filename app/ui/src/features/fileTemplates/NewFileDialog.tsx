/**
 * The dialogs of File | New ▸ <template>:
 *
 * - "New PHP Class" (PHP class-like templates): Name, Namespace, File name, Directory,
 *   Template (Class / Interface / Trait / Enum and the project's framework templates),
 *   Extends / Implements when the template uses them. The namespace of the directory and the
 *   directory of a typed namespace come from composer.json's PSR-4 rules (computed in Rust).
 * - "New <template>" for the others: the name plus the template's own variables.
 *
 * The file is rendered and created by the backend; it opens in the editor afterwards.
 */
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { templatesApi, type TemplateInfo } from "@/ipc/templates";
import { suspendKeymap } from "@/keymap";
import { openFile } from "@/state/editor";
import { refreshDirs, revealPath } from "@/features/project/ProjectView";
import {
  catalog,
  dialog as dialogRequest,
  phpClassTemplates,
  setDialog,
  templateLabel,
  validateClassName,
  validateNamespace,
  variableLabel,
  type NewFileDialogRequest,
} from "./model";

const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");
const message = (e: unknown) => String(e instanceof Error ? e.message : e);

export function NewFileDialog(props: { req: NewFileDialogRequest }) {
  const release = suspendKeymap();
  onCleanup(release);

  const [template, setTemplate] = createSignal<TemplateInfo>(props.req.template);
  const php = () => template().phpClass;
  const [name, setName] = createSignal("");
  const [namespace, setNamespace] = createSignal("");
  const [directory, setDirectory] = createSignal(props.req.dir);
  const [fileName, setFileName] = createSignal<string | null>(null);
  const [extendsName, setExtends] = createSignal("");
  const [implementsNames, setImplements] = createSignal("");
  const [vars, setVars] = createSignal<Record<string, string>>({});
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  const uses = (v: string) => template().references.includes(v);
  const ext = () => template().extension.replace(/^\./, "");
  /** The file name follows the class name until it is edited. */
  const effectiveFileName = () => fileName() ?? (name().trim() ? `${name().trim()}${ext() ? `.${ext()}` : ""}` : "");

  let nsSeq = 0;
  const loadNamespace = async (dir: string) => {
    const id = ++nsSeq;
    const ns = await templatesApi.phpNamespace(dir).catch(() => null);
    if (id === nsSeq && ns !== null) setNamespace(ns);
  };
  onMount(() => {
    if (php()) void loadNamespace(props.req.dir);
  });

  let dirSeq = 0;
  const onNamespace = async (value: string) => {
    setNamespace(value);
    nsSeq++; // a typed namespace wins over a pending lookup
    if (validateNamespace(value)) return;
    const id = ++dirSeq;
    const dir = await templatesApi.phpDirectory(value).catch(() => null);
    if (id === dirSeq && dir !== null) setDirectory(dir);
  };

  const problem = () =>
    (php() ? validateClassName(name()) : null) ?? (php() ? validateNamespace(namespace()) : null);
  const valid = () => !!name().trim() && !problem() && !busy();

  const close = () => setDialog(null);

  const submit = async () => {
    if (!valid()) return;
    setBusy(true);
    setError(null);
    const dir = php() ? directory().trim().replace(/^\/+|\/+$/g, "") : props.req.dir;
    try {
      const file = await templatesApi.create({
        template: template().name,
        dir,
        name: name().trim(),
        fileName: php() ? effectiveFileName() : undefined,
        namespace: php() ? namespace().trim() : undefined,
        extends: php() && uses("EXTENDS") ? extendsName().trim() : undefined,
        implements: php() && uses("IMPLEMENTS") ? implementsNames().trim() : undefined,
        variables: vars(),
      });
      close();
      refreshDirs([props.req.dir, dir, parentOf(file.path)]);
      revealPath(parentOf(file.path));
      await openFile(file.path);
    } catch (e) {
      setError(message(e));
      setBusy(false);
    }
  };

  const title = () => (php() ? "New PHP Class" : `New ${template().name}`);
  const focusFirst = (el: HTMLInputElement) => queueMicrotask(() => el.focus());

  return (
    <div class="popup-backdrop modal">
      <div
        class="dialog ft-dialog"
        style={{ width: `min(${php() ? 520 : 420}px, calc(100vw - 32px))` }}
        role="dialog"
        aria-label={title()}
        onKeyDown={(e) => {
          if (e.key === "Escape") close();
          else if (e.key === "Enter" && !(e.target instanceof HTMLSelectElement)) void submit();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">{title()}</div>
        <div class="dialog-body ft-form">
          <label class="form-label" for="ft-name">
            Name:
          </label>
          <input id="ft-name" ref={focusFirst} value={name()} spellcheck={false} onInput={(e) => setName(e.currentTarget.value)} />
          <Show when={php()}>
            <label class="form-label" for="ft-namespace">
              Namespace:
            </label>
            <input
              id="ft-namespace"
              class="ft-mono"
              value={namespace()}
              spellcheck={false}
              onInput={(e) => void onNamespace(e.currentTarget.value)}
            />
            <label class="form-label" for="ft-file">
              File name:
            </label>
            <input id="ft-file" value={effectiveFileName()} spellcheck={false} onInput={(e) => setFileName(e.currentTarget.value)} />
            <label class="form-label" for="ft-dir">
              Directory:
            </label>
            <input
              id="ft-dir"
              class="ft-mono"
              value={directory()}
              placeholder="(project root)"
              spellcheck={false}
              onInput={(e) => setDirectory(e.currentTarget.value)}
              onChange={(e) => void loadNamespace(e.currentTarget.value.trim())}
            />
            <label class="form-label" for="ft-template">
              Template:
            </label>
            <select
              id="ft-template"
              value={template().name}
              onChange={(e) => {
                const next = catalog().templates.find((t) => t.kind === "file" && t.name === e.currentTarget.value);
                if (next) setTemplate(next);
              }}
            >
              <For each={phpClassTemplates(catalog().templates, props.req.template)}>
                {(t) => <option value={t.name}>{templateLabel(t)}</option>}
              </For>
            </select>
            <Show when={uses("EXTENDS")}>
              <label class="form-label" for="ft-extends">
                Extends:
              </label>
              <input
                id="ft-extends"
                class="ft-mono"
                value={extendsName()}
                placeholder={template().name === "PHP Interface" ? "Parent interfaces, comma separated" : "Parent class"}
                spellcheck={false}
                onInput={(e) => setExtends(e.currentTarget.value)}
              />
            </Show>
            <Show when={uses("IMPLEMENTS")}>
              <label class="form-label" for="ft-implements">
                Implements:
              </label>
              <input
                id="ft-implements"
                class="ft-mono"
                value={implementsNames()}
                placeholder="Interfaces, comma separated"
                spellcheck={false}
                onInput={(e) => setImplements(e.currentTarget.value)}
              />
            </Show>
          </Show>
          <For each={template().variables}>
            {(v) => (
              <>
                <label class="form-label" for={`ft-var-${v}`}>
                  {variableLabel(v)}:
                </label>
                <input
                  id={`ft-var-${v}`}
                  value={vars()[v] ?? ""}
                  spellcheck={false}
                  onInput={(e) => setVars({ ...vars(), [v]: e.currentTarget.value })}
                />
              </>
            )}
          </For>
          <Show when={problem() ?? error()}>
            <div class="dialog-error ft-error">{problem() ?? error()}</div>
          </Show>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={close}>
            Cancel
          </button>
          <button class="btn-primary" disabled={!valid()} onClick={() => void submit()}>
            OK
          </button>
        </div>
      </div>
    </div>
  );
}

export function NewFileDialogHost() {
  return <Show when={dialogRequest()} keyed>{(req) => <NewFileDialog req={req} />}</Show>;
}
