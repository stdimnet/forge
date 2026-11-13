/**
 * Settings | Build, Execution, Deployment | Build Tools | Gradle: the project's Gradle
 * wrapper (version, distribution, `distributionSha256Sum` and its check against the sum
 * Gradle publishes, Update Wrapper) above the schema settings (Use Gradle from, Gradle JVM,
 * offline work, Build and run using, Run tests using).
 */
import { createResource, createSignal, Show } from "solid-js";
import { CircleCheck, LoaderCircle, ShieldAlert, ShieldCheck, TriangleAlert } from "lucide-solid";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { javaApi, type WrapperChecksum } from "@/ipc/java";
import { refreshBuild, refreshModel } from "./store";
import { t } from "@/i18n";

const CHECKS: Record<WrapperChecksum, { text: string; ok: boolean }> = {
  matches: { text: "matches the checksum Gradle publishes", ok: true },
  mismatch: { text: "does not match the checksum Gradle publishes: the wrapper will refuse the download", ok: false },
  notSet: { text: "not set: the wrapper does not verify the download", ok: false },
  unknown: { text: "set (not an official distribution, nothing to compare with)", ok: true },
};

export function GradlePage() {
  const [info, { refetch }] = createResource(() => javaApi.wrapperInfo().catch(() => null));
  const [version, setVersion] = createSignal("");
  const [busy, setBusy] = createSignal<"verify" | "update" | null>(null);
  const [check, setCheck] = createSignal<WrapperChecksum | null>(null);
  const [message, setMessage] = createSignal<{ text: string; error: boolean } | null>(null);
  const verify = async () => {
    setBusy("verify");
    setMessage(null);
    try {
      setCheck(await javaApi.wrapperVerify());
    } catch (e) {
      setMessage({ text: String(e).replace(/^Error: /, ""), error: true });
    } finally {
      setBusy(null);
    }
  };
  const update = async () => {
    setBusy("update");
    setMessage(null);
    try {
      const w = await javaApi.wrapperUpdate(version().trim() || undefined);
      setMessage({ text: `The wrapper now uses Gradle ${w.version} (checksum ${w.sha256?.slice(0, 12)}…). Reload the project to use it.`, error: false });
      setCheck("matches");
      void refetch();
      void refreshBuild();
      void refreshModel();
    } catch (e) {
      setMessage({ text: String(e).replace(/^Error: /, ""), error: true });
    } finally {
      setBusy(null);
    }
  };
  return (
    <div class="java-page" data-testid="gradle-settings-page">
      <Show when={info()}>
        {(i) => (
          <div class="java-status java-wrapper" data-testid="gradle-wrapper">
            <div class="java-status-row">
              <b>{t("langs.java.gradleWrapper")}</b>
              <span class="java-value">Gradle {i().wrapper.version || "?"}</span>
              <span class="muted">({i().wrapper.distributionType})</span>
            </div>
            <div class="java-status-row">
              <span class="muted java-mono" title={i().wrapper.distributionUrl}>
                {i().wrapper.distributionUrl}
              </span>
            </div>
            <div class="java-status-row">
              <Show when={i().wrapper.sha256} fallback={<TriangleAlert size={14} class="java-warn" />}>
                <Show when={check() !== "mismatch"} fallback={<ShieldAlert size={14} class="java-error-icon" />}>
                  <ShieldCheck size={14} class={check() === "matches" ? "java-ok" : "muted"} />
                </Show>
              </Show>
              <span>
                {t("langs.java.distributionsha256sum")}
                {check() ? CHECKS[check()!].text : i().wrapper.sha256 ? <span class="java-mono muted">{i().wrapper.sha256!.slice(0, 16)}…</span> : CHECKS.notSet.text}
              </span>
              <button class="link-btn" disabled={!!busy() || !i().wrapper.sha256} onClick={() => void verify()}>
                {t("langs.java.verify")}
              </button>
            </div>
            <div class="java-form-row">
              <label for="gradle-wrapper-version">{t("langs.java.updateWrapperTo")}</label>
              <input
                id="gradle-wrapper-version"
                class="java-input"
                placeholder={t("langs.java.newestRelease")}
                value={version()}
                spellcheck={false}
                onInput={(e) => setVersion(e.currentTarget.value)}
              />
              <button class="btn-secondary" disabled={!!busy()} data-testid="gradle-update-wrapper" onClick={() => void update()}>
                <Show when={busy() === "update"} fallback="Update Wrapper">
                  <LoaderCircle size={13} class="lsp-spin" /> {t("langs.java.updatingEllipsis")}
                </Show>
              </button>
            </div>
            <Show when={message()}>
              {(m) => (
                <p classList={{ "java-error": m().error, "java-hint": !m().error }}>
                  <Show when={!m().error}>
                    <CircleCheck size={13} class="java-ok" />{" "}
                  </Show>
                  {m().text}
                </p>
              )}
            </Show>
          </div>
        )}
      </Show>
      <SchemaPage pageId="build.tools.gradle" />
    </div>
  );
}
