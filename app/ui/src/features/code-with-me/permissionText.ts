/** Permissions as text, for tooltips and hints. */
import type { Permissions } from "@/ipc/collab";
import { AREAS } from "./CustomPermissions";

/** "Files: Edit · Terminal: No access · …" for custom permissions. */
export const describePermissions = (p: Permissions) =>
  AREAS.map((a) => `${a.label}: ${a.values.find((v) => v.value === p[a.area])?.label.replace(/ \(.*\)$/, "") ?? p[a.area]}`).join(" · ");
