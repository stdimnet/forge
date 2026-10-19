/**
 * Settings dialog contribution point. Features register pages here; the Settings dialog
 * (Ctrl+Alt+S) renders them in a searchable tree grouped like PHPStorm
 * ("Appearance & Behavior", "Editor", "Version Control", "Languages & Frameworks", "Tools", ...).
 */
import type { Component } from "solid-js";
import { createSignal } from "solid-js";

export interface SettingsPage {
  id: string;
  title: string;
  /** Top-level group, e.g. "Tools" or "Languages & Frameworks". */
  group: string;
  /** Optional parent page id for nesting (e.g. "lang.php" under "Languages & Frameworks"). */
  parent?: string;
  order?: number;
  /** Extra words the settings search should match. */
  keywords?: string[];
  component: Component;
}

const [settingsPages, setSettingsPages] = createSignal<SettingsPage[]>([]);

export function registerSettingsPage(page: SettingsPage): void {
  setSettingsPages((prev) =>
    [...prev.filter((p) => p.id !== page.id), page].sort((a, b) => (a.order ?? 100) - (b.order ?? 100)),
  );
}

export { settingsPages };
