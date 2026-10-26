/**
 * Settings | Languages & Frameworks | Go, like GoLand's GOROOT / GOPATH / Go Modules pages
 * in one: the project's SDK (detected SDKs, a folder, Download…), the Go tools Forge uses
 * with Install for those it can install, the schema settings (GOPATH, GOFLAGS, build tags,
 * environment, formatter, annotators, debugger) and gopls from Settings | Language Servers.
 * The SDK choice stays in the Settings draft until Apply.
 */
import { createResource, For, Show } from "solid-js";
import { Download, RefreshCw } from "lucide-solid";
import type { GoToolId } from "@/ipc/go";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { LanguageServersPage } from "@/features/langservers/LanguageServersPage";
import { downloadSdk } from "./sdk";
import { installTool, refreshGoStatus } from "./state";

export function GoSettingsPage() {
  const [status, { refetch }] = createResource(() => refreshGoStatus(true));
  const goroot = () => draftValue<string>("go.goroot") ?? "";
  const install = async (tool: GoToolId) => {
    await installTool(tool);
    await refetch();
  };
  return (
    <div class="go-settings">
      <h3 class="set-section-title">GOROOT</h3>
      <div class="go-sdk-row">
        <select
          data-testid="go-sdk"
          value={goroot()}
          onChange={(e) => setDraft("go.goroot", e.currentTarget.value)}
        >
          <option value="">
            {`Automatic${status()?.sdks[0] ? ` (Go ${status()!.sdks[0].version}, ${status()!.sdks[0].root})` : ": no Go SDK found"}`}
          </option>
          <For each={status()?.sdks ?? []}>
            {(s) => (
              <option value={s.root}>
                Go {s.version} — {s.root} ({s.source})
              </option>
            )}
          </For>
          <Show when={goroot() && !(status()?.sdks ?? []).some((s) => s.root === goroot())}>
            <option value={goroot()}>{goroot()}</option>
          </Show>
        </select>
        <button class="btn-secondary" onClick={() => void downloadSdk().then(() => refetch())}>
          <Download size={13} /> Download…
        </button>
        <button class="icon-btn" title="Detect SDKs again" onClick={() => void refetch()}>
          <RefreshCw size={13} />
        </button>
      </div>

      <h3 class="set-section-title">Tools</h3>
      <table class="go-tools">
        <tbody>
          <For each={status()?.tools ?? []}>
            {(t) => (
              <tr>
                <td>{t.label}</td>
                <td class="go-tool-path" classList={{ "go-missing": !t.path }}>
                  {t.path ?? "not found"}
                </td>
                <td class="muted">{t.licence}</td>
                <td>
                  <Show when={t.install}>
                    <button class="pill-btn" title={`go install ${t.install}`} onClick={() => void install(t.tool)}>
                      {t.path ? "Reinstall" : "Install"}
                    </button>
                  </Show>
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <SchemaPage pageId="lang.go" />
      <h3 class="set-section-title">Language Server</h3>
      <LanguageServersPage groups={["Go"]} />
    </div>
  );
}
