/**
 * Editor views: tab contents that are not files, such as SQL consoles and table editors.
 * A feature registers a view id once and opens tabs with `openViewTab({ view: id, ... })`.
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
