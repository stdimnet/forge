/**
 * "Add data source from .env": Laravel's DB_* variables and Symfony's DATABASE_URL are offered
 * as a data source once per project and session (setting `database.suggestFromEnv`).
 */
import { createSignal } from "solid-js";
import { dbApi, DRIVER_LABELS, type DetectedDataSource } from "@/ipc/db";
import { showBalloon } from "@/keymap";
import { getSetting } from "@/state/settings";
import { showToolWindow } from "@/state/layout";
import { workspace } from "@/state/workspace";
import { connect, revealSource, setSelectedNode } from "./store";

const [detected, setDetected] = createSignal<DetectedDataSource[]>([]);
export { detected };

const announced = new Set<string>();

/** Look for an unconfigured `.env` database; `announce` shows the notification once. */
export async function checkEnv(announce = true): Promise<void> {
  if (!workspace() || getSetting<boolean>("database.suggestFromEnv", true) === false) {
    setDetected([]);
    return;
  }
  let found: DetectedDataSource[] = [];
  try {
    found = await dbApi.detectEnv();
  } catch {
    /* no project open in the backend yet */
  }
  setDetected(found);
  const first = found[0];
  if (!announce || !first) return;
  const key = `${workspace()?.root}#${first.source.driver}:${first.source.name}`;
  if (announced.has(key)) return;
  announced.add(key);
  showBalloon(
    `Database found in ${first.origin}`,
    `${DRIVER_LABELS[first.source.driver]} · ${first.source.name}`,
    15000,
    [{ label: "Add data source", run: () => void addFromEnv() }],
  );
}

/** Add the detected data sources, open the Database tool window and connect. */
export async function addFromEnv(): Promise<void> {
  try {
    const added = await dbApi.addFromEnv();
    setDetected([]);
    const ds = added[0];
    if (!ds) return;
    showToolWindow("right", "database");
    setSelectedNode(`ds:${ds.id}`);
    if (await connect(ds.id)) revealSource(ds.id);
  } catch (e) {
    showBalloon("Cannot add the data source", e instanceof Error ? e.message : String(e));
  }
}
