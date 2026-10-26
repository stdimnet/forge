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
    if (status?.problems.length) showBalloon(`kotlin-lsp ${version} installed`, status.problems.join(" "), 0, [{ label: "Settings…", run: () => openSettings("lang.kotlin") }]);
    else showBalloon(`kotlin-lsp ${version} installed`, "The Kotlin language server is importing the Gradle build.", 6000);
  } catch (e) {
    showBalloon("kotlin-lsp could not be installed", String(e).replace(/^Error: /, ""), 0, [{ label: "Retry", run: () => void installLanguageServer() }]);
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
    showBalloon("ktlint and detekt installed", "Kotlin files are checked with ktlint; detekt runs after saving.", 5000);
  } catch (e) {
    showBalloon("ktlint and detekt could not be installed", String(e).replace(/^Error: /, ""), 0, [{ label: "Retry", run: () => void installLinters() }]);
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
    "Kotlin language server kotlin-lsp is not installed",
    `Completion, navigation, inspections, refactorings and the Kotlin debugger in ${workspace()?.name ?? "this project"} need it. Forge downloads JetBrains' standalone kotlin-lsp ${status.pinnedVersion}${status.downloadSize ? ` (${megabytes(status.downloadSize)})` : ""} from download.jetbrains.com and checks its SHA-256.`,
    0,
    [
      { label: "Install", run: () => void installLanguageServer() },
      { label: "Not now", run: () => {} },
      { label: "Don't ask again", run: () => void saveSetting(AUTO_INSTALL_KEY, "never") },
    ],
  );
}
