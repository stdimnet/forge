/** Import External Coverage Report: a report file of the project read in Rust (`coverage_import`). */
import { isTauri } from "@/ipc";
import { testingApi } from "@/ipc/testing";
import { balloonsFor } from "@/keymap";
import { workspace } from "@/state/workspace";
import { showSuite } from "./store";

const showBalloon = balloonsFor("Coverage");

/** Read a report (LCOV, Cobertura, Clover, JaCoCo, Go profile) and show it. */
export async function importCoverageFile(path: string): Promise<void> {
  try {
    const suite = await testingApi.coverageImport(path);
    await showSuite(suite.id);
  } catch (err) {
    showBalloon("Cannot read the coverage report", String(err));
  }
}

/** Pick a report file of the project and import it. */
export async function importReport(): Promise<void> {
  if (!isTauri()) {
    showBalloon("Import External Coverage Report", "Right-click a report in the Project view, or use the desktop app to pick a file.");
    return;
  }
  const { open } = await import("@tauri-apps/plugin-dialog");
  const root = workspace()?.root;
  const picked = await open({
    multiple: false,
    defaultPath: root,
    filters: [{ name: "Coverage reports", extensions: ["info", "lcov", "xml", "out", "txt"] }],
  });
  if (typeof picked !== "string") return;
  await importCoverageFile(root && picked.startsWith(`${root}/`) ? picked.slice(root.length + 1) : picked);
}
