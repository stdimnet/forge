/**
 * Import External Coverage Report (a report file of the project read in Rust,
 * `coverage_import`) and Generate Coverage Report (the shown suite as HTML, `coverage_export_html`).
 */
import { isTauri } from "@/ipc";
import { testingApi } from "@/ipc/testing";
import { balloonsFor } from "@/keymap";
import { workspace } from "@/state/workspace";
import { activeSuite, showSuite } from "./store";
import { t } from "@/i18n";

const showBalloon = balloonsFor("Coverage");

/** Read a report (LCOV, Cobertura, Clover, JaCoCo, Go profile, Istanbul JSON) and show it. */
export async function importCoverageFile(path: string): Promise<void> {
  try {
    const suite = await testingApi.coverageImport(path);
    await showSuite(suite.id);
  } catch (err) {
    showBalloon(t("tools.coverage.cannotReadTheCoverageReport"), String(err));
  }
}

/** Pick a report file of the project and import it. */
export async function importReport(): Promise<void> {
  if (!isTauri()) {
    showBalloon(t("tools.coverage.importExternalCoverageReport"), t("tools.coverage.rightClickAReportIn"));
    return;
  }
  const { open } = await import("@tauri-apps/plugin-dialog");
  const root = workspace()?.root;
  const picked = await open({
    multiple: false,
    defaultPath: root,
    filters: [{ name: "Coverage reports", extensions: ["info", "lcov", "xml", "out", "txt", "json"] }],
  });
  if (typeof picked !== "string") return;
  await importCoverageFile(root && picked.startsWith(`${root}/`) ? picked.slice(root.length + 1) : picked);
}

/** The report's file name: "coverage-<suite>.html". */
const reportName = (name: string) => `coverage-${name.replace(/[^\w.-]+/g, "-").replace(/^-+|-+$/g, "") || "report"}.html`;

/** Generate Coverage Report: save the shown suite as one HTML file (browser mode: a download). */
export async function generateReport(): Promise<void> {
  const suite = activeSuite();
  if (!suite) {
    showBalloon(t("tools.coverage.generateCoverageReport"), t("tools.coverage.thereIsNoCoverageData"));
    return;
  }
  const file = reportName(suite.name);
  try {
    if (isTauri()) {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const root = workspace()?.root;
      const path = await save({ defaultPath: root ? `${root}/${file}` : file, filters: [{ name: "HTML", extensions: ["html"] }] });
      if (!path) return;
      await testingApi.coverageExportHtml(suite.id, path);
      showBalloon(t("tools.coverage.coverageReportGenerated"), path);
      return;
    }
    const html = await testingApi.coverageExportHtml(suite.id);
    const url = URL.createObjectURL(new Blob([html], { type: "text/html" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = file;
    a.click();
    URL.revokeObjectURL(url);
    showBalloon(t("tools.coverage.coverageReportGenerated"), file);
  } catch (err) {
    showBalloon(t("tools.coverage.cannotGenerateTheCoverageReport"), String(err));
  }
}
