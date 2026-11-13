/**
 * Keyboard navigation of the Project tree, kept free of DOM and Solid so it is testable.
 * Works on the currently visible rows in display order.
 */
export interface VisibleRow {
  path: string;
  isDir: boolean;
  expanded: boolean;
  depth: number;
}

export type TreeAction =
  | { type: "select"; path: string }
  | { type: "expand"; path: string }
  | { type: "collapse"; path: string }
  | { type: "open"; path: string }
  | { type: "none" };

export function navigate(rows: VisibleRow[], selected: string | null, key: string): TreeAction {
  if (rows.length === 0) return { type: "none" };
  const idx = rows.findIndex((r) => r.path === selected);
  if (idx < 0) return { type: "select", path: rows[0].path };
  const row = rows[idx];

  switch (key) {
    case "ArrowDown":
      return idx + 1 < rows.length ? { type: "select", path: rows[idx + 1].path } : { type: "none" };
    case "ArrowUp":
      return idx > 0 ? { type: "select", path: rows[idx - 1].path } : { type: "none" };
    case "Home":
      return { type: "select", path: rows[0].path };
    case "End":
      return { type: "select", path: rows[rows.length - 1].path };
    case "ArrowRight":
      if (!row.isDir) return { type: "none" };
      if (!row.expanded) return { type: "expand", path: row.path };
      // Already open: move to the first child, if any.
      return rows[idx + 1]?.depth > row.depth ? { type: "select", path: rows[idx + 1].path } : { type: "none" };
    case "ArrowLeft": {
      if (row.isDir && row.expanded) return { type: "collapse", path: row.path };
      for (let i = idx - 1; i >= 0; i--) {
        if (rows[i].depth < row.depth) return { type: "select", path: rows[i].path };
      }
      return { type: "none" };
    }
    case "Enter":
    case "F4":
      return row.isDir ? { type: row.expanded ? "collapse" : "expand", path: row.path } : { type: "open", path: row.path };
    default:
      return { type: "none" };
  }
}

/**
 * Arrow keys, Home and End in a flat list (`role="listbox"` with roving `tabIndex` options):
 * the id to select next, or null when the key does nothing.
 */
export function navigateList(ids: readonly string[], selected: string | null, key: string): string | null {
  const action = navigate(
    ids.map((path) => ({ path, isDir: false, expanded: false, depth: 0 })),
    selected,
    key,
  );
  return action.type === "select" ? action.path : null;
}

/**
 * Keyboard handler for a `role="listbox"` whose options carry `data-option-id` and a roving
 * `tabIndex`: Up/Down/Home/End select and focus the next option. Keys typed into a control
 * inside an option (an input in an editable row) are left alone.
 */
export function listboxKeyDown(
  e: KeyboardEvent & { currentTarget: HTMLElement },
  ids: readonly string[],
  selected: string | null,
  select: (id: string) => void,
): void {
  if (!(e.target instanceof Element) || e.target.getAttribute("role") !== "option") return;
  const next = navigateList(ids, selected, e.key);
  if (!next) return;
  e.preventDefault();
  select(next);
  // Compare ids as data, not through a selector (ids may contain any character).
  [...e.currentTarget.querySelectorAll<HTMLElement>("[data-option-id]")].find((el) => el.dataset.optionId === next)?.focus();
}

/** Roving tabIndex of a list option: 0 on the selected one (or the first without selection). */
export const optionTabIndex = (id: string, selected: string | null, first: string | undefined): 0 | -1 =>
  id === selected || (selected === null && id === first) ? 0 : -1;
