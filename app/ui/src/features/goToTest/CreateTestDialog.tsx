/**
 * PHPStorm's "Create New PHP Test" dialog (Navigate | Test ▸ Create New Test...): the test
 * framework (PHPUnit, or Pest when the project uses it), the class to test, the test class
 * name, its namespace and directory (composer.json `autoload-dev` PSR-4 rules), the
 * superclass, the file name, setUp()/tearDown() and the public methods to generate
 * `test<Method>()` stubs for. JS/TS subjects get "Create New Test" with Vitest or Jest,
 * `describe`/`it` stubs for the module's exports and the project's test file convention.
 *
 * The initial values and the test source are computed in Rust; the backend creates the file
 * and it opens in the editor with the caret in the first test.
 */
import { createSignal, For, onCleanup, Show } from "solid-js";
import { codeApi, type NewTestPlan, type TestFramework, type TestMember } from "@/ipc/codeintel";
import { templatesApi } from "@/ipc/templates";
import { suspendKeymap } from "@/keymap";
import { openFileAt } from "@/editor/views";
import { refreshDirs, revealPath } from "@/features/project/ProjectView";
import { validateClassName, validateNamespace } from "@/features/fileTemplates/model";
import { createTestPlan, dialogTitle, hookNames, isPhpFramework, memberStub, setCreateTestPlan } from "./model";
import "./goToTest.css";

const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");
const message = (e: unknown) => String(e instanceof Error ? e.message : e);

