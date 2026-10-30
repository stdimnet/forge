/**
 * Keyboard navigation inside the Code With Me popup and tool window, as in PHPStorm's
 * popups: Up / Down (and Home / End) move between the items marked `data-nav`, Enter or Space
 * activates the focused one. These are the widget's own keys, like a list's; shortcuts still
 * go through the keymap. Native controls (radio buttons, selects, buttons) keep their keys.
 */

export const NAV_ITEM = "[data-nav]";

/** The item to focus after `key`, from `current` (-1: none focused) among `count` items. */
export function nextIndex(current: number, count: number, key: string): number | null {
  if (count === 0) return null;
  switch (key) {
    case "ArrowDown":
      return current < 0 ? 0 : (current + 1) % count;
    case "ArrowUp":
      return current < 0 ? count - 1 : (current - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}

/** Handle a key inside `container`; true when it was used (the caller stops it). */
export function navigateItems(e: KeyboardEvent, container: HTMLElement): boolean {
  const target = e.target as HTMLElement | null;
  if (target && target.closest("input, select, textarea, button") && target !== container) return false;
  const items = [...container.querySelectorAll<HTMLElement>(NAV_ITEM)].filter((el) => el.getAttribute("aria-disabled") !== "true");
  const current = items.findIndex((el) => el === document.activeElement);
  if ((e.key === "Enter" || e.key === " ") && current >= 0) {
    items[current].click();
    return true;
  }
  const next = nextIndex(current, items.length, e.key);
  if (next === null) return false;
  items[next].focus();
  return true;
}

/** `onKeyDown` for a container of `data-nav` items. */
export function navKeys(e: KeyboardEvent & { currentTarget: HTMLElement }): void {
  if (e.altKey || e.ctrlKey || e.metaKey) return;
  if (navigateItems(e, e.currentTarget)) {
    e.preventDefault();
    e.stopPropagation();
  }
}
