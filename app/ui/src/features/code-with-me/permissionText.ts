/** Permissions as text, for tooltips and hints. */
import type { Permissions } from "@/ipc/collab";
import { t } from "@/i18n";
import { AREAS } from "./CustomPermissions";

/** The short name of `p`'s value for one area ("Read-only"), or the raw value. */
const short = (p: Permissions, area: keyof Permissions) =>
  AREAS.find((a) => a.area === area)?.values.find((v) => v.value === p[area])?.short ?? p[area];

/** "Files: Edit · Terminal: No access · …" for custom permissions. */
export const describePermissions = (p: Permissions) =>
  t("tools.codeWithMe.permissionsSummary", {
    files: short(p, "files"),
    terminal: short(p, "terminal"),
    run: short(p, "run"),
    vcs: short(p, "vcs"),
  });
