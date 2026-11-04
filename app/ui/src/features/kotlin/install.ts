/**
 * Downloads with the user's consent: kotlin-lsp (the offer when a Kotlin project opens
 * without a language server, `kotlin.lsp.autoInstall`: ask | always | never) and the ktlint
 * and detekt jars (Maven Central). Pinned versions, SHA-256 checked, into Forge's data
 * directory; nothing is bundled.
 */
import { kotlinApi } from "@/ipc/kotlin";
import { balloonsFor } from "@/keymap/dispatch";
import { getSetting, saveSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { openSettings } from "@/features/settings/dialogState";
import { active, clearInstall, listenForInstalls, megabytes, refreshStatus } from "./store";
import { t } from "@/i18n";

const showBalloon = balloonsFor("Kotlin");
const AUTO_INSTALL_KEY = "kotlin.lsp.autoInstall";

let installing = false;

/** Download the pinned kotlin-lsp and start it for the project. */
export async function installLanguageServer(): Promise<void> {
  if (installing) return;
  installing = true;
  listenForInstalls();
  try {
    const version = await kotlinApi.installLanguageServer();
    const status = await refreshStatus();
    if (status?.problems.length) showBalloon(t("langs.kotlin.kotlinLspInstalled", { version }), status.problems.join(" "), 0, [{ label: "Settings…", run: () => openSettings("lang.kotlin") }]);
    else showBalloon(t("langs.kotlin.kotlinLspInstalled", { version }), t("langs.kotlin.theKotlinLanguageServerIs"), 6000);
  } catch (e) {
    showBalloon(t("langs.kotlin.kotlinLspCouldNotBe"), String(e).replace(/^Error: /, ""), 0, [{ label: "Retry", run: () => void installLanguageServer() }]);
  } finally {
    installing = false;
    clearInstall();
  }
}

let installingLinters = false;

/** Download the pinned ktlint and detekt jars. */
export async function installLinters(): Promise<void> {
  if (installingLinters) return;
  installingLinters = true;
  listenForInstalls();
  try {
    await kotlinApi.installLinters();
    await refreshStatus();
    showBalloon(t("langs.kotlin.ktlintAndDetektInstalled"), t("langs.kotlin.kotlinFilesAreCheckedWith"), 5000);
  } catch (e) {
    showBalloon(t("langs.kotlin.ktlintAndDetektCouldNot"), String(e).replace(/^Error: /, ""), 0, [{ label: "Retry", run: () => void installLinters() }]);
  } finally {
    installingLinters = false;
    clearInstall();
  }
}

let offered = false;

/** After a Kotlin project opened: offer kotlin-lsp once per session. */
export async function checkKotlinSetup(): Promise<void> {
  if (!active() || offered) return;
  const status = await refreshStatus();
  if (!status || status.remote) return;
  offered = true;
  if (status.server !== "kotlin-lsp" || status.serverPath) return;
  const mode = getSetting<string>(AUTO_INSTALL_KEY, "ask");
  if (mode === "never") return;
  if (mode === "always") return void installLanguageServer();
  showBalloon(
    t("langs.kotlin.kotlinLanguageServerKotlinLsp"),
    t("langs.kotlin.completionNavigationInspectionsRefactori", { value: workspace()?.name ?? "this project", pinnedVersion: status.pinnedVersion, value2: status.downloadSize ? ` (${megabytes(status.downloadSize)})` : "" }),
    0,
    [
      { label: "Install", run: () => void installLanguageServer() },
      { label: "Not now", run: () => {} },
      { label: "Don't ask again", run: () => void saveSetting(AUTO_INSTALL_KEY, "never") },
    ],
  );
}
