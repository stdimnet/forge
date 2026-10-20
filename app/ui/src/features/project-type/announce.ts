/** Decides the one-time "framework detected" notification. Pure, so it is unit-tested. */
import type { ProjectInfo } from "@/ipc/project";
import type { PluginInfo } from "@/ipc/settings";

export interface Announcement {
  /** `<root>#<framework id>`, remembered so the balloon shows once per project. */
  key: string;
  title: string;
  detail: string;
  /** Settings pages to try for "Configure", most specific first. */
  settingsPages: string[];
}

export function announcement(
  info: ProjectInfo,
  root: string,
  plugins: Pick<PluginInfo, "id" | "name" | "enabled" | "projectTypes">[],
  announced: string[],
): Announcement | null {
  const fw = info.primary;
  // Only frameworks are announced; a plain language project is not news.
  if (!fw || fw.id === fw.language) return null;
  const key = `${root}#${fw.id}`;
  if (announced.includes(key)) return null;
  const enabled = plugins.filter((p) => p.enabled && p.projectTypes?.includes(fw.id));
  if (!enabled.length) return null;
  return {
    key,
    title: `${fw.name} project detected`,
    detail: `${enabled.map((p) => p.name).join(", ")} support enabled.`,
    settingsPages: [`lang.${fw.language}.${fw.id}`, `lang.${fw.id}`, "plugins"],
  };
}
