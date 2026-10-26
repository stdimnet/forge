/**
 * Settings | Languages & Frameworks | Java: the state of Eclipse JDT LS and java-debug
 * (installed version, the JDK it runs on, what is missing) with Install, and the
 * `java.jdtls.*` / `java.debug.*` options.
 */
import { createSignal, onMount, Show } from "solid-js";
import { CircleAlert, CircleCheck } from "lucide-solid";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { activeInstall, javaStatus, jdkLabel, refreshStatus } from "./store";
import { installLanguageServer } from "./install";
import "./java.css";

export function JavaPage() {
  const [busy, setBusy] = createSignal(false);
  onMount(() => void refreshStatus());
  const install = async () => {
    setBusy(true);
    try {
      await installLanguageServer();
    } finally {
      setBusy(false);
    }
  };
  return (
    <div class="java-page">
      <div class="java-status" data-java-status>
        <div class="java-status-row">
          <Show when={javaStatus()?.jdtls} fallback={<CircleAlert size={14} class="java-warn" />}>
            <CircleCheck size={14} class="java-ok" />
          </Show>
          <span>
            Eclipse JDT Language Server:{" "}
            <b>{javaStatus()?.jdtls ?? "not installed"}</b>
          </span>
        </div>
        <div class="java-status-row">
          <Show when={javaStatus()?.javaDebug} fallback={<CircleAlert size={14} class="java-warn" />}>
            <CircleCheck size={14} class="java-ok" />
          </Show>
          <span>
            Debugger (java-debug): <b>{javaStatus()?.javaDebug ? "installed" : "not installed"}</b>
          </span>
        </div>
        <div class="java-status-row">
          <span class="java-status-pad" />
          <span>
            Runs on: <b>{javaStatus()?.serverJdk ? jdkLabel(javaStatus()!.serverJdk!) : "no JDK 21+ found"}</b>
          </span>
        </div>
        <Show when={javaStatus()?.problems.length}>
          <p class="java-error">{javaStatus()!.problems.join(" ")}</p>
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
          <button class="btn-secondary" disabled={busy()} onClick={() => void install()}>
            {javaStatus()?.jdtls ? "Update JDT LS and java-debug" : "Install JDT LS and java-debug"}
          </button>
        </div>
        <p class="muted java-hint">
          Downloaded with your consent from download.eclipse.org (EPL-2.0, SHA-256 checked) and Maven Central (java-debug, EPL-1.0, checksum checked) into
          Forge's data directory. Workspace data lives in the user cache directory, never in the project.
        </p>
      </div>
      <SchemaPage pageId="lang.java" />
    </div>
  );
}
