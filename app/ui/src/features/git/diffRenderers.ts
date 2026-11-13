/**
 * Diff renderers: views a file type brings into the diff viewer instead of the text diff,
 * like PyCharm's notebook diff for `.ipynb` files. The viewer offers a switch between the
 * renderer and the plain text diff.
 */
import type { Component } from "solid-js";
import type { DiffRequest } from "./overlays";

export interface DiffRenderer {
  id: string;
  /** Label of the switch ("Cells"). */
  title: string;
  accepts: (path: string) => boolean;
  /** Shown first (else the text diff is). */
  preferred?: () => boolean;
  component: Component<{ request: DiffRequest }>;
}

const renderers: DiffRenderer[] = [];

export function registerDiffRenderer(r: DiffRenderer): void {
  const at = renderers.findIndex((x) => x.id === r.id);
  if (at >= 0) renderers.splice(at, 1);
  renderers.push(r);
}

export const diffRendererFor = (path: string): DiffRenderer | undefined => renderers.find((r) => r.accepts(path));
