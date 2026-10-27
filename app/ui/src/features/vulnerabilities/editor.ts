/**
 * The "Vulnerable Dependencies" inspection in the editor: problems at the declarations of
 * vulnerable packages, recomputed while the manifest is edited, with the Alt+Enter fixes
 * "Upgrade '<package>' to <version>" and "Open advisory <id>".
 */
import type { ContextAction } from "@/ipc/codeintel";
import type { Upgrade } from "@/ipc/vulnerabilities";
import { registerLineMarkerProvider } from "@/editor/lineMarkers";
import { registerContextActionHandler, registerContextActionProvider } from "@/features/codeintel/intentions";
import { applyUpgrade, publishFile, resultFor, vulnerabilityProblems, vulnerabilityResults } from "./store";

interface UpgradeData {
  path: string;
  upgrade: Upgrade;
}

registerLineMarkerProvider({
  appliesTo: (path) => !!resultFor(path),
  watch: () => vulnerabilityResults(),
  markers: async (path, text) => {
    await publishFile(path, text);
    return [];
  },
});

const openUrl = (url: string): void => void window.open(url, "_blank", "noopener");

registerContextActionProvider(async (path, view) => {
  const r = resultFor(path);
  if (!r) return [];
  const pos = view.state.selection.main.head;
  const line = view.state.doc.lineAt(pos);
  const here = (vulnerabilityProblems()[path] ?? []).filter((p) => p.line === line.number - 1);
  const actions: ContextAction[] = [];
  for (const p of here) {
    if (p.upgrade) {
      actions.push({
        title: `Upgrade '${p.upgrade.package}' to ${p.upgrade.fixed}`,
        kind: "quickFix",
        source: "vulnerabilities",
        isPreferred: true,
        needsResolve: false,
        action: "Vulnerabilities.Upgrade",
        data: { path, upgrade: p.upgrade } satisfies UpgradeData,
      });
    }
    const v = r.vulnerabilities.find((x) => x.package === p.package && x.url);
    if (v?.url) {
      actions.push({
        title: `Open advisory ${v.id}`,
        kind: "intention",
        source: "vulnerabilities",
        isPreferred: false,
        needsResolve: false,
        action: "Vulnerabilities.OpenAdvisory",
        data: v.url,
      });
    }
  }
  return actions;
});

registerContextActionHandler("Vulnerabilities.Upgrade", (data) => {
  const d = data as UpgradeData;
  void applyUpgrade(d.path, d.upgrade);
});
registerContextActionHandler("Vulnerabilities.OpenAdvisory", (data) => openUrl(String(data)));
