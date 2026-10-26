/**
 * KotlinForge in the UI, in the style of IntelliJ IDEA. Everything is computed in Rust
 * (`commands/kotlin.rs`, `packs/kotlinforge/lang-kotlin`); code intelligence comes from
 * kotlin-lsp (or kotlin-language-server) through the shared LSP client (completion,
 * navigation, diagnostics, rename, code actions, formatting). JDKs, Gradle and Maven come
 * from JavaForge. This module plugs KotlinForge into the contribution points:
 *
 * - Settings | Languages & Frameworks | Kotlin (`KotlinPage.tsx`);
 * - run configuration types Kotlin Script and Kotest (`runTypes.tsx`), the Kotlin debugger
 *   with its Coroutines tab (`debug.ts`, `CoroutinesView.tsx`);
 * - gutter ▶ on `main` functions (Kotlin applications run through the build or kotlinc)
 *   and on Kotlin scripts, Run Context Configuration (Ctrl+Shift+F10);
 * - ktlint and detekt in the editor and the Problems view, ktlint format (`lint.ts`);
 * - the kotlin-lsp download offer and its progress in the status bar.
 */
import { Show } from "solid-js";
import { Bug, LoaderCircle, Play, Settings2 } from "lucide-solid";
import { isDialogWindow } from "@/shell/window/mode";
import { kotlinApi, type MainFunction } from "@/ipc/kotlin";
import { registerLineMarkerProvider, type LineMarker } from "@/editor/lineMarkers";
import { shortcutLabel } from "@/keymap";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerStatusItem } from "@/features/statusItems";
import { action, registerMainMenuEntries } from "@/features/mainMenu/model";
import { useTemporary } from "@/features/run/configStore";
import { debugConfiguration, launch } from "@/features/run/runStore";
import { openMenu } from "@/features/run/PopupMenu";
import { openRunConfigurations } from "@/features/run/dialogState";
import { commands, registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { onProjectDetected } from "@/state/project";
import { KotlinPage } from "./KotlinPage";
import { checkKotlinSetup, installLanguageServer, installLinters } from "./install";
import { detektFiles, isKotlinSource, ktlintFile, ktlintFormatFile } from "./lint";
import { active, activeInstall, moduleOf } from "./store";
import { SCRIPT } from "./runTypes";
import "./debug";
import "./kotlin.css";

// ---- Settings ---------------------------------------------------------------------------

registerSettingsPage({
  id: "lang.kotlin",
  title: "Kotlin",
  group: "Languages & Frameworks",
  order: 26,
  keywords: ["kotlin", "kotlin-lsp", "language server", "kotlinc", "ktlint", "detekt", "kotest", "coroutines", "scripts", "kts"],
  component: KotlinPage,
});

// ---- Commands ---------------------------------------------------------------------------

registerCommand({ id: "kotlin.installLanguageServer", title: "Install Kotlin Language Server (kotlin-lsp)", category: "Kotlin", run: () => void installLanguageServer() });
registerCommand({ id: "kotlin.installLinters", title: "Download ktlint and detekt", category: "Kotlin", run: () => void installLinters() });
registerCommand({ id: "kotlin.inspectCode", title: "Run detekt on the Project", category: "Kotlin", run: () => void detektFiles(undefined, true) });
registerCommand({
  id: "kotlin.ktlintFormat",
  title: "Format File with ktlint",
  category: "Kotlin",
  run: () => {
    const tab = activeTab();
    if (tab && isKotlinSource(tab.path)) void ktlintFormatFile(tab.path);
  },
});

registerMainMenuEntries("_Code", [action("kotlin.ktlintFormat", "Format File with _ktlint"), action("kotlin.inspectCode", "Run _detekt on the Project")], { when: active });

// ---- Status bar: kotlin-lsp / ktlint / detekt downloads -----------------------------------

registerStatusItem({
  id: "kotlin.install",
  align: "right",
  order: 6,
  when: () => !!activeInstall(),
  component: () => (
    <Show when={activeInstall()}>
      {(i) => (
        <span class="status-item ls-install-status" title={i().label}>
          <LoaderCircle size={13} class="lsp-spin" />
          <span class="ls-install-status-name">{i().id === "kotlin-lsp" ? "Installing kotlin-lsp:" : `Downloading ${i().id}:`}</span>
          <span class="ls-install-status-text">{i().total ? `${i().label} · ${Math.floor((i().done / i().total!) * 100)}%` : i().label}</span>
          <span class="ls-install-bar" classList={{ indeterminate: !i().total }}>
            <span style={{ width: `${Math.round((i().total ? i().done / i().total! : 0.35) * 100)}%` }} />
          </span>
        </span>
      )}
    </Show>
  ),
});

// ---- Project open -----------------------------------------------------------------------

if (typeof document !== "undefined" && !isDialogWindow()) {
  onProjectDetected(() => {
    if (!active()) return;
    void checkKotlinSetup();
  });
}

// ---- Run gutter on main functions and scripts ----------------------------------------------

const fileName = (path: string) => path.split("/").pop() ?? path;

/** The temporary Application configuration of a Kotlin `main`. */
function mainConfiguration(path: string, m: MainFunction) {
  return useTemporary({
    name: m.name,
    type: "java-application",
    options: { mainClass: m.class, kotlin: "true", file: path, module: moduleOf(path) },
  });
}

function scriptConfiguration(path: string) {
  return useTemporary({ name: fileName(path), type: SCRIPT, options: { script: path } });
}

const isScript = (path: string) => path.endsWith(".kts") && !path.endsWith(".gradle.kts");

function runMenu(label: string, cfg: () => ReturnType<typeof useTemporary>, debuggable: boolean): LineMarker["onClick"] {
  return (_view, e) =>
    openMenu({
      x: e.clientX,
      y: e.clientY + 6,
      items: [
        { label: `Run '${label}'`, icon: Play, iconColor: "var(--success)", shortcut: shortcutLabel("RunClass"), run: () => void launch(cfg()) },
        ...(debuggable ? [{ label: `Debug '${label}'`, icon: Bug, iconColor: "var(--success)", shortcut: shortcutLabel("DebugClass"), run: () => void debugConfiguration(cfg()) }] : []),
        { separator: true },
        { label: "Modify Run Configuration…", icon: Settings2, run: () => openRunConfigurations(cfg().name) },
      ],
    });
}

/** Files ktlint checked since they were opened (later checks run after saving). */
const linted = new Set<string>();

registerLineMarkerProvider({
  appliesTo: isKotlinSource,
  watch: active,
  markers: async (path, text) => {
    if (!active()) return [];
    if (!linted.has(path)) {
      linted.add(path);
      void ktlintFile(path, text);
    }
    if (isScript(path)) {
      return [{ line: 0, icon: Play, title: `Run '${fileName(path)}'`, className: "marker-run", onClick: runMenu(fileName(path), () => scriptConfiguration(path), false) }];
    }
    const mains = await kotlinApi.mainFunctions(path, text).catch(() => [] as MainFunction[]);
    return mains.map((m) => ({
      line: m.line,
      icon: Play,
      title: `Run '${m.name}'`,
      className: "marker-run",
      onClick: runMenu(m.name, () => mainConfiguration(path, m), true),
    }));
  },
});

// Ctrl+Shift+F10 in a Kotlin file with a main function (or a script) runs it.
const runClass = commands().get("RunClass");
if (runClass) {
  registerCommand({
    ...runClass,
    run: async () => {
      const tab = activeTab();
      if (tab && active() && isScript(tab.path)) return void launch(scriptConfiguration(tab.path));
      if (tab && active() && tab.path.endsWith(".kt")) {
        const [main] = await kotlinApi.mainFunctions(tab.path, tab.content).catch(() => [] as MainFunction[]);
        if (main) return void launch(mainConfiguration(tab.path, main));
      }
      return runClass.run();
    },
  });
}
