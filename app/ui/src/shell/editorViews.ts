/**
 * Editor views: tab contents that are not files, such as SQL consoles and table editors.
 * A feature registers a view id once and opens tabs with `openViewTab({ view: id, ... })`.
 *
 * File editors (`registerFileEditor`) replace the plain text editor of some files with a
 * richer one that usually embeds it, like PHPStorm's file editor providers: the Markdown
 * editor with preview, SVG with preview, CSV with "Edit as Table". Binary files (images)
 * open as view tabs through `registerBinaryFileView` (`state/editor.ts`).
 */
import type { Component } from "solid-js";
import { createSignal } from "solid-js";
import type { EditorTab } from "@/state/editor";

export interface EditorView {
  id: string;
  component: Component<{ tab: EditorTab }>;
  /** Tab icon; defaults to the file icon of the tab name. */
  icon?: Component<{ size?: number; tab: EditorTab }>;
  /** Breadcrumb parts shown above the view (defaults to the tab name). */
  breadcrumbs?: (tab: EditorTab) => string[];
}

const [views, setViews] = createSignal<Map<string, EditorView>>(new Map());

export function registerEditorView(view: EditorView): void {
  setViews((prev) => new Map(prev).set(view.id, view));
}

export const editorViewFor = (tab: EditorTab | undefined): EditorView | undefined =>
  tab?.view ? views().get(tab.view) : undefined;

/** An editor for text files of some kind; it gets the tab and the editor group it shows in. */
export interface FileEditorProvider {
  id: string;
  accepts: (path: string) => boolean;
  component: Component<{ tab: EditorTab; group?: number }>;
}

const [fileEditors, setFileEditors] = createSignal<FileEditorProvider[]>([]);

export function registerFileEditor(provider: FileEditorProvider): void {
  setFileEditors((prev) => [...prev.filter((p) => p.id !== provider.id), provider]);
}

/** The file editor of a file tab (view tabs have none). */
export const fileEditorFor = (tab: EditorTab | undefined): FileEditorProvider | undefined =>
  tab && !tab.view ? fileEditors().find((p) => p.accepts(tab.path)) : undefined;
