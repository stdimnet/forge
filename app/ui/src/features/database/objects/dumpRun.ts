/**
 * Dumps and restores as runs of the Run tool window: a hidden run configuration type whose
 * runs start through `db_dump_start` (the backend builds the command line, passes the
 * password in the environment and keeps the SSH tunnel open while the tool runs).
 */
import { DatabaseBackup } from "lucide-solid";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { registerRunConfigurationType } from "@/features/run/types";
import { launch } from "@/features/run/runStore";
import { dbObjectsApi, type DumpRequest } from "@/ipc/dbObjects";
import { showBalloon } from "@/keymap";
import { t } from "@/i18n";

export const DUMP_RUN_TYPE = "db-dump";

registerRunConfigurationType({
  id: DUMP_RUN_TYPE,
  name: "Database Dump",
  description: t("database.objects.dumpRun.aDumpOrRestoreWith"),
  icon: DatabaseBackup,
  color: "var(--info)",
  fields: [],
  argumentsLabel: false,
  noRunTarget: true,
  hidden: true,
  start: (cfg, request) =>
    dbObjectsApi.dumpStart(cfg.options!.dataSource, JSON.parse(cfg.options!.request) as DumpRequest, request.runId!, cfg.options!.confirmed === "true"),
});

/** Start a dump or restore: restores are refused on read-only data sources and ask first
 * on production ones. */
export async function startDump(ds: string, request: DumpRequest, name: string): Promise<boolean> {
  let confirmed = false;
  if (request.action === "restore") {
    let warning: string | null;
    try {
      warning = await dbObjectsApi.restoreGuard(ds);
    } catch (e) {
      showBalloon(t("database.objects.dumpRun.restoreNotAllowed"), e instanceof Error ? e.message : String(e));
      return false;
    }
    if (warning) {
      if (!(await askConfirm({ title: t("database.objects.dumpRun.productionDataSource"), message: warning, confirm: t("database.objects.dumpRun.restore") }))) return false;
      confirmed = true;
    }
  }
  const handle = await launch({
    name,
    type: DUMP_RUN_TYPE,
    options: { dataSource: ds, request: JSON.stringify(request), confirmed: String(confirmed) },
  });
  return !!handle;
}
