/**
 * Package Checker (plugin `forge.vulnerabilities`), like IntelliJ's: Code | Analyze Code |
 * Check for Vulnerable Dependencies, automatic checks on project open and when a manifest
 * or lock file changes (cached per lock file by the backend), the "Vulnerable
 * Dependencies" inspection with the Upgrade quick fix, the Vulnerable Dependencies tab of
 * the Problems tool window and Settings | Tools | Package Checker. Logic lives in Rust
 * (`crates/forge-vulns`, `commands/vulnerabilities.rs`).
 */
import { createEffect, createRoot, on } from "solid-js";
import { ShieldAlert } from "lucide-solid";
import type { CommandLine } from "@/ipc/run";
import { registerKeymapActions } from "@/keymap";
import { registerProblemsTab } from "@/features/lsp/problemsTabs";
import { registerRunConfigurationType } from "@/features/run/types";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { safeMode } from "@/features/trust/store";
import { isDialogWindow } from "@/shell/window/mode";
import { registerCommand } from "@/state/commands";
import { onFsChanged } from "@/state/fsWatch";
import { isPluginActive } from "@/state/project";
import { getSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { clearResults, COMMAND_TYPE, runCheck, vulnerabilityCount, vulnerabilityResults, VULNERABILITIES_TAB } from "./store";
import { VulnerabilitiesView } from "./VulnerabilitiesView";
import "./editor";
import "./vulnerabilities.css";
import { t } from "@/i18n";

export const PLUGIN = "forge.vulnerabilities";
const active = () => isPluginActive(PLUGIN);
const automatic = () => active() && !safeMode() && getSetting<boolean>("vulnerabilities.enabled", true) && getSetting<boolean>("vulnerabilities.checkOnOpen", true);

// Install steps after an upgrade (`composer update <pkg>`, `npm install`, `pip install`),
// built by the backend; not offered in Run/Debug Configurations.
registerRunConfigurationType({
  id: COMMAND_TYPE,
  name: "Package Checker",
  icon: ShieldAlert,
  pluginId: PLUGIN,
  hidden: true,
  fields: [],
  argumentsLabel: false,
  buildCommand: (cfg) => JSON.parse(cfg.options?.commandLine ?? "{}") as CommandLine,
});

registerKeymapActions([{ id: "Vulnerabilities.Check", title: "Check for Vulnerable Dependencies", group: "Code" }]);

registerCommand({
  id: "Vulnerabilities.Check",
  title: "Check for Vulnerable Dependencies",
  category: "Code",
  run: () => void runCheck({ force: true, reveal: true }),
});

registerProblemsTab({
  id: VULNERABILITIES_TAB,
  title: "Vulnerable Dependencies",
  order: 10,
  count: vulnerabilityCount,
  visible: active,
  component: VulnerabilitiesView,
});

registerSettingsPage({
  id: "tools.vulnerabilities",
  title: t("settings.page.tools.vulnerabilities"),
  group: "Tools",
  order: 60,
  keywords: ["vulnerable", "vulnerabilities", "security", "CVE", "advisory", "audit", "npm audit", "composer audit", "pip-audit", "cargo audit", "govulncheck", "dependency-check"],
  component: () => <SchemaPage pageId="tools.vulnerabilities" />,
});

/** Files whose change re-runs the check (the cache key tells whether it really changed). */
const WATCHED = /^(composer\.(json|lock)|package(-lock)?\.json|npm-shrinkwrap\.json|pnpm-lock\.yaml|yarn\.lock|requirements[^/]*\.txt|pyproject\.toml|uv\.lock|poetry\.lock|Pipfile(\.lock)?|Cargo\.(toml|lock)|go\.(mod|sum))$/;

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    createEffect(
      on(
        () => workspace()?.root,
        (root) => {
          clearResults();
          if (!root) return;
          // Once the project's plugins and trust are known; throttled by the backend's cache.
          setTimeout(() => {
            if (workspace()?.root === root && automatic()) void runCheck({ quiet: true });
          }, 4000);
        },
      ),
    );
    createEffect(
      on(
        () => getSetting<boolean>("vulnerabilities.enabled", true),
        (on, before) => {
          if (on === false) clearResults();
          else if (before === false && automatic()) void runCheck({ quiet: true });
        },
        { defer: true },
      ),
    );
  });

  let timer: ReturnType<typeof setTimeout> | undefined;
  onFsChanged((c) => {
    const inputs = new Set(vulnerabilityResults().flatMap((r) => r.inputs));
    if (![...c.changed, ...c.removed].some((p) => inputs.has(p) || WATCHED.test(p))) return;
    clearTimeout(timer);
    timer = setTimeout(() => {
      if (automatic()) void runCheck({ quiet: true });
    }, 2000);
  });
}
