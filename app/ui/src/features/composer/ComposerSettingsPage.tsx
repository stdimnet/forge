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
            Forge downloads the latest stable composer.phar from https://getcomposer.org, checks its SHA-256 sum and saves it in Forge's data
            directory. It runs with the PHP interpreter of Settings | PHP.
            <div class="composer-phar-buttons">
              <button class="btn-primary" onClick={() => void download()}>
                Download
              </button>
              <button class="btn-secondary" onClick={() => setStep({ kind: "idle" })}>
                Cancel
              </button>
            </div>
          </div>
        </Match>
        <Match when={step().kind === "busy"}>
          <span class="set-desc">Downloading composer.phar…</span>
        </Match>
        <Match when={step().kind !== "confirm" && step().kind !== "busy"}>
          <button class="link-btn composer-phar-link" onClick={() => setStep({ kind: "confirm" })}>
            <Download size={13} />
            Download composer.phar from getcomposer.org
          </button>
          <Switch>
            <Match when={step().kind === "done"}>
              <span class="set-desc composer-phar-result">Saved as {(step() as { path: string }).path}; click Apply to use it.</span>
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
