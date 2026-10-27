/**
 * Settings | Languages & Frameworks | Kotlin: the language server (kotlin-lsp or
 * kotlin-language-server, its version, what is missing) with Install, ktlint, detekt and
 * kotlinc as found, and the `kotlin.*` options.
 */
import { createSignal, onMount, Show } from "solid-js";
import { CircleAlert, CircleCheck } from "lucide-solid";
import { SchemaPage } from "@/features/settings/SchemaPage";
import type { ToolInfo } from "@/ipc/kotlin";
import { activeInstall, kotlinStatus, megabytes, refreshStatus } from "./store";
import { installLanguageServer, installLinters } from "./install";
import "./kotlin.css";

const SOURCES: Record<string, string> = {
  setting: "from the settings",
  path: "on the PATH",
  managed: "downloaded by Forge",
  gradle: "the project's Gradle plugin",
};

function ToolRow(props: { label: string; tool: ToolInfo | null | undefined }) {
  return (
    <div class="java-status-row">
      <Show when={props.tool} fallback={<CircleAlert size={14} class="java-warn" />}>
        <CircleCheck size={14} class="java-ok" />
      </Show>
      <span>
        {props.label}: <b>{props.tool ? SOURCES[props.tool.source] ?? props.tool.source : "not found"}</b>
        <Show when={props.tool?.path}>
          <span class="muted kotlin-path"> {props.tool!.path}</span>
        </Show>
      </span>
    </div>
  );
}

export function KotlinPage() {
  const [busy, setBusy] = createSignal(false);
  onMount(() => void refreshStatus());
  const run = async (f: () => Promise<void>) => {
    setBusy(true);
    try {
      await f();
    } finally {
      setBusy(false);
    }
  };
  const s = kotlinStatus;
  const serverLabel = () => (s()?.server === "kotlin-language-server" ? "Kotlin Language Server (fwcd)" : "Kotlin LSP (JetBrains)");
  return (
    <div class="java-page" data-kotlin-page>
      <div class="java-status" data-kotlin-status>
        <div class="java-status-row">
          <Show when={s()?.serverPath} fallback={<CircleAlert size={14} class="java-warn" />}>
            <CircleCheck size={14} class="java-ok" />
          </Show>
          <span>
            {serverLabel()}: <b>{s()?.serverPath ? (s()?.serverVersion ?? "installed") : "not installed"}</b>
            <Show when={s()?.serverPath}>
              <span class="muted kotlin-path"> {s()!.serverPath}</span>
            </Show>
          </span>
        </div>
        <ToolRow label="ktlint" tool={s()?.ktlint} />
        <ToolRow label="detekt" tool={s()?.detekt} />
        <ToolRow label="Kotlin compiler (kotlinc)" tool={s()?.kotlinc ? { source: "path", path: s()!.kotlinc! } : null} />
        <Show when={s()?.problems.length}>
          <p class="java-error">{s()!.problems.join(" ")}</p>
        </Show>
        <Show when={busy() && activeInstall()}>
          <div class="java-progress">
            <span>{activeInstall()!.label}</span>
            <Show when={activeInstall()!.total}>
              <progress max={activeInstall()!.total!} value={activeInstall()!.done} />
            </Show>
          </div>
        </Show>
        <div class="java-buttons java-buttons-left">
          <Show when={s()?.server !== "kotlin-language-server"}>
            <button class="btn-secondary" disabled={busy()} onClick={() => void run(installLanguageServer)}>
              {s()?.serverPath ? `Reinstall kotlin-lsp ${s()?.pinnedVersion ?? ""}` : `Install kotlin-lsp ${s()?.pinnedVersion ?? ""}`}
            </button>
          </Show>
          <button class="btn-secondary" disabled={busy()} onClick={() => void run(installLinters)}>
            Download ktlint and detekt
          </button>
        </div>
        <p class="muted java-hint">
          kotlin-lsp comes from download.jetbrains.com ({megabytes(s()?.downloadSize) || "about 360 MB"}, pinned version, SHA-256 checked); its repository is Apache-2.0 and the
          server build contains closed JetBrains components, so Forge never bundles it. ktlint (MIT) and detekt (Apache-2.0) come from Maven Central, pinned and SHA-256
          checked. Everything goes into Forge's data directory, never into the project.
        </p>
      </div>
      <SchemaPage pageId="lang.kotlin" />
    </div>
  );
}
