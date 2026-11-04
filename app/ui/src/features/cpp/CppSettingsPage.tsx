/**
 * Settings | Languages & Frameworks | C/C++: clangd (where it comes from, its version and
 * Download clangd for the pinned release), clang-format, the compilation database clangd
 * reads (the active CMake profile's build folder, or a compile_commands.json of a project
 * without CMake) and the schema settings (clangd options, formatter, debugger renderers).
 */
import { createResource, Show } from "solid-js";
import { Download, RefreshCw } from "lucide-solid";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { installClangd } from "./clangd";
import { refreshCppStatus } from "./state";
import { t } from "@/i18n";
import "./cpp.css";

const SOURCE: Record<string, string> = {
  setting: "chosen below",
  toolchain: "from the toolchain",
  path: "on the PATH",
  managed: "downloaded by Forge",
};

export function CppSettingsPage() {
  const [status, { refetch }] = createResource(() => refreshCppStatus(true));
  return (
    <div class="cpp-settings" data-testid="cpp-settings">
      <h3 class="set-section-title">{t("langs.cpp.languageServerAndTools")}</h3>
      <div class="cpp-status-row">
        <Show when={status()?.clangd.path} fallback={<span class="tc-missing">{t("langs.cpp.clangdIsNotInstalled")}</span>}>
          <span class="tc-value">{status()!.clangd.path}</span>
          <span class="muted">
            {status()!.clangd.version ? `clangd ${status()!.clangd.version}` : ""} {SOURCE[status()!.clangd.source] ? `(${SOURCE[status()!.clangd.source]})` : ""}
          </span>
        </Show>
        <Show when={status()?.clangd.pinnedVersion && !status()?.remote}>
          <button class="btn-secondary" onClick={() => void installClangd().then(() => refetch())}>
            <Download size={13} /> {t("langs.cpp.downloadClangd", { pinnedVersion: status()!.clangd.pinnedVersion })}
          </button>
        </Show>
        <button class="icon-btn" title={t("langs.cpp.lookAgain")} onClick={() => void refetch()}>
          <RefreshCw size={13} />
        </button>
      </div>
      <div class="cpp-status-row">
        <span class="tc-label">{t("langs.cpp.compilationDatabaseLabel")}</span>
        <Show when={status()?.compileCommandsDir} fallback={<span class="muted">{t("langs.cpp.noneBuildFilesAreGuessed")}</span>}>
          <span class="tc-value">{status()!.compileCommandsDir}</span>
          <Show when={status()?.compileCommands}>
            {(c) => (
              <span class="muted">
                {c().files} files{c().standards.length ? `, ${c().standards.join(", ")}` : ""}
              </span>
            )}
          </Show>
        </Show>
      </div>
      <div class="cpp-status-row">
        <span class="tc-label">clang-format:</span>
        <span class="tc-value" classList={{ "tc-missing": !status()?.clangFormat }}>
          {status()?.clangFormat ?? t("common.notFound")}
        </span>
      </div>
      <SchemaPage pageId="lang.cpp" />
    </div>
  );
}
