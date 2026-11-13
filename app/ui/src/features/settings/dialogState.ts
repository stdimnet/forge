/**
 * State of the Settings dialog. Like PHPStorm, edits are kept as a draft until Apply/OK;
 * Cancel throws them away. The dialog opens as its own window in the desktop app
 * (`openDialogWindow("settings")`), so this state lives in the window that shows it.
 */
import { createSignal } from "solid-js";
import { closeDialog, notifyDialogOpener, openDialogWindow } from "@/shell/window/dialogs";
import { createStore, produce, reconcile } from "solid-js/store";
import type { SettingValue } from "@/ipc/settings";
import { findSpec, getSetting, newProjectValue, saveSettings, setPluginEnabled, settingsState } from "@/state/settings";

const [isOpen, setOpen] = createSignal(false);
/** Selected tree node; remembered between openings like PHPStorm does. */
const [selected, setSelected] = createSignal("appearance");
const [query, setQuery] = createSignal("");
const [applying, setApplying] = createSignal(false);
const [error, setError] = createSignal<string | null>(null);
/**
 * The dialog edits the Settings for New Projects (File | New Projects Setup): project-level
 * settings show and store the defaults for projects instead of the open project's values.
 */
const [forNewProjects, setForNewProjects] = createSignal(false);

/** Pending setting values by key (`null`: back to the inherited value). */
const [draft, setDraftStore] = createStore<Record<string, SettingValue | null>>({});
/** Pending plugin enablement by plugin id. */
const [pluginDraft, setPluginDraftStore] = createStore<Record<string, boolean>>({});

export { isOpen, selected, setSelected, query, setQuery, applying, error, draft, pluginDraft, forNewProjects };

/**
 * Edits that pages keep in their own store instead of the settings draft (the Code Style
 * schemes). Apply, OK, Cancel and Reset reach them through this interface; the getters must
 * be reactive.
 */
export interface DraftSource {
  /** Whether `pageId` has unapplied edits of this source. */
  modified: (pageId: string) => boolean;
  hasChanges: () => boolean;
  apply: () => Promise<void>;
  discard: () => void;
  /** Throw away the edits of one page (the "Reset" link). */
  resetPage: (pageId: string) => void;
}

const draftSources: DraftSource[] = [];

export function registerDraftSource(source: DraftSource): void {
  draftSources.push(source);
}

export const SETTINGS_DIALOG = "settings";

export interface SettingsDialogProps {
  /** Page to select; the last selected page when omitted. */
  page?: string;
  /** Data for the page, e.g. the text of a new live template (Save as Live Template). */
  args?: unknown;
  /** Edit the Settings for New Projects. */
  forNewProjects?: boolean;
}

/** Arguments handed to a page by `openSettings(page, args)`; the page takes them once. */
const [pageArgs, setPageArgs] = createSignal<{ page: string; args: unknown } | null>(null);

/** The arguments for `pageId`, cleared once taken. Reactive, so an open page sees new ones. */
export function takePageArgs<T>(pageId: string): T | undefined {
  const a = pageArgs();
  if (!a || a.page !== pageId) return undefined;
  queueMicrotask(() => setPageArgs((cur) => (cur === a ? null : cur)));
  return a.args as T;
}

/** Result of the dialog: the page to select next time (PHPStorm remembers it). */
export interface SettingsDialogResult {
  page: string;
}

export function openSettings(pageId?: string, args?: unknown): void {
  // This window shows the dialog already (e.g. a link to another page): just switch pages.
  if (isOpen()) {
    if (pageId) setSelected(pageId);
    if (pageId && args !== undefined) setPageArgs({ page: pageId, args });
    return;
  }
  void openDialogWindow(SETTINGS_DIALOG, { page: pageId ?? selected(), args } satisfies SettingsDialogProps);
}

/** File | New Projects Setup | Settings for New Projects…: the dialog in defaults mode. */
export function openSettingsForNewProjects(pageId?: string): void {
  if (isOpen()) return;
  void openDialogWindow(SETTINGS_DIALOG, { page: pageId ?? selected(), forNewProjects: true } satisfies SettingsDialogProps);
}

/** Called by the dialog content when it mounts (in-page or in its window). */
export function dialogOpened(props: SettingsDialogProps): void {
  setForNewProjects(!!props.forNewProjects);
  if (props.page) setSelected(props.page);
  setPageArgs(props.page && props.args !== undefined ? { page: props.page, args: props.args } : null);
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
  for (const source of draftSources) source.discard();
  setError(null);
}

/** Close the dialog (Cancel, or after OK applied the changes). */
export function closeSettings(): void {
  discardChanges();
  closeDialog(SETTINGS_DIALOG, { page: selected() } satisfies SettingsDialogResult);
}

/**
 * The stored value the dialog starts from: the effective value, or for project-level
 * settings in the Settings for New Projects their value for new projects.
 */
export function baseValue<T extends SettingValue>(key: string): T {
  if (forNewProjects() && findSpec(key)?.project) return newProjectValue<T>(key);
  return getSetting<T>(key);
}

/** Value shown in the dialog: the draft if edited, else the stored value. */
export const draftValue = <T extends SettingValue>(key: string): T => {
  if (!(key in draft)) return baseValue<T>(key);
  const v = draft[key];
  return (v === null ? findSpec(key)?.default : v) as T;
};

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

export function setDraft(key: string, value: SettingValue): void {
  if (same(value, baseValue(key))) {
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
  return Object.keys(draft).some((k) => findSpec(k)?.group === pageId) || draftSources.some((s) => s.modified(pageId));
}

export const hasChanges = () =>
  Object.keys(draft).length > 0 || Object.keys(pluginDraft).length > 0 || draftSources.some((s) => s.hasChanges());

/** Schema settings shown on a page (including those its custom editors keep). */
const pageSpecs = (pageId: string) => settingsState.schema.filter((s) => s.group === pageId);

/** Whether a page shows a value that differs from its default ("Restore Defaults" applies). */
export function pageDiffersFromDefaults(pageId: string): boolean {
  return pageSpecs(pageId).some((s) => !same(draftValue(s.key), s.default));
}

/** "Restore Defaults": draft every setting of the page back to its default (Apply stores it). */
export function restorePageDefaults(pageId: string): void {
  for (const spec of pageSpecs(pageId)) setDraft(spec.key, spec.default);
}

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
  for (const source of draftSources) source.resetPage(pageId);
}

export async function applyChanges(): Promise<boolean> {
  if (!hasChanges()) return true;
  setApplying(true);
  setError(null);
  try {
    await saveSettings({ ...draft }, forNewProjects());
    setDraftStore(reconcile({}));
    for (const source of draftSources) if (source.hasChanges()) await source.apply();
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
