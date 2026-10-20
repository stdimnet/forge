/**
 * State of the Settings dialog. Like PHPStorm, edits are kept as a draft until Apply/OK;
 * Cancel throws them away. The dialog opens as its own window in the desktop app
 * (`openDialogWindow("settings")`), so this state lives in the window that shows it.
 */
import { createSignal } from "solid-js";
import { closeDialog, notifyDialogOpener, openDialogWindow } from "@/shell/window/dialogs";
import { createStore, produce, reconcile } from "solid-js/store";
import type { SettingValue } from "@/ipc/settings";
import { findSpec, getSetting, saveSettings, setPluginEnabled, settingsState } from "@/state/settings";

const [isOpen, setOpen] = createSignal(false);
/** Selected tree node; remembered between openings like PHPStorm does. */
const [selected, setSelected] = createSignal("appearance");
const [query, setQuery] = createSignal("");
const [applying, setApplying] = createSignal(false);
const [error, setError] = createSignal<string | null>(null);

/** Pending setting values by key. */
const [draft, setDraftStore] = createStore<Record<string, SettingValue>>({});
/** Pending plugin enablement by plugin id. */
const [pluginDraft, setPluginDraftStore] = createStore<Record<string, boolean>>({});

export { isOpen, selected, setSelected, query, setQuery, applying, error, draft, pluginDraft };

export const SETTINGS_DIALOG = "settings";

export interface SettingsDialogProps {
  /** Page to select; the last selected page when omitted. */
  page?: string;
}

/** Result of the dialog: the page to select next time (PHPStorm remembers it). */
export interface SettingsDialogResult {
  page: string;
}

export function openSettings(pageId?: string): void {
  // This window shows the dialog already (e.g. a link to another page): just switch pages.
  if (isOpen()) {
    if (pageId) setSelected(pageId);
    return;
  }
  void openDialogWindow(SETTINGS_DIALOG, { page: pageId ?? selected() } satisfies SettingsDialogProps);
}

/** Called by the dialog content when it mounts (in-page or in its window). */
export function dialogOpened(props: SettingsDialogProps): void {
  if (props.page) setSelected(props.page);
  setError(null);
  setOpen(true);
}

/** Called when the dialog content unmounts. */
export function dialogClosed(): void {
  discardChanges();
  setQuery("");
  setOpen(false);
}

export function discardChanges(): void {
  setDraftStore(reconcile({}));
  setPluginDraftStore(reconcile({}));
  setError(null);
}

/** Close the dialog (Cancel, or after OK applied the changes). */
export function closeSettings(): void {
  discardChanges();
  closeDialog(SETTINGS_DIALOG, { page: selected() } satisfies SettingsDialogResult);
}

/** Value shown in the dialog: the draft if edited, else the effective value. */
export const draftValue = <T extends SettingValue>(key: string): T =>
  (key in draft ? draft[key] : getSetting(key)) as T;

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

export function setDraft(key: string, value: SettingValue): void {
  if (same(value, getSetting(key))) {
    setDraftStore(produce((d) => void delete d[key]));
  } else {
    setDraftStore(key, value);
  }
}

export const pluginEnabled = (id: string): boolean =>
  id in pluginDraft ? pluginDraft[id] : (settingsState.plugins.find((p) => p.id === id)?.enabled ?? false);

export function setPluginDraftEnabled(id: string, enabled: boolean): void {
  const current = settingsState.plugins.find((p) => p.id === id)?.enabled;
  if (current === enabled) setPluginDraftStore(produce((d) => void delete d[id]));
  else setPluginDraftStore(id, enabled);
}

/** Whether a settings page has unapplied edits. */
export function pageModified(pageId: string): boolean {
  if (pageId === "plugins") return Object.keys(pluginDraft).length > 0;
  return Object.keys(draft).some((k) => findSpec(k)?.group === pageId);
}

export const hasChanges = () => Object.keys(draft).length > 0 || Object.keys(pluginDraft).length > 0;

/** Discard the draft of one page (the "Reset" link). */
export function resetPage(pageId: string): void {
  if (pageId === "plugins") {
    setPluginDraftStore(reconcile({}));
    return;
  }
  setDraftStore(
    produce((d) => {
      for (const k of Object.keys(d)) if (findSpec(k)?.group === pageId) delete d[k];
    }),
  );
}

export async function applyChanges(): Promise<boolean> {
  if (!hasChanges()) return true;
  setApplying(true);
  setError(null);
  try {
    await saveSettings({ ...draft });
    setDraftStore(reconcile({}));
    for (const [id, enabled] of Object.entries({ ...pluginDraft })) await setPluginEnabled(id, enabled);
    setPluginDraftStore(reconcile({}));
    // The window that opened the dialog refreshes its plugin list and setting layers.
    notifyDialogOpener(SETTINGS_DIALOG, "applied");
    return true;
  } catch (e) {
    setError(String(e instanceof Error ? e.message : e));
    return false;
  } finally {
    setApplying(false);
  }
}