export function CreateTestDialog(props: { plan: NewTestPlan }) {
  const release = suspendKeymap();
  onCleanup(release);

  const plan = props.plan;
  const php = isPhpFramework(plan.framework);
  const [framework, setFramework] = createSignal<TestFramework>(plan.framework);
  const [className, setClassName] = createSignal(plan.className);
  const [namespace, setNamespace] = createSignal(plan.namespace);
  const [superclass, setSuperclass] = createSignal(plan.superclass);
  const [fileName, setFileName] = createSignal<string | null>(null);
  const [directory, setDirectory] = createSignal(plan.directory);
  const [setUp, setSetUp] = createSignal(false);
  const [tearDown, setTearDown] = createSignal(false);
  const [selected, setSelected] = createSignal<Set<string>>(new Set());
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  const isPhpUnit = () => framework() === "PHPUnit";
  /** The PHP file name follows the class name until it is edited. */
  const effectiveFileName = () => fileName() ?? (php && className().trim() ? `${className().trim()}.php` : plan.fileName);
  const hooks = () => hookNames(framework());

  let dirSeq = 0;
  const onNamespace = async (value: string) => {
    setNamespace(value);
    if (validateNamespace(value)) return;
    const id = ++dirSeq;
    const dir = await templatesApi.phpDirectory(value).catch(() => null);
    if (id === dirSeq && dir !== null) setDirectory(dir);
  };

  const toggle = (m: TestMember, on: boolean) => {
    const next = new Set(selected());
    if (on) next.add(m.name);
    else next.delete(m.name);
    setSelected(next);
  };
  const allSelected = () => plan.members.length > 0 && plan.members.every((m) => selected().has(m.name));

  const problem = () => {
    if (isPhpUnit()) {
      if (!className().trim()) return "Specify the test class name";
      return validateClassName(className()) ?? validateNamespace(namespace());
    }
    return effectiveFileName().trim() ? null : "Specify the file name";
  };
  const valid = () => !problem() && !busy();
  const close = () => setCreateTestPlan(null);

  const submit = async () => {
    if (!valid()) return;
    setBusy(true);
    setError(null);
    try {
      const file = await codeApi.createTest({
        framework: framework(),
        subject: plan.subject,
        subjectFqn: plan.subjectFqn,
        subjectPath: plan.subjectPath,
        className: className().trim(),
        namespace: namespace().trim(),
        directory: directory().trim().replace(/^\/+|\/+$/g, ""),
        superclass: superclass().trim(),
        fileName: effectiveFileName().trim(),
        members: plan.members.filter((m) => selected().has(m.name)),
        setUp: setUp(),
        tearDown: tearDown(),
      });
      close();
      refreshDirs([parentOf(file.path)]);
      revealPath(parentOf(file.path));
      await openFileAt(file.path, file.line + 1, null, { flash: false });
    } catch (e) {
      setError(message(e));
      setBusy(false);
    }
  };

  const focusFirst = (el: HTMLElement) => queueMicrotask(() => el.focus());
  const title = dialogTitle(plan);

  return (
    <div class="popup-backdrop modal">
      <div
        class="dialog ft-dialog create-test-dialog"
        style={{ width: "min(560px, calc(100vw - 32px))" }}
        role="dialog"
        aria-label={title}
        onKeyDown={(e) => {
          if (e.key === "Escape") close();
          else if (e.key === "Enter" && !(e.target instanceof HTMLSelectElement) && !(e.target instanceof HTMLButtonElement)) void submit();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">{title}</div>
        <div class="dialog-body ft-form">
          <label class="form-label" for="ct-framework">
            Test framework:
          </label>
          <select id="ct-framework" value={framework()} onChange={(e) => setFramework(e.currentTarget.value as TestFramework)}>
            <For each={plan.frameworks}>{(f) => <option value={f}>{f}</option>}</For>
          </select>
          <label class="form-label">{php ? "Class to test:" : "Module to test:"}</label>
          <div class="ct-subject ft-mono" title={plan.subjectPath}>
            {php ? plan.subjectFqn || plan.subject : plan.subjectPath}
          </div>
          <Show when={isPhpUnit()}>
            <label class="form-label" for="ct-class">
              Test class name:
            </label>
            <input id="ct-class" ref={focusFirst} value={className()} spellcheck={false} onInput={(e) => setClassName(e.currentTarget.value)} />
            <label class="form-label" for="ct-namespace">
              Namespace:
            </label>
            <input
              id="ct-namespace"
              class="ft-mono"
              value={namespace()}
              spellcheck={false}
              onInput={(e) => void onNamespace(e.currentTarget.value)}
            />
            <label class="form-label" for="ct-superclass">
              Superclass:
            </label>
            <input
              id="ct-superclass"
              class="ft-mono"
              list="ct-superclasses"
              value={superclass()}
              spellcheck={false}
              onInput={(e) => setSuperclass(e.currentTarget.value)}
            />
            <datalist id="ct-superclasses">
              <For each={plan.superclasses}>{(s) => <option value={s} />}</For>
            </datalist>
          </Show>
          <label class="form-label" for="ct-file">
            File name:
          </label>
          <input
            id="ct-file"
            ref={(el) => !isPhpUnit() && focusFirst(el)}
            value={effectiveFileName()}
            spellcheck={false}
            onInput={(e) => setFileName(e.currentTarget.value)}
          />
          <label class="form-label" for="ct-dir">
            Directory:
          </label>
          <input
            id="ct-dir"
            class="ft-mono"
            value={directory()}
            placeholder="(project root)"
            spellcheck={false}
            onInput={(e) => setDirectory(e.currentTarget.value)}
          />
          <label class="form-label">Generate:</label>
          <div class="ct-hooks">
            <label class="ct-check">
              <input type="checkbox" checked={setUp()} onChange={(e) => setSetUp(e.currentTarget.checked)} />
              {hooks()[0]}
            </label>
            <label class="ct-check">
              <input type="checkbox" checked={tearDown()} onChange={(e) => setTearDown(e.currentTarget.checked)} />
              {hooks()[1]}
            </label>
          </div>
          <div class="ct-members-label">{php ? "Generate test methods for:" : "Generate tests for:"}</div>
          <div class="ct-members" role="table" aria-label="Members">
            <div class="ct-members-head" role="row">
              <label class="ct-member">
                <input
                  type="checkbox"
                  aria-label="Select all"
                  disabled={!plan.members.length}
                  checked={allSelected()}
                  onChange={(e) => setSelected(new Set(e.currentTarget.checked ? plan.members.map((m) => m.name) : []))}
                />
                <span>Member</span>
              </label>
              <span>Test</span>
            </div>
            <div class="ct-members-body">
              <For each={plan.members} fallback={<div class="ct-members-empty">{php ? "No public methods" : "No exports"}</div>}>
                {(m) => (
                  <div class="ct-members-row" role="row" classList={{ selected: selected().has(m.name) }}>
                    <label class="ct-member">
                      <input type="checkbox" checked={selected().has(m.name)} onChange={(e) => toggle(m, e.currentTarget.checked)} />
                      <span class="ft-mono">{php ? `${m.name}()` : m.name}</span>
                      <Show when={m.defaultExport}>
                        <span class="ct-hint">default</span>
                      </Show>
                    </label>
                    <span class="ct-stub ft-mono">{memberStub(framework(), m.name)}</span>
                  </div>
                )}
              </For>
            </div>
          </div>
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

export function CreateTestDialogHost() {
  return <Show when={createTestPlan()} keyed>{(plan) => <CreateTestDialog plan={plan} />}</Show>;
}
