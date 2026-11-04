/**
 * Run injected SQL: the value of a SQL string (unescaped, joined over concatenations, by
 * Rust) runs in a query console of the selected data source, with the Database tool
 * window's usual execution and result grid.
 */
import { showBalloon } from "@/keymap";
import { showToolWindow } from "@/state/layout";
import { consoles } from "@/features/database/controllers";
import { loadSources, openConsole, openDataSources, selectedSourceId, sources } from "@/features/database/store";
import { t } from "@/i18n";

const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** Open a console with `sql` for the selected data source and execute it. */
export async function runSqlInConsole(sql: string): Promise<boolean> {
  const text = sql.trim();
  if (!text) return false;
  if (!sources().length) await loadSources().catch(() => undefined);
  const id = selectedSourceId();
  if (!id) {
    showBalloon(t("tools.injections.noDataSource"), t("tools.injections.addADataSourceIn"), 6000);
    showToolWindow("database");
    openDataSources({ newDriver: "mysql" });
    return false;
  }
  showToolWindow("database");
  const path = openConsole(id, text);
  // The console registers its controller when its view mounts.
  for (let i = 0; i < 100; i++) {
    const c = consoles.get(path);
    if (c) {
      await delay(30);
      await c.execute();
      return true;
    }
    await delay(20);
  }
  return false;
}
