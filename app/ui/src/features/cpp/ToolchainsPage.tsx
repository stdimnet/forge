/**
 * Settings | Build, Execution, Deployment | Toolchains, like CLion's: the toolchains list
 * (the user's from `cpp.toolchains` first, the first one is the default; then the detected
 * ones: system GCC / Clang, MinGW, Visual Studio, cross compilers) and the selected
 * toolchain's CMake, build tool, C and C++ compilers and debugger with the versions Rust
 * read from them (`cforge-toolchains`). Added toolchains are edited in the Settings draft
 * (Apply / Cancel) and checked live (`cpp_toolchain_inspect`).
 */
import { createEffect, createResource, createSignal, For, on, Show } from "solid-js";
import { CircleCheck, CircleX, Copy, Cpu, Minus, Plus, RefreshCw } from "lucide-solid";
import { cppApi, type Toolchain, type ToolchainEntry } from "@/ipc/cpp";
import type { SettingObject } from "@/ipc/settings";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import "./cpp.css";

const KEY = "cpp.toolchains";

const KIND_LABEL: Record<string, string> = {
  system: "System",
  minGw: "MinGW",
  mingw: "MinGW",
  visualStudio: "Visual Studio",
  cross: "Cross compiler",
  custom: "Custom",
};

const EMPTY: ToolchainEntry = { name: "", kind: "system", cCompiler: "", cxxCompiler: "", cmake: "", make: "", ninja: "", debugger: "", environmentScript: "" };

function Check(props: { label: string; path?: string; version?: string; detail?: string }) {
  return (
    <div class="tc-row">
      <span class="tc-label">{props.label}:</span>
      <span class="tc-value" classList={{ "tc-missing": !props.path }}>
        {props.path ?? "Not found"}
      </span>
      <span class="tc-status" title={props.path ? props.version : "Not found"}>
        <Show when={props.path} fallback={<CircleX size={13} class="tc-bad" />}>
          <CircleCheck size={13} class="tc-good" />
          <span class="muted">{props.detail ?? (props.version ? `Version: ${props.version}` : "")}</span>
        </Show>
      </span>
    </div>
  );
}

function Details(props: { t: Toolchain }) {
  const t = () => props.t;
  const build = () => t().ninja ?? t().make;
  return (
    <div class="tc-details" data-testid="toolchain-details">
      <Check label="CMake" path={t().cmake?.path} version={t().cmake?.version} />
      <Check label="Build Tool" path={build()?.path} version={build()?.version} />
      <Check label="C Compiler" path={t().cCompiler} detail={t().compiler ? `${t().compiler!.family === "msvc" ? "MSVC" : t().compiler!.family === "gcc" ? "GNU" : "Clang"} ${t().compiler!.version}` : undefined} />
      <Check label="C++ Compiler" path={t().cxxCompiler} detail={t().compiler ? `${t().compiler!.target}${t().compiler!.vendor ? ` (${t().compiler!.vendor})` : ""}` : undefined} />
      <Check label="Debugger" path={t().debugger?.path} detail={t().debugger ? `${t().debugger!.kind === "gdb" ? "GDB" : "LLDB"} ${t().debugger!.version}` : undefined} />
      <Show when={t().environmentScript}>
        <div class="tc-row">
          <span class="tc-label">Environment:</span>
          <span class="tc-value">{t().environmentScript}</span>
        </div>
      </Show>
      <Show when={t().problems?.length}>
        <div class="tc-problems">
          <For each={t().problems}>{(p) => <div class="tc-problem">{p}</div>}</For>
        </div>
      </Show>
    </div>
  );
}

function Editor(props: { index: number; entry: ToolchainEntry; onChange: (e: ToolchainEntry) => void }) {
  const [checked, setChecked] = createSignal<Toolchain | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  createEffect(
    on(
      () => JSON.stringify(props.entry),
      () => {
        clearTimeout(timer);
        timer = setTimeout(() => void cppApi.inspectToolchain(props.entry).then(setChecked).catch(() => setChecked(null)), 300);
      },
    ),
  );
  const field = (key: keyof ToolchainEntry, label: string, placeholder = "Detect") => (
    <label class="tc-field">
      <span>{label}:</span>
      <input type="text" value={props.entry[key]} placeholder={placeholder} onInput={(e) => props.onChange({ ...props.entry, [key]: e.currentTarget.value })} />
    </label>
  );
  return (
    <div class="tc-editor" data-testid="toolchain-editor">
      {field("name", "Name", "")}
      <label class="tc-field">
        <span>Kind:</span>
        <select value={props.entry.kind || "system"} onChange={(e) => props.onChange({ ...props.entry, kind: e.currentTarget.value })}>
          <For each={["system", "mingw", "visualStudio", "cross", "custom"]}>{(k) => <option value={k}>{KIND_LABEL[k]}</option>}</For>
        </select>
      </label>
      {field("cmake", "CMake", "Bundled or on the PATH")}
      {field("make", "Build Tool", "Detect (Make)")}
      {field("ninja", "Ninja", "Detect")}
      {field("cCompiler", "C Compiler")}
      {field("cxxCompiler", "C++ Compiler")}
      {field("debugger", "Debugger", "GDB or LLDB of the compiler")}
      {field("environmentScript", "Environment script", "vcvarsall.bat, environment-setup-*")}
      <Show when={checked()}>{(t) => <Details t={t()} />}</Show>
    </div>
  );
}

