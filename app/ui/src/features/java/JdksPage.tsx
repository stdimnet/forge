/**
 * Settings | Build, Execution, Deployment | JDKs: IntelliJ's Project Structure SDKs and
 * Project settings in one page. The project JDK and language level (project settings), the
 * detected JDKs (`javaforge-jdk`: JAVA_HOME, PATH, system folders, SDKMAN!, asdf, jabba,
 * Gradle, ~/.jdks, Forge's downloads) plus the ones added from disk, and Download JDK…
 * (Eclipse Temurin from Adoptium, SHA-256 checked).
 */
import { createResource, createSignal, For, onMount, Show } from "solid-js";
import { Coffee, Download, FolderOpen, Minus, Plus, RefreshCw } from "lucide-solid";
import { isTauri } from "@/ipc";
import { javaApi, type Jdk } from "@/ipc/java";
import { getSetting, saveSetting } from "@/state/settings";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { activeInstall, jdkLabel, jdks, refreshJdks, refreshStatus } from "./store";
import { t } from "@/i18n";
import "./java.css";

const [downloadOpen, setDownloadOpen] = createSignal(false);
/** Open the page with the Download JDK form (Find Action "Download JDK…"). */
export const openDownloadForm = () => setDownloadOpen(true);

const LANGUAGE_LEVELS = [
  { value: "", label: "SDK default" },
  { value: "8", label: "8 - Lambdas, type annotations etc." },
  { value: "11", label: "11 - Local variable syntax for lambda parameters" },
  { value: "17", label: "17 - Sealed types, always-strict floating-point semantics" },
  { value: "21", label: "21 - Record patterns, pattern matching for switch" },
  { value: "25", label: "25 - Compact source files, instance main methods" },
];

async function pickFolder(): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ directory: true, multiple: false });
  return typeof picked === "string" ? picked : null;
}

