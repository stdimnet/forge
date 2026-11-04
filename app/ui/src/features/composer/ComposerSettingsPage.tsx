/**
 * Settings | Languages & Frameworks | PHP | Composer: the schema settings plus "Download
 * composer.phar from getcomposer.org". Forge downloads only after the user confirmed it,
 * checks the published SHA-256 sum (in Rust) and stores the file in its data directory;
 * the path is put into "Path to composer" (Apply saves it).
 */
import { createSignal, Match, Switch } from "solid-js";
import { Download } from "lucide-solid";
import { composerApi } from "@/ipc/composer";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { setDraft } from "@/features/settings/dialogState";
import { t } from "@/i18n";

type Step = { kind: "idle" } | { kind: "confirm" } | { kind: "busy" } | { kind: "done"; path: string } | { kind: "error"; message: string };

function DownloadPhar() {
  const [step, setStep] = createSignal<Step>({ kind: "idle" });
  const download = async () => {
    setStep({ kind: "busy" });
    try {
      const path = await composerApi.downloadPhar();
      setDraft("php.composer.executable", path);
      setStep({ kind: "done", path });
    } catch (e) {
      setStep({ kind: "error", message: String(e instanceof Error ? e.message : e) });
    }
  };
  return (
    <div class="composer-phar">
      <Switch>
        <Match when={step().kind === "confirm"}>
          <div class="composer-phar-consent">
            {t("langs.composer.forgeDownloadsTheLatestStable")}
            <div class="composer-phar-buttons">
              <button class="btn-primary" onClick={() => void download()}>
                {t("langs.composer.download")}
              </button>
              <button class="btn-secondary" onClick={() => setStep({ kind: "idle" })}>
                {t("common.cancel")}
              </button>
            </div>
          </div>
        </Match>
        <Match when={step().kind === "busy"}>
          <span class="set-desc">{t("langs.composer.downloadingComposerPharEllipsis")}</span>
        </Match>
        <Match when={step().kind !== "confirm" && step().kind !== "busy"}>
          <button class="link-btn composer-phar-link" onClick={() => setStep({ kind: "confirm" })}>
            <Download size={13} />
            {t("langs.composer.downloadComposerPharFromGetcomposer")}
          </button>
          <Switch>
            <Match when={step().kind === "done"}>
              <span class="set-desc composer-phar-result">{t("langs.composer.savedAsClickApplyTo", { path: (step() as { path: string }).path })}</span>
            </Match>
            <Match when={step().kind === "error"}>
              <span class="set-desc composer-phar-error">{(step() as { message: string }).message}</span>
            </Match>
          </Switch>
        </Match>
      </Switch>
    </div>
  );
}

export function ComposerSettingsPage() {
  return <SchemaPage pageId="lang.php.composer" sectionHeader={(s) => (s === "Execution" ? <DownloadPhar /> : null)} />;
}