export function ToolchainsPage() {
  const [detected, { refetch }] = createResource(() => cppApi.toolchains(true).catch(() => [] as Toolchain[]));
  const entries = () => (draftValue<SettingObject[]>(KEY) ?? []) as unknown as ToolchainEntry[];
  const save = (list: ToolchainEntry[]) => setDraft(KEY, list as unknown as SettingObject[]);
  const [selected, setSelected] = createSignal("0");
  const rows = () => [
    ...entries().map((e, i) => ({ key: `${i}`, name: e.name || "Unnamed", kind: e.kind, entry: e, index: i, toolchain: undefined as Toolchain | undefined })),
    ...(detected() ?? [])
      .filter((t) => t.source === "detected")
      .map((t) => ({ key: t.id, name: t.name, kind: t.kind as string, entry: undefined, index: -1, toolchain: t })),
  ];
  const current = () => rows().find((r) => r.key === selected()) ?? rows()[0];
  const add = (from?: ToolchainEntry) => {
    const list = [...entries(), { ...(from ?? EMPTY), name: from ? `${from.name} (copy)` : `Toolchain ${entries().length + 1}` }];
    save(list);
    setSelected(`${list.length - 1}`);
  };
  const copyDetected = (t: Toolchain): ToolchainEntry => ({
    ...EMPTY,
    name: t.name,
    kind: t.kind === "minGw" ? "mingw" : t.kind,
    cCompiler: t.cCompiler ?? "",
    cxxCompiler: t.cxxCompiler ?? "",
    cmake: t.cmake?.path ?? "",
    make: t.make?.path ?? "",
    ninja: t.ninja?.path ?? "",
    debugger: t.debugger?.path ?? "",
    environmentScript: t.environmentScript ?? "",
  });
  return (
    <div class="tc-page" data-testid="toolchains-page">
      <div class="tc-list">
        <div class="tc-toolbar">
          <button class="icon-btn" title="Add" onClick={() => add()}>
            <Plus size={14} />
          </button>
          <button
            class="icon-btn"
            title="Remove"
            disabled={!current()?.entry}
            onClick={() => {
              const r = current();
              if (r?.entry) save(entries().filter((_, i) => i !== r.index));
              setSelected("0");
            }}
          >
            <Minus size={14} />
          </button>
          <button class="icon-btn" title="Copy" disabled={!current()} onClick={() => {
            const r = current();
            if (r) add(r.entry ?? copyDetected(r.toolchain!));
          }}>
            <Copy size={14} />
          </button>
          <button class="icon-btn" title="Detect again" onClick={() => void refetch()}>
            <RefreshCw size={14} />
          </button>
        </div>
        <For each={rows()}>
          {(r, i) => (
            <div class="tree-row tc-item" classList={{ selected: current()?.key === r.key }} data-testid="toolchain-row" onClick={() => setSelected(r.key)}>
              <Cpu size={14} class="tc-icon" />
              <span class="tree-label">{r.name}</span>
              <span class="tree-hint">{i() === 0 ? "(default)" : KIND_LABEL[r.kind] ?? ""}</span>
            </div>
          )}
        </For>
        <Show when={detected.loading}>
          <div class="muted cmake-pad">Detecting toolchains…</div>
        </Show>
      </div>
      <div class="tc-body">
        <Show when={current()} fallback={<div class="muted cmake-pad">No toolchains found. Install GCC or Clang with CMake, or add a toolchain.</div>}>
          {(r) => (
            <Show
              when={r().entry}
              fallback={
                <>
                  <div class="tc-head">
                    <span class="tc-name">{r().name}</span>
                    <span class="muted">{KIND_LABEL[r().kind]} · detected</span>
                  </div>
                  <Details t={r().toolchain!} />
                </>
              }
            >
              <Editor
                index={r().index}
                entry={r().entry!}
                onChange={(e) => save(entries().map((x, i) => (i === r().index ? e : x)))}
              />
            </Show>
          )}
        </Show>
      </div>
    </div>
  );
}
