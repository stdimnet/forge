/**
 * Settings | Build, Execution, Deployment | CMake, like CLion's: the profiles of the project
 * (setting `cmake.profiles`, a Debug profile when there is none) with build type, toolchain,
 * generator, CMake options, build directory, build options and environment, and the
 * configure presets of `CMakePresets.json` / `CMakeUserPresets.json` as read-only profiles
 * (parsed in Rust). Below them the CMake options of the schema (executable, reload on
 * change, build before run). Profiles are edited in the Settings draft (Apply / Cancel).
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { Copy, FileJson, Minus, Plus } from "lucide-solid";
import { cppApi, type Profile, type ProfileEntry } from "@/ipc/cpp";
import type { SettingObject } from "@/ipc/settings";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { SchemaPage } from "@/features/settings/SchemaPage";
import "./cpp.css";

const KEY = "cmake.profiles";
const BUILD_TYPES = ["Debug", "Release", "RelWithDebInfo", "MinSizeRel"];
const GENERATORS = ["", "Ninja", "Ninja Multi-Config", "Unix Makefiles", "MinGW Makefiles", "NMake Makefiles", "Visual Studio 17 2022", "Xcode"];

/** CLion's build folder of a profile name. */
export const defaultBuildDir = (name: string) => `cmake-build-${name.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "default"}`;

const entryOf = (p: Partial<Profile> & { name: string }): ProfileEntry => ({
  name: p.name,
  buildType: p.buildType ?? "Debug",
  toolchain: p.toolchain ?? "",
  generator: p.generator ?? "",
  cmakeOptions: p.cmakeOptions ?? "",
  buildDirectory: p.buildDirectory ?? "",
  buildOptions: p.buildOptions ?? "",
  environment: (p.environment ?? []).join(";"),
  enabled: p.enabled === false ? "false" : "true",
});

function Form(props: { entry: ProfileEntry; onChange: (e: ProfileEntry) => void; toolchains: { id: string; name: string }[] }) {
  const set = (key: keyof ProfileEntry, value: string) => props.onChange({ ...props.entry, [key]: value });
  const text = (key: keyof ProfileEntry, label: string, placeholder = "") => (
    <label class="tc-field">
      <span>{label}:</span>
      <input type="text" value={props.entry[key]} placeholder={placeholder} onInput={(e) => set(key, e.currentTarget.value)} />
    </label>
  );
  return (
    <div class="tc-editor" data-testid="cmake-profile-form">
      <label class="tc-check">
        <input type="checkbox" checked={props.entry.enabled !== "false"} onChange={(e) => set("enabled", e.currentTarget.checked ? "true" : "false")} />
        Enable profile
      </label>
      {text("name", "Name")}
      <label class="tc-field">
        <span>Build type:</span>
        <select value={props.entry.buildType || "Debug"} onChange={(e) => set("buildType", e.currentTarget.value)}>
          <For each={BUILD_TYPES}>{(b) => <option value={b}>{b}</option>}</For>
        </select>
      </label>
      <label class="tc-field">
        <span>Toolchain:</span>
        <select value={props.entry.toolchain} onChange={(e) => set("toolchain", e.currentTarget.value)}>
          <option value="">Use Default ({props.toolchains[0]?.name ?? "none"})</option>
          <For each={props.toolchains}>{(t) => <option value={t.id}>{t.name}</option>}</For>
        </select>
      </label>
      <label class="tc-field">
        <span>Generator:</span>
        <select value={props.entry.generator} onChange={(e) => set("generator", e.currentTarget.value)}>
          <For each={GENERATORS}>{(g) => <option value={g}>{g || "Use default"}</option>}</For>
        </select>
      </label>
      {text("cmakeOptions", "CMake options", "-DSHAPES_TESTS=ON")}
      {text("buildDirectory", "Build directory", defaultBuildDir(props.entry.name))}
      {text("buildOptions", "Build options", "-j 8")}
      {text("environment", "Environment", "CCACHE_DISABLE=1;ASAN_OPTIONS=detect_leaks=0")}
    </div>
  );
}

