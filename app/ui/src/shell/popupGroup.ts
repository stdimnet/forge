/**
 * Only one toolbar or status bar popup is open at a time, as in PHPStorm: opening a popup
 * closes the one that was open before (e.g. the Code With Me session popup and a status bar
 * widget's popup).
 */

let current: { close: () => void } | null = null;

/**
 * Registers a popup that has just opened and closes the previous one. Call the returned
 * function when the popup closes.
 */
export function claimPopup(close: () => void): () => void {
  const entry = { close };
  const previous = current;
  current = entry;
  if (previous) previous.close();
  return () => {
    if (current === entry) current = null;
  };
}
