/**
 * Settings | Languages & Frameworks | Rust, like RustRover's Rust page: the toolchain
 * (rustup's choice with the reason — default, rust-toolchain.toml, an override — or one
 * chosen for the project), its sysroot and standard library sources, the tools Forge uses
 * with Install for missing ones, then the schema settings (Cargo, tests, rustfmt, Clippy,
 * rust-analyzer, debugger) and rust-analyzer from Settings | Language Servers. The
 * toolchain choice stays in the Settings draft until Apply.
 */
import { createResource, For, Show } from "solid-js";
import { Download, RefreshCw } from "lucide-solid";
import type { ActiveReason, RustToolId } from "@/ipc/rust";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { LanguageServersPage } from "@/features/langservers/LanguageServersPage";
import { installAnalyzer, installCargoPlugin, installComponent, installToolchain } from "./install";
import { refreshRustStatus } from "./state";
import { t as tr } from "@/i18n";
import "./cargo.css";

export function reasonLabel(r: ActiveReason | undefined): string {
  switch (r?.kind) {
    case "default":
      return "rustup default";
    case "file":
      return `overridden by ${r.path.split(/[\\/]/).pop()}`;
    case "directory":
      return "directory override";
    case "environment":
      return "RUSTUP_TOOLCHAIN";
    case "other":
      return r.text;
    default:
      return "";
  }
}

export function RustSettingsPage() {
  const [status, { refetch }] = createResource(() => refreshRustStatus(true));
  const chosen = () => draftValue<string>("rust.toolchain") ?? "";
  const install = async (tool: RustToolId) => {
    if (tool === "rust-analyzer") await installAnalyzer();
    else if (tool === "cargo-nextest" || tool === "cargo-llvm-cov") await installCargoPlugin(tool);
    else await installComponent(tool);
    await refetch();
  };
  const channelOf = (name: string) => name.replace(/-(x86_64|aarch64|i686|armv7|riscv64gc)-.*$/, "");
  return (
    <div class="rust-settings">
      <h3 class="set-section-title">{tr("langs.rust.toolchain")}</h3>
      <div class="rust-row">
        <select data-testid="rust-toolchain" value={chosen()} onChange={(e) => setDraft("rust.toolchain", e.currentTarget.value)}>
          <option value="">
            {tr("langs.rust.rustupSChoice", { value: status()?.active ? ` (${status()!.active!.name}, ${reasonLabel(status()!.active!.reason)})` : ": no toolchain found" })}
          </option>
          <For each={(status()?.toolchains ?? []).filter((t) => t.source === "rustup")}>
            {(t) => <option value={channelOf(t.name)}>{t.name}</option>}
          </For>
        </select>
        <button class="btn-secondary" onClick={() => installToolchain()}>
          <Download size={13} /> {tr("common.installEllipsis")}
        </button>
        <button class="icon-btn" title={tr("langs.rust.detectToolchainsAgain")} onClick={() => void refetch()}>
          <RefreshCw size={13} />
        </button>
      </div>
      <Show when={status()}>
        {(s) => (
          <div class="rust-facts">
            <span>rustc</span>
            <span class="rust-mono">{s().rustc ? `${s().rustc!.release} (${s().rustc!.host})` : tr("common.notFound")}</span>
            <span>{tr("langs.rust.toolchainFile")}</span>
            <span class="rust-mono">
              {s().toolchainFile ? `${s().toolchainFile!.path}: ${s().toolchainFile!.file?.channel || s().toolchainFile!.error || ""}` : "none"}
            </span>
            <span>{tr("langs.rust.sysroot")}</span>
            <span class="rust-mono" title={s().sysroot ?? ""}>
              {s().sysroot ?? "unknown"}
            </span>
            <span>{tr("langs.rust.standardLibrary")}</span>
            <span class="rust-mono" classList={{ "rust-missing": !s().stdlibSources }}>
              {s().stdlibSources ?? tr("langs.rust.rustSrcIsNotInstalled")}
              <Show when={!s().stdlibSources && s().rustup}>
                {" "}
                <button class="pill-btn" onClick={() => void installComponent("rust-src").then(() => refetch())}>
                  {tr("common.install")}
                </button>
              </Show>
            </span>
          </div>
        )}
      </Show>

      <h3 class="set-section-title">{tr("common.tools")}</h3>
      <table class="rust-tools">
        <tbody>
          <For each={status()?.tools ?? []}>
            {(t) => (
              <tr>
                <td>{t.label}</td>
                <td class="rust-tool-path" classList={{ "rust-missing": !t.path }}>
                  {t.path ?? tr("common.notFound")}
                </td>
                <td class="muted">{t.licence}</td>
                <td>
                  <Show when={t.install && !t.path}>
                    <button class="pill-btn" onClick={() => void install(t.tool)}>
                      {t.install === "download" ? tr("common.downloadEllipsis") : tr("common.install")}
                    </button>
                  </Show>
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <SchemaPage pageId="lang.rust" />
      <h3 class="set-section-title">{tr("common.languageServer")}</h3>
      <LanguageServersPage groups={["Rust"]} />
    </div>
  );
}
