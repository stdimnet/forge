/**
 * Installing Eclipse JDT LS and java-debug with the user's consent: the offer when a Java
 * project opens without them (`java.jdtls.autoInstall`: ask | always | never), Install from
 * the Java settings page or Find Action, and the hint when no JDK 21+ can run JDT LS.
 */
import { javaApi } from "@/ipc/java";
import { balloonsFor } from "@/keymap/dispatch";
import { getSetting, saveSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { openSettings } from "@/features/settings/dialogState";
import { active, clearInstall, listenForInstalls, refreshJdks, refreshStatus } from "./store";
import { openDownloadForm } from "./JdksPage";
import { t } from "@/i18n";

const showBalloon = balloonsFor("Java");
const AUTO_INSTALL_KEY = "java.jdtls.autoInstall";

let installing = false;

/** Install (or update) JDT LS and java-debug, then start JDT LS for the project. */
export async function installLanguageServer(): Promise<void> {
  if (installing) return;
  installing = true;
  listenForInstalls();
  try {
    const version = await javaApi.installLanguageServer();
    const status = await refreshStatus();
    if (status?.problems.length) showBalloon(t("langs.java.eclipseJdtLsInstalled", { version }), status.problems.join(" "), 0, jdkActions());
    else showBalloon(t("langs.java.eclipseJdtLsInstalled", { version }), t("langs.java.theJavaLanguageServerIs"), 6000);
  } catch (e) {
    showBalloon(t("langs.java.eclipseJdtLsCouldNot"), String(e).replace(/^Error: /, ""), 0, [{ label: "Retry", run: () => void installLanguageServer() }]);
  } finally {
    installing = false;
    clearInstall();
  }
}

const jdkActions = () => [
  {
    label: "Download JDK…",
    run: () => {
      openDownloadForm();
      openSettings("build.jdks");
    },
  },
  { label: "Settings…", run: () => openSettings("lang.java") },
];

let offered = false;

/** After a Java project opened: offer JDT LS (once per session) or point at a missing JDK. */
export async function checkJavaSetup(): Promise<void> {
  if (!active() || offered) return;
  await refreshJdks();
  const status = await refreshStatus();
  if (!status || status.remote) return;
  offered = true;
  if (!status.jdtls) {
    const mode = getSetting<string>(AUTO_INSTALL_KEY, "ask");
    if (mode === "never") return;
    if (mode === "always") return void installLanguageServer();
    showBalloon(
      t("langs.java.javaLanguageServerEclipseJdt"),
      t("langs.java.completionNavigationInspectionsAndRefact", { value: workspace()?.name ?? "this project" }),
      0,
      [
        { label: "Install", run: () => void installLanguageServer() },
        { label: "Not now", run: () => {} },
        { label: "Don't ask again", run: () => void saveSetting(AUTO_INSTALL_KEY, "never") },
      ],
    );
    return;
  }
  if (status.problems.length) showBalloon(t("langs.java.eclipseJdtLsCannotStart"), status.problems.join(" "), 0, jdkActions());
}
