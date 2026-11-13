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
import { t } from "@/i18n";
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
            {t("langs.java.eclipseJdtLanguageServerLabel")}
            <b>{javaStatus()?.jdtls ?? t("common.notInstalled2")}</b>
          </span>
        </div>
        <div class="java-status-row">
          <Show when={javaStatus()?.javaDebug} fallback={<CircleAlert size={14} class="java-warn" />}>
            <CircleCheck size={14} class="java-ok" />
          </Show>
          <span>
            {t("langs.java.debuggerJavaDebugLabel")} <b>{javaStatus()?.javaDebug ? "installed" : t("common.notInstalled2")}</b>
          </span>
        </div>
        <div class="java-status-row">
          <span class="java-status-pad" />
          <span>
            {t("langs.java.runsOnLabel")} <b>{javaStatus()?.serverJdk ? jdkLabel(javaStatus()!.serverJdk!) : t("langs.java.noJdk21Found")}</b>
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
            {javaStatus()?.jdtls ? t("langs.java.updateJdtLsAndJava") : t("langs.java.installJdtLsAndJava")}
          </button>
        </div>
        <p class="muted java-hint">
          {t("langs.java.downloadedWithYourConsentFrom")}
        </p>
      </div>
      <SchemaPage pageId="lang.java" />
    </div>
  );
}