function DownloadForm(props: { onDone: (jdk: Jdk) => void }) {
  const [releases] = createResource(() => javaApi.releases().catch(() => null));
  const [feature, setFeature] = createSignal<number | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const versions = () => {
    const r = releases();
    if (!r) return [25, 21, 17, 11, 8];
    return [...r.available_releases].filter((v) => v >= 8).sort((a, b) => b - a);
  };
  const isLts = (v: number) => releases()?.available_lts_releases.includes(v) ?? [8, 11, 17, 21, 25].includes(v);
  const chosen = () => feature() ?? releases()?.most_recent_lts ?? 21;
  const download = async () => {
    setBusy(true);
    setError(null);
    try {
      props.onDone(await javaApi.downloadJdk(chosen()));
    } catch (e) {
      setError(String(e).replace(/^Error: /, ""));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div class="java-download" data-java-download>
      <div class="java-download-title">{t("langs.java.downloadJdk")}</div>
      <div class="java-form-row">
        <label for="java-dl-version">{t("langs.java.versionLabel")}</label>
        <select id="java-dl-version" class="java-input" value={chosen()} onChange={(e) => setFeature(Number(e.currentTarget.value))} disabled={busy()}>
          <For each={versions()}>{(v) => <option value={v}>{`${v}${isLts(v) ? " (LTS)" : ""}`}</option>}</For>
        </select>
      </div>
      <div class="java-form-row">
        <label for="java-dl-vendor">{t("langs.java.vendorLabel")}</label>
        <select id="java-dl-vendor" class="java-input" disabled={busy()}>
          <option>{t("langs.java.eclipseTemurinAdoptium")}</option>
        </select>
      </div>
      <div class="java-form-row">
        <label>{t("langs.java.locationLabel")}</label>
        <span class="muted java-mono">{t("langs.java.localShareForgeJdksJdkEllipsis", { chosen: chosen() })}</span>
      </div>
      <p class="muted java-hint">{t("langs.java.downloadedFromApiAdoptiumNet")}</p>
      <Show when={busy() && activeInstall()?.id === "jdk"}>
        <div class="java-progress">
          <span>{activeInstall()!.label}</span>
          <Show when={activeInstall()!.total}>
            <progress max={activeInstall()!.total!} value={activeInstall()!.done} />
          </Show>
        </div>
      </Show>
      <Show when={error()}>
        <p class="java-error">{error()}</p>
      </Show>
      <div class="java-buttons">
        <button class="btn-secondary" onClick={() => setDownloadOpen(false)} disabled={busy()}>
          {t("common.cancel")}
        </button>
        <button class="btn-primary" onClick={() => void download()} disabled={busy()}>
          {busy() ? t("langs.java.downloadingEllipsis") : t("langs.java.download")}
        </button>
      </div>
    </div>
  );
}

export function JdksPage() {
  const [selected, setSelected] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  onMount(() => {
    void refreshJdks().then((list) => setSelected((s) => s ?? list[0]?.home ?? null));
  });
  const current = () => jdks().find((j) => j.home === selected());
  const added = () => getSetting<string[]>("java.jdks", []);
  const projectJdk = () => getSetting<string>("java.projectJdk", "");
  const level = () => getSetting<string>("java.languageLevel", "");
  const addFromDisk = async () => {
    const folder = await pickFolder();
    if (!folder) return;
    const jdk = await javaApi.inspectJdk(folder);
    if (!jdk) {
      setError(`${folder} is not a JDK home (no bin/java and bin/javac).`);
      return;
    }
    setError(null);
    await saveSetting("java.jdks", [...added().filter((h) => h !== jdk.home), jdk.home]);
    await refreshJdks();
    setSelected(jdk.home);
  };
  const remove = async () => {
    const j = current();
    if (!j || !added().includes(j.home)) return;
    await saveSetting("java.jdks", added().filter((h) => h !== j.home));
    await refreshJdks();
    setSelected(jdks()[0]?.home ?? null);
  };
  return (
    <div class="java-jdks">
      <div class="java-form-row">
        <label for="java-project-jdk">{t("langs.java.projectJdkLabel")}</label>
        <select
          id="java-project-jdk"
          class="java-input"
          value={projectJdk()}
          onChange={(e) => void saveSetting("java.projectJdk", e.currentTarget.value).then(refreshStatus)}
        >
          <option value="">{t("langs.java.newest", { value: jdks()[0] ? jdkLabel(jdks()[0]) : "no JDK found" })}</option>
          <For each={jdks()}>{(j) => <option value={j.home}>{jdkLabel(j)}</option>}</For>
        </select>
      </div>
      <div class="java-form-row">
        <label for="java-language-level">{t("langs.java.languageLevelLabel")}</label>
        <select id="java-language-level" class="java-input" value={level()} onChange={(e) => void saveSetting("java.languageLevel", e.currentTarget.value)}>
          <For each={LANGUAGE_LEVELS}>{(l) => <option value={l.value}>{l.label}</option>}</For>
        </select>
      </div>
      <div class="java-sdks">
        <div class="java-sdk-list">
          <div class="java-toolbar">
            <button class="icon-btn" title={t("langs.java.addJdkFromDiskEllipsis")} onClick={() => void addFromDisk()}>
              <Plus size={14} />
            </button>
            <button class="icon-btn" title={t("common.remove")} disabled={!current() || !added().includes(current()!.home)} onClick={() => void remove()}>
              <Minus size={14} />
            </button>
            <button class="icon-btn" title={t("langs.java.downloadJdkEllipsis")} onClick={() => setDownloadOpen(true)}>
              <Download size={14} />
            </button>
            <button class="icon-btn" title={t("langs.java.detectAgain")} onClick={() => void refreshJdks(true)}>
              <RefreshCw size={13} />
            </button>
          </div>
          <div class="java-sdk-items">
            <For each={jdks()} fallback={<div class="muted java-pad">{t("langs.java.noJdksFoundDownloadOne")}</div>}>
              {(j) => (
                <div class="tree-row java-sdk-row" classList={{ selected: j.home === selected() }} onClick={() => setSelected(j.home)} data-jdk={j.name}>
                  <Coffee size={13} class="java-jdk-icon" />
                  <span>{j.name}</span>
                  <span class="muted java-sdk-version">{j.version}</span>
                </div>
              )}
            </For>
          </div>
        </div>
        <div class="java-sdk-details">
          <Show when={downloadOpen()}>
            <DownloadForm
              onDone={(jdk) => {
                setDownloadOpen(false);
                void refreshJdks(true).then(() => setSelected(jdk.home));
              }}
            />
          </Show>
          <Show when={!downloadOpen() && current()}>
            {(j) => (
              <>
                <div class="java-form-row">
                  <label>{t("common.nameLabel")}</label>
                  <span class="java-value">{j().name}</span>
                </div>
                <div class="java-form-row">
                  <label>{t("langs.java.jdkHomePathLabel")}</label>
                  <span class="java-value java-mono">{j().home}</span>
                  <FolderOpen size={13} class="muted" />
                </div>
                <div class="java-form-row">
                  <label>{t("langs.java.versionLabel")}</label>
                  <span class="java-value">{j().version || "unknown"}</span>
                </div>
                <div class="java-form-row">
                  <label>{t("langs.java.vendorLabel")}</label>
                  <span class="java-value">{j().vendor}</span>
                </div>
                <div class="java-form-row">
                  <label>{t("langs.java.foundInLabel")}</label>
                  <span class="java-value">{j().source}</span>
                </div>
                <Show when={j().major > 0 && j().major < 21}>
                  <p class="muted java-hint">{t("langs.java.eclipseJdtLsRunsOn")}</p>
                </Show>
              </>
            )}
          </Show>
          <Show when={error()}>
            <p class="java-error">{error()}</p>
          </Show>
        </div>
      </div>
      <SchemaPage pageId="build.jdks" />
    </div>
  );
}
