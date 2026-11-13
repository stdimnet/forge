/** Signals for git popups rendered by `GitOverlays` (diff viewer, branches popup, cards). */
import { createSignal } from "solid-js";
import type { BlameCommit } from "./api";

export interface DiffSide {
  title: string;
  content: string;
}

export interface DiffRequest {
  path: string;
  left: DiffSide;
  right: DiffSide;
  /** Right side is the working copy: editable, saved back on close. */
  editable: boolean;
  /** Other files the viewer can step through (Commit window selection). */
  files?: string[];
  /** HEAD vs working copy of a tracked change: checkboxes include single hunks in the commit. */
  partial?: boolean;
}

const [diffRequest, setDiffRequest] = createSignal<DiffRequest | null>(null);
export { diffRequest, setDiffRequest };

const [branchesOpen, setBranchesOpen] = createSignal(false);
export { branchesOpen, setBranchesOpen };

export interface BlameCard {
  x: number;
  y: number;
  commit: BlameCommit;
  path: string;
}
const [blameCard, setBlameCard] = createSignal<BlameCard | null>(null);
export { blameCard, setBlameCard };

export interface InputDialog {
  title: string;
  label: string;
  value: string;
  okLabel: string;
  /** Confirmation only, no text field. */
  noInput?: boolean;
  /** The text field may stay empty (e.g. an optional stash message). */
  optional?: boolean;
  checkbox?: { label: string; checked: boolean };
  onOk: (value: string, checked: boolean) => void | Promise<void>;
}
const [inputDialog, setInputDialog] = createSignal<InputDialog | null>(null);
export { inputDialog, setInputDialog };

export interface ChooserItem {
  id: string;
  label: string;
  /** Muted text after the label (hash, author, date, URL). */
  detail?: string;
}

/** A modal list to pick one entry from: revisions, branches, stashes, remotes. */
export interface ChooserDialog {
  title: string;
  items: ChooserItem[];
  empty: string;
  okLabel: string;
  checkboxes?: { label: string; checked: boolean }[];
  /** Buttons left of Cancel; `needsSelection` disables them while nothing is selected. */
  extra?: { label: string; needsSelection?: boolean; run: (item: ChooserItem | null) => void | Promise<void> }[];
  /** Without `onOk` the dialog only has a Close button. */
  onOk?: (item: ChooserItem, checked: boolean[]) => void | Promise<void>;
}
const [chooserDialog, setChooserDialog] = createSignal<ChooserDialog | null>(null);
export { chooserDialog, setChooserDialog };

/** Paths with "Annotate with Git Blame" switched on. */
const [annotated, setAnnotated] = createSignal<Set<string>>(new Set());
export { annotated };
export function toggleAnnotate(path: string, on?: boolean): void {
  setAnnotated((prev) => {
    const next = new Set(prev);
    const enable = on ?? !next.has(path);
    if (enable) next.add(path);
    else next.delete(path);
    return next;
  });
}
