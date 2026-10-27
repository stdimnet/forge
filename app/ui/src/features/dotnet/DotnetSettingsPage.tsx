/**
 * Settings | Languages & Frameworks | .NET, like Rider's Toolset and Build page: the dotnet
 * host and its SDKs and runtimes, the SDK the solution uses (global.json), the Roslyn
 * language server and netcoredbg with their downloads, then the schema settings (CLI,
 * build, Code Cleanup, Roslyn, debugger, NuGet) and the C# / F# servers of Settings |
 * Language Servers.
 */
import { createResource, For, Show } from "solid-js";
import { Download, RefreshCw } from "lucide-solid";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { LanguageServersPage } from "@/features/langservers/LanguageServersPage";
import { installNetcoredbg, installRoslyn, installSdk } from "./install";
import { refreshDotnetStatus } from "./state";
import "./dotnet.css";

export function DotnetSettingsPage() {
  const [status, { refetch }] = createResource(() => refreshDotnetStatus(true));
  return (
    <div class="dn-settings" data-testid="dotnet-settings">
      <h3 class="set-section-title">.NET SDK</h3>
      <Show when={status()}>
        {(s) => (
          <>
            <div class="dn-facts">
              <span>dotnet</span>
              <span class="dn-mono" classList={{ "dn-missing": !s().host }}>
                {s().host ? `${s().host!.path} (${s().host!.source})` : "not found"}
              </span>
              <span>SDK in use</span>
              <span class="dn-mono" classList={{ "dn-missing": !s().sdk }}>
                {s().sdk ?? (s().requestedSdk ? `${s().requestedSdk} (global.json) is not installed` : "none")}
                <Show when={s().requestedSdk && s().sdk}>
                  <span class="muted"> (global.json asks for {s().requestedSdk})</span>
                </Show>
              </span>
              <span>SDKs</span>
              <span class="dn-mono">{s().sdks.map((x) => x.version).join(", ") || "none"}</span>
              <span>Runtimes</span>
              <span class="dn-mono">{[...new Set(s().runtimes.map((r) => `${r.name.replace(/^Microsoft\./, "")} ${r.version}`))].join(", ") || "none"}</span>
            </div>
            <For each={s().problems}>{(p) => <div class="dn-missing dn-problem-line">{p}</div>}</For>
          </>
        )}
      </Show>
      <div class="dn-row-buttons">
        <button class="btn-secondary" onClick={() => void installSdk()}>
          <Download size={13} /> Install .NET SDK…
        </button>
        <button class="icon-btn" title="Detect again" onClick={() => void refetch()}>
          <RefreshCw size={13} />
        </button>
      </div>

      <h3 class="set-section-title">Tools</h3>
      <table class="dn-tools">
        <tbody>
          <tr>
            <td>Roslyn language server</td>
            <td class="dn-tool-path" classList={{ "dn-missing": !status()?.roslyn }} data-testid="dotnet-roslyn-status">
              {status()?.roslyn ? `${status()!.roslyn!.path}${status()!.roslyn!.version ? ` (${status()!.roslyn!.version})` : ""}` : (status()?.roslynProblem ?? "not installed")}
            </td>
            <td class="muted">MIT</td>
            <td>
              <Show when={!status()?.roslyn && !status()?.remote}>
                <button class="pill-btn" onClick={() => void installRoslyn().then(() => refetch())}>
                  Download…
                </button>
              </Show>
            </td>
          </tr>
          <tr>
            <td>netcoredbg</td>
            <td class="dn-tool-path" classList={{ "dn-missing": !status()?.netcoredbg }} data-testid="dotnet-netcoredbg-status">
              {status()?.netcoredbg ? status()!.netcoredbg!.path : (status()?.netcoredbgProblem ?? "not installed")}
            </td>
            <td class="muted">MIT</td>
            <td>
              <Show when={!status()?.netcoredbg && !status()?.remote}>
                <button class="pill-btn" onClick={() => void installNetcoredbg().then(() => refetch())}>
                  Download…
                </button>
              </Show>
            </td>
          </tr>
        </tbody>
      </table>
      <SchemaPage pageId="lang.dotnet" />
      <h3 class="set-section-title">Language Servers</h3>
      <LanguageServersPage groups={["C#", "F#"]} />
    </div>
  );
}
