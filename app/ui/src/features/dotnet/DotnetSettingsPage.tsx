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
import { t } from "@/i18n";
import "./dotnet.css";

export function DotnetSettingsPage() {
  const [status, { refetch }] = createResource(() => refreshDotnetStatus(true));
  return (
    <div class="dn-settings" data-testid="dotnet-settings">
      <h3 class="set-section-title">{t("langs.dotnet.netSdk")}</h3>
      <Show when={status()}>
        {(s) => (
          <>
            <div class="dn-facts">
              <span>dotnet</span>
              <span class="dn-mono" classList={{ "dn-missing": !s().host }}>
                {s().host ? `${s().host!.path} (${s().host!.source})` : t("common.notFound")}
              </span>
              <span>{t("langs.dotnet.sdkInUse")}</span>
              <span class="dn-mono" classList={{ "dn-missing": !s().sdk }}>
                {s().sdk ?? (s().requestedSdk ? t("langs.dotnet.globalJsonIsNotInstalled", { requestedSdk: s().requestedSdk }) : "none")}
                <Show when={s().requestedSdk && s().sdk}>
                  <span class="muted"> {t("langs.dotnet.globalJsonAsksFor", { requestedSdk: s().requestedSdk })}</span>
                </Show>
              </span>
              <span>{t("langs.dotnet.sdks")}</span>
              <span class="dn-mono">{s().sdks.map((x) => x.version).join(", ") || "none"}</span>
              <span>{t("langs.dotnet.runtimes")}</span>
              <span class="dn-mono">{[...new Set(s().runtimes.map((r) => `${r.name.replace(/^Microsoft\./, "")} ${r.version}`))].join(", ") || "none"}</span>
            </div>
            <For each={s().problems}>{(p) => <div class="dn-missing dn-problem-line">{p}</div>}</For>
          </>
        )}
      </Show>
      <div class="dn-row-buttons">
        <button class="btn-secondary" onClick={() => void installSdk()}>
          <Download size={13} /> {t("langs.dotnet.installNetSdkEllipsis")}
        </button>
        <button class="icon-btn" title={t("common.detectAgain")} onClick={() => void refetch()}>
          <RefreshCw size={13} />
        </button>
      </div>

      <h3 class="set-section-title">{t("common.tools")}</h3>
      <table class="dn-tools">
        <tbody>
          <tr>
            <td>{t("langs.dotnet.roslynLanguageServer")}</td>
            <td class="dn-tool-path" classList={{ "dn-missing": !status()?.roslyn }} data-testid="dotnet-roslyn-status">
              {status()?.roslyn ? `${status()!.roslyn!.path}${status()!.roslyn!.version ? ` (${status()!.roslyn!.version})` : ""}` : (status()?.roslynProblem ?? t("common.notInstalled2"))}
            </td>
            <td class="muted">MIT</td>
            <td>
              <Show when={!status()?.roslyn && !status()?.remote}>
                <button class="pill-btn" onClick={() => void installRoslyn().then(() => refetch())}>
                  {t("common.downloadEllipsis")}
                </button>
              </Show>
            </td>
          </tr>
          <tr>
            <td>netcoredbg</td>
            <td class="dn-tool-path" classList={{ "dn-missing": !status()?.netcoredbg }} data-testid="dotnet-netcoredbg-status">
              {status()?.netcoredbg ? status()!.netcoredbg!.path : (status()?.netcoredbgProblem ?? t("common.notInstalled2"))}
            </td>
            <td class="muted">MIT</td>
            <td>
              <Show when={!status()?.netcoredbg && !status()?.remote}>
                <button class="pill-btn" onClick={() => void installNetcoredbg().then(() => refetch())}>
                  {t("common.downloadEllipsis")}
                </button>
              </Show>
            </td>
          </tr>
        </tbody>
      </table>
      <SchemaPage pageId="lang.dotnet" />
      <h3 class="set-section-title">{t("langs.dotnet.languageServers")}</h3>
      <LanguageServersPage groups={["C#", "F#"]} />
    </div>
  );
}
