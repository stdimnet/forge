/**
 * Entries other features add to the Generate popup (Alt+Insert), below the language's own
 * (e.g. "Documentation with AI..."). In files without a Generate implementation the popup
 * shows only these.
 */
import type { EditorView } from "@codemirror/view";

export interface GenerateItem {
  label: string;
  /** Index of the mnemonic letter in `label`. */
  mnemonic: number;
  enabled: boolean;
  run: () => void;
  shortcut?: string;
}

export type GenerateItemsProvider = (path: string, view: EditorView) => GenerateItem[];

const providers: GenerateItemsProvider[] = [];

export function registerGenerateItems(provider: GenerateItemsProvider): void {
  providers.push(provider);
}

export const generateItems = (path: string, view: EditorView): GenerateItem[] => providers.flatMap((p) => p(path, view));
