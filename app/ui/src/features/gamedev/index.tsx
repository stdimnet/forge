/**
 * DotNetForge for game engines: Unity (`unity.tsx`) and Godot (`godot.tsx`), their file
 * types (`syntax.ts`) and Settings | Languages & Frameworks | .NET | Unity / Godot. Logic
 * lives in Rust (`packs/dotnetforge/gamedev`, `commands/gamedev.rs`).
 */
import { For, Show } from "solid-js";
import { Terminal } from "lucide-solid";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerRunConfigurationType } from "@/features/run/types";
import { godotStatus, refreshGodot } from "./godot";
import { refreshUnity, unityStatus } from "./unity";
import "./syntax";
import "./gamedev.css";
import { t } from "@/i18n";

/** Unity and Godot command lines built in Rust (Start Unity, project files, the Godot editor). */
registerRunConfigurationType({
  id: "gamedev-command",
  name: "Game Engine",
  icon: Terminal,
  hidden: true,
  fields: [],
  argumentsLabel: false,
  buildCommand: (cfg) => JSON.parse(cfg.options?.commandLine ?? "{}"),
});

function UnityPage() {
  void refreshUnity();
  const s = () => unityStatus();
  return (
    <div class="gd-settings">
      <div class="gd-settings-info" data-testid="unity-settings-info">
        <Show when={s()?.project} fallback={<span class="muted">The open project is not a Unity project.</span>}>
          {(p) => (
            <>
              <span class="muted">Project</span>
              <span>Unity {p().version}</span>
              <span class="muted">Installed editors</span>
              <span>
                <For each={s()?.editors ?? []} fallback={<span class="muted">None found (Unity Hub folders, secondaryInstallPath.json, editors-v2.json)</span>}>
                  {(e) => (
                    <div class="mono gd-editor" classList={{ current: e.path === s()?.editor?.path }}>
                      {e.version} — {e.path}
                    </div>
                  )}
                </For>
              </span>
              <span class="muted">Editor bridge</span>
              <span>{p().bridge.kind === "none" ? `Not installed (bundled: ${s()?.bundledBridge})` : `dev.forge.unity ${p().bridge.version ?? ""}`}</span>
            </>
          )}
        </Show>
      </div>
      <SchemaPage pageId="lang.dotnet.unity" />
    </div>
  );
}

function GodotPage() {
  void refreshGodot();
  const s = () => godotStatus();
  return (
    <div class="gd-settings">
      <div class="gd-settings-info" data-testid="godot-settings-info">
        <span class="muted">Godot</span>
        <span class="mono">{s()?.godot ? `${s()!.godot!.path}${s()?.version ? ` (${s()!.version!.version}${s()!.version!.dotnet ? " .NET" : ""})` : ""}` : "Not found"}</span>
        <Show when={s()?.project}>
          {(p) => (
            <>
              <span class="muted">Project</span>
              <span>
                {p().name} — Godot {p().version ?? "?"}
                {p().csharp ? ", C#" : ""}
              </span>
              <span class="muted">Godot editor</span>
              <span>{s()?.lspListening ? `Language server on ${s()!.host}:${s()!.lspPort}` : "Not running (no language server answers)"}</span>
            </>
          )}
        </Show>
      </div>
      <SchemaPage pageId="lang.dotnet.godot" />
    </div>
  );
}

registerSettingsPage({
  id: "lang.dotnet.unity",
  title: t("settings.page.lang.dotnet.unity"),
  group: "Languages & Frameworks",
  parent: "lang.dotnet",
  order: 10,
  keywords: ["unity", "unity hub", "editor bridge", "asmdef", "shader", "hlsl", "unity tests", "mono debugger"],
  component: UnityPage,
});

registerSettingsPage({
  id: "lang.dotnet.godot",
  title: t("settings.page.lang.dotnet.godot"),
  group: "Languages & Frameworks",
  parent: "lang.dotnet",
  order: 11,
  keywords: ["godot", "gdscript", "gdunit4", "tscn", "language server", "debug adapter"],
  component: GodotPage,
});