export function ProfilesPage() {
  const [info] = createResource(() => cppApi.profiles().catch(() => null));
  const [toolchains] = createResource(() => cppApi.toolchains().catch(() => []));
  const saved = () => (draftValue<SettingObject[]>(KEY) ?? []) as unknown as ProfileEntry[];
  // Without saved profiles the page edits CLion's default Debug profile.
  const entries = () => (saved().length ? saved() : [entryOf({ name: "Debug" })]);
  const save = (list: ProfileEntry[]) => setDraft(KEY, list as unknown as SettingObject[]);
  const presets = () => (info()?.profiles ?? []).filter((p) => p.preset);
  const [selected, setSelected] = createSignal("e:0");
  const current = () => {
    const s = selected();
    if (s.startsWith("e:")) {
      const i = Number(s.slice(2));
      return entries()[i] ? { kind: "entry" as const, index: i, entry: entries()[i] } : { kind: "entry" as const, index: 0, entry: entries()[0] };
    }
    const p = presets().find((x) => x.preset === s.slice(2));
    return p ? { kind: "preset" as const, profile: p } : { kind: "entry" as const, index: 0, entry: entries()[0] };
  };
  const currentEntry = () => {
    const c = current();
    return c.kind === "entry" ? c : undefined;
  };
  const currentPreset = () => {
    const c = current();
    return c.kind === "preset" ? c.profile : undefined;
  };
  const add = (from?: ProfileEntry) => {
    const base = from ?? entryOf({ name: entries().some((e) => e.name === "Release") ? `Profile ${entries().length + 1}` : "Release", buildType: "Release" });
    const list = [...entries(), { ...base, name: from ? `${from.name} (copy)` : base.name }];
    save(list);
    setSelected(`e:${list.length - 1}`);
  };
  const tcs = () => (toolchains() ?? []).map((t) => ({ id: t.id, name: t.name }));
  return (
    <div class="cpp-settings">
      <div class="tc-page" data-testid="cmake-profiles-page">
        <div class="tc-list">
          <div class="tc-toolbar">
            <button class="icon-btn" title="Add" onClick={() => add()}>
              <Plus size={14} />
            </button>
            <button
              class="icon-btn"
              title="Remove"
              disabled={current().kind !== "entry" || entries().length < 2}
              onClick={() => {
                const c = current();
                if (c.kind === "entry") save(entries().filter((_, i) => i !== c.index));
                setSelected("e:0");
              }}
            >
              <Minus size={14} />
            </button>
            <button
              class="icon-btn"
              title="Copy"
              onClick={() => {
                const c = current();
                add(c.kind === "entry" ? c.entry : entryOf({ ...c.profile, name: c.profile.name, preset: undefined }));
              }}
            >
              <Copy size={14} />
            </button>
          </div>
          <For each={entries()}>
            {(e, i) => (
              <div class="tree-row tc-item" classList={{ selected: selected() === `e:${i()}`, "tc-disabled": e.enabled === "false" }} data-testid="cmake-profile-row" onClick={() => setSelected(`e:${i()}`)}>
                <span class="tree-label">{e.name || "Unnamed"}</span>
                <span class="tree-hint">{e.buildType}</span>
              </div>
            )}
          </For>
          <For each={presets()}>
            {(p) => (
              <div class="tree-row tc-item" classList={{ selected: selected() === `p:${p.preset}` }} data-testid="cmake-profile-row" onClick={() => setSelected(`p:${p.preset}`)}>
                <FileJson size={13} class="tc-icon" />
                <span class="tree-label">{p.name}</span>
                <span class="tree-hint">preset</span>
              </div>
            )}
          </For>
        </div>
        <div class="tc-body">
          <Show when={currentEntry()}>
            {(c) => <Form entry={c().entry} toolchains={tcs()} onChange={(e) => save(entries().map((x, i) => (i === c().index ? e : x)))} />}
          </Show>
          <Show when={currentPreset()}>
            {(p) => (
              <div class="tc-details" data-testid="cmake-preset-details">
                <div class="tc-note">Loaded from {info()?.presetFiles.join(", ")}. Edit the preset file to change it, or Copy it to a profile.</div>
                <div class="tc-row">
                  <span class="tc-label">Preset:</span>
                  <span class="tc-value">{p().preset}</span>
                </div>
                <div class="tc-row">
                  <span class="tc-label">Build type:</span>
                  <span class="tc-value">{p().buildType}</span>
                </div>
                <div class="tc-row">
                  <span class="tc-label">Generator:</span>
                  <span class="tc-value">{p().generator || "Default"}</span>
                </div>
                <div class="tc-row">
                  <span class="tc-label">Build directory:</span>
                  <span class="tc-value">{p().buildDirectory}</span>
                </div>
                <For each={Object.entries(p().cacheVariables ?? {})}>
                  {([k, v]) => (
                    <div class="tc-row">
                      <span class="tc-label">{k}:</span>
                      <span class="tc-value">{v}</span>
                    </div>
                  )}
                </For>
              </div>
            )}
          </Show>
          <Show when={info()?.presetErrors.length}>
            <div class="tc-problems">
              <For each={info()!.presetErrors}>{(e) => <div class="tc-problem">{e}</div>}</For>
            </div>
          </Show>
        </div>
      </div>
      <SchemaPage pageId="build.cmake" />
    </div>
  );
}
