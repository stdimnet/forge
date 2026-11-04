/**
 * Scala in the UI (JavaForge), like IntelliJ IDEA's Scala plugin. Everything is computed in
 * Rust (`commands/scala.rs`, `packs/javaforge/lang-scala`); code intelligence comes from
 * Metals through the shared LSP client. This module plugs Scala into the contribution
 * points:
 *
 * - Settings | Languages & Frameworks | Scala (Metals, Coursier, sbt);
 * - the sbt tool window (`SbtToolWindow.tsx`), Reload sbt Project, the sbt shell;
 * - the sbt Task, ScalaTest and MUnit run configuration types (`runTypes.tsx`);
 * - the Run gutter on `main` methods, `App` objects and `@main` methods (they run with
 *   `sbt runMain`), Run Context Configuration (Ctrl+Shift+F10);
 * - the offer to install Metals with Coursier when a Scala project opens without it.
 */
import { createSignal, onMount, Show } from "solid-js";
import { CircleAlert, CircleCheck, LoaderCircle, Play, Settings2, Workflow } from "lucide-solid";
import { isDialogWindow } from "@/shell/window/mode";
import { scalaApi, type MainObject } from "@/ipc/scala";
import { registerLineMarkerProvider, type LineMarker } from "@/editor/lineMarkers";
import { shortcutLabel } from "@/keymap";
import { balloonsFor } from "@/keymap/dispatch";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerToolWindow } from "@/features/registry";
import { registerStatusItem } from "@/features/statusItems";
import { useTemporary } from "@/features/run/configStore";
import { launch } from "@/features/run/runStore";
import { openMenu } from "@/features/run/PopupMenu";
import { openRunConfigurations } from "@/features/run/dialogState";
import { openSettings } from "@/features/settings/dialogState";
import { commands, registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { onProjectDetected } from "@/state/project";
import { getSetting, saveSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { SbtToolWindow, openSbtShell } from "./SbtToolWindow";
import { SBT_TASK } from "./runTypes";
import { active, importBuild, metalsInstalling, projectOf, refreshBuild, refreshStatus, sbtBuild, scalaStatus, setMetalsInstalling } from "./store";
import "./scala.css";
import { t } from "@/i18n";

const showBalloon = balloonsFor("Scala");

// ---- Metals ---------------------------------------------------------------------------------

let installing = false;

/** Install the pinned Metals with Coursier and start it for the project. */
export async function installMetals(): Promise<void> {
  if (installing) return;
  installing = true;
  setMetalsInstalling("Installing Metals with Coursier…");
  const off = await scalaApi.onInstall((l) => setMetalsInstalling(l.line));
  try {
    const version = await scalaApi.installMetals();
    await refreshStatus();
    showBalloon(t("langs.scala.metalsInstalled", { version }), t("langs.scala.metalsIsImportingTheSbt"), 6000);
  } catch (e) {
    showBalloon(t("langs.scala.metalsCouldNotBeInstalled"), String(e).replace(/^Error: /, ""), 0, [{ label: t("common.retry"), run: () => void installMetals() }]);
  } finally {
    off();
    installing = false;
    setMetalsInstalling(null);
  }
}

let offered = false;

async function checkScalaSetup(): Promise<void> {
  if (!active() || offered) return;
  void refreshBuild();
  if (getSetting<boolean>("scala.sbt.importOnOpen", false)) void importBuild();
  const s = await refreshStatus();
  if (!s || s.remote || s.metals) return;
  offered = true;
  if (getSetting<string>("scala.metals.autoInstall", "ask") === "never") return;
  showBalloon(
    t("langs.scala.scalaLanguageServerMetalsIs"),
    s.coursier
      ? t("langs.scala.completionNavigationAndDiagnosticsIn", { value: workspace()?.name ?? "this project", pinnedVersion: s.pinnedVersion, coursier: s.coursier })
      : t("langs.scala.completionNavigationAndDiagnosticsNeed"),
    0,
    [
      ...(s.coursier ? [{ label: t("common.install"), run: () => void installMetals() }] : []),
      { label: t("langs.scala.settingsEllipsis"), run: () => openSettings("lang.scala") },
      { label: t("langs.scala.donTAskAgain"), run: () => void saveSetting("scala.metals.autoInstall", "never") },
    ],
  );
}

// ---- Settings -------------------------------------------------------------------------------

function Row(props: { ok: boolean; label: string; value: string; path?: string | null }) {
  return (
    <div class="java-status-row">
      <Show when={props.ok} fallback={<CircleAlert size={14} class="java-warn" />}>
        <CircleCheck size={14} class="java-ok" />
      </Show>
      <span>
        {props.label}: <b>{props.value}</b>
        <Show when={props.path}>
          <span class="scala-path"> {props.path}</span>
        </Show>
      </span>
    </div>
  );
}

function ScalaPage() {
  const [busy, setBusy] = createSignal(false);
  onMount(() => void refreshStatus());
  const s = scalaStatus;
  return (
    <div class="java-page" data-scala-page>
      <div class="java-status" data-scala-status>
        <Row ok={!!s()?.metals} label={t("langs.scala.metals")} value={s()?.metals ? (s()?.metalsVersion ?? "installed") : "not installed"} path={s()?.metals} />
        <Row ok={!!s()?.coursier} label={t("langs.scala.coursier")} value={s()?.coursier ? "found" : "not found (get-coursier.io)"} path={s()?.coursier} />
        <Row ok={!!s()?.sbt} label="sbt" value={s()?.sbt ? "found" : "not found (sdk install sbt, scala-sbt.org)"} path={s()?.sbt} />
        <Show when={s()?.problems.length}>
          <p class="java-error">{s()!.problems.join(" ")}</p>
        </Show>
        <div class="java-buttons java-buttons-left">
          <button class="btn-secondary" disabled={busy() || !!metalsInstalling() || !s()?.coursier} onClick={async () => (setBusy(true), await installMetals(), setBusy(false))}>
            {s()?.metals ? t("langs.scala.reinstallMetals", { value: s()?.pinnedVersion ?? "" }) : t("langs.scala.installMetals", { value: s()?.pinnedVersion ?? "" })}
          </button>
        </div>
        <p class="muted java-hint">
          {t("langs.scala.metalsApache20Scalameta")}
        </p>
      </div>
      <SchemaPage pageId="lang.scala" />
    </div>
  );
}

registerSettingsPage({
  id: "lang.scala",
  title: t("settings.page.lang.scala"),
  group: "Languages & Frameworks",
  order: 28,
  keywords: ["scala", "sbt", "metals", "coursier", "scalatest", "munit", "bloop"],
  component: ScalaPage,
});

// ---- Commands and the sbt tool window -----------------------------------------------------------

registerCommand({ id: "scala.reloadSbt", title: "Reload sbt Project", category: "Scala", run: () => void importBuild() });
registerCommand({ id: "scala.sbtShell", title: "Open sbt Shell", category: "Scala", run: openSbtShell });
registerCommand({ id: "scala.installMetals", title: "Install Metals (Scala Language Server)", category: "Scala", run: () => void installMetals() });

registerToolWindow({
  id: "sbt",
  title: t("toolwindow.sbt.title"),
  icon: Workflow,
  anchor: "right",
  order: 41,
  component: SbtToolWindow,
  when: () => active() && !!sbtBuild(),
});

registerStatusItem({
  id: "scala.install",
  align: "right",
  order: 6,
  when: () => !!metalsInstalling(),
  component: () => (
    <span class="status-item ls-install-status" title={metalsInstalling() ?? ""}>
      <LoaderCircle size={13} class="lsp-spin" />
      <span class="ls-install-status-name">{t("langs.scala.installingMetalsLabel")}</span>
      <span class="ls-install-status-text">{metalsInstalling()}</span>
    </span>
  ),
});

if (typeof document !== "undefined" && !isDialogWindow()) {
  onProjectDetected(() => void checkScalaSetup());
}

// ---- Run gutter on main objects -------------------------------------------------------------

const isScala = (path: string) => path.endsWith(".scala");

function mainConfiguration(path: string, m: MainObject) {
  const project = projectOf(path);
  return useTemporary({
    name: m.name,
    type: SBT_TASK,
    options: { tasks: `runMain ${m.class}`, project },
  });
}

function runMenu(label: string, cfg: () => ReturnType<typeof useTemporary>): LineMarker["onClick"] {
  return (_view, e) =>
    openMenu({
      x: e.clientX,
      y: e.clientY + 6,
      items: [
        { label: t("common.run3", { label }), icon: Play, iconColor: "var(--success)", shortcut: shortcutLabel("RunClass"), run: () => void launch(cfg()) },
        { separator: true },
        { label: t("common.modifyRunConfigurationEllipsis"), icon: Settings2, run: () => openRunConfigurations(cfg().name) },
      ],
    });
}

const mainsOf = (text: string) => scalaApi.mainObjects(text).catch(() => [] as MainObject[]);

registerLineMarkerProvider({
  appliesTo: isScala,
  watch: active,
  markers: async (path, text) => {
    if (!active()) return [];
    const mains = await mainsOf(text);
    return mains.map((m) => ({ line: m.line, icon: Play, title: t("common.run", { name: m.name }), className: "marker-run", onClick: runMenu(m.name, () => mainConfiguration(path, m)) }));
  },
});

const runClass = commands().get("RunClass");
if (runClass) {
  registerCommand({
    ...runClass,
    run: async () => {
      const tab = activeTab();
      if (tab && active() && isScala(tab.path)) {
        const [main] = await mainsOf(tab.content);
        if (main) return void launch(mainConfiguration(tab.path, main));
      }
      return runClass.run();
    },
  });
}
