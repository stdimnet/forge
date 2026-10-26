/**
 * Settings | Project | Python Interpreter, like PyCharm's page: the project's interpreter
 * (configured interpreters, the project's environments and the Pythons found on this
 * machine), "Add Interpreter…" and the interpreter's packages with their latest PyPI
 * versions. The choice stays in the Settings draft until Apply.
 */
import { createEffect, createMemo, createResource, createSignal, For, on, Show } from "solid-js";
import { ArrowUpCircle, Package, Plus, Trash2 } from "lucide-solid";
import { pythonApi, type DetectedPython, type PythonInterpreter } from "@/ipc/python";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { showToolWindow } from "@/state/layout";
import { AddInterpreterDialog, type AddResult } from "./AddInterpreterDialog";
import { newer } from "./PackagesView";
import { currentInterpreter, fromSetting, KIND_LABELS, labelOf, packageListing, packagesLoading, toSetting } from "./state";

const norm = (n: string) => n.toLowerCase().replace(/[-_.]+/g, "-");

export function InterpreterPage() {
  const [adding, setAdding] = createSignal(false);
  const [detected] = createResource(() => pythonApi.detect().catch((): DetectedPython[] => []));
  const [latest, setLatest] = createSignal<Record<string, string>>({});
  const selected = () => draftValue<string>("python.interpreter") ?? "";
  const configured = createMemo((): PythonInterpreter[] => {
    const read = (key: string, project: boolean) =>
      ((draftValue<Record<string, string>[]>(key) ?? []) as Record<string, string>[]).map((t) => fromSetting(t, project)).filter((i) => i.name);
    return [...read("python.projectInterpreters", true), ...read("python.interpreters", false)];
  });
  createEffect(
    on(packageListing, (l) => {
      if (!l?.packages.length) return;
      void pythonApi
        .latest(l.packages.map((p) => p.name))
        .then((m) => setLatest(Object.fromEntries(Object.entries(m).map(([k, v]) => [norm(k), v]))))
        .catch(() => undefined);
    }),
  );

  const automatic = () => !selected() && currentInterpreter()?.automatic;

  const done = (r: AddResult) => {
    setAdding(false);
    if (r.add) {
      const list = (draftValue<Record<string, string>[]>("python.projectInterpreters") ?? []) as Record<string, string>[];
      setDraft("python.projectInterpreters", [...list.filter((t) => t.name !== r.add!.name), toSetting(r.add)]);
    }
    setDraft("python.interpreter", r.select);
  };

  const removeSelected = () => {
    const name = selected();
    for (const key of ["python.projectInterpreters", "python.interpreters"]) {
      const list = (draftValue<Record<string, string>[]>(key) ?? []) as Record<string, string>[];
      if (list.some((t) => t.name === name)) setDraft(key, list.filter((t) => t.name !== name));
    }
    setDraft("python.interpreter", "");
  };

  const isConfigured = () => configured().some((i) => i.name === selected());
  const known = () =>
    !selected() || isConfigured() || (detected() ?? []).some((d) => d.path === selected());

  return (
    <div class="set-page py-interp-page" data-testid="python-interpreter-page">
      <div class="py-interp-picker">
        <label for="py-interp-select">Python Interpreter:</label>
        <select id="py-interp-select" value={selected()} onChange={(e) => setDraft("python.interpreter", e.currentTarget.value)}>
          <option value="">{automatic() ? `${currentInterpreter()!.label} (project environment)` : "<No interpreter>"}</option>
          <Show when={!known()}>
            <option value={selected()}>{selected()}</option>
          </Show>
          <Show when={configured().length}>
            <optgroup label="Configured">
              <For each={configured()}>
                {(i) => (
                  <option value={i.name}>
                    {labelOf(i)} [{KIND_LABELS[i.kind]}]
                  </option>
                )}
              </For>
            </optgroup>
          </Show>
          <optgroup label="Detected">
            <For each={detected() ?? []}>
              {(d) => (
                <option value={d.path}>
                  {labelOf(d)} {d.path} [{KIND_LABELS[d.kind]}]
                </option>
              )}
            </For>
          </optgroup>
        </select>
        <button class="btn-secondary" onClick={() => setAdding(true)} data-testid="python-add-interpreter-button">
          <Plus size={13} /> Add Interpreter…
        </button>
        <Show when={isConfigured()}>
          <button class="icon-btn" title="Remove Interpreter" onClick={removeSelected}>
            <Trash2 size={14} />
          </button>
        </Show>
      </div>
      <Show when={currentInterpreter()}>
        {(cur) => (
          <div class="py-interp-path muted">
            {cur().interpreter.path}
            <Show when={cur().interpreter.version}> · Python {cur().interpreter.version}</Show>
            <Show when={packageListing()}> · packages managed with {packageListing()!.manager}</Show>
          </div>
        )}
      </Show>
      <div class="py-interp-table-head">
        <span>Package</span>
        <span>Version</span>
        <span>Latest version</span>
      </div>
      <div class="py-interp-table">
        <Show when={!packagesLoading()} fallback={<div class="muted py-interp-empty">Loading packages…</div>}>
          <For each={packageListing()?.packages ?? []} fallback={<div class="muted py-interp-empty">No packages</div>}>
            {(p) => (
              <div class="py-interp-table-row">
                <span>
                  <Package size={12} /> {p.name}
                </span>
                <span>{p.version}</span>
                <span classList={{ "py-update": newer(latest()[norm(p.name)], p.version) }}>
                  <Show when={newer(latest()[norm(p.name)], p.version)}>
                    <ArrowUpCircle size={12} />{" "}
                  </Show>
                  {latest()[norm(p.name)] ?? ""}
                </span>
              </div>
            )}
          </For>
        </Show>
      </div>
      <div class="py-interp-foot muted">
        Install, upgrade and remove packages in the{" "}
        <button class="link-btn" onClick={() => showToolWindow("pythonPackages")}>
          Python Packages
        </button>{" "}
        tool window.
      </div>
      <Show when={adding()}>
        <AddInterpreterDialog onDone={done} onClose={() => setAdding(false)} />
      </Show>
    </div>
  );
}
