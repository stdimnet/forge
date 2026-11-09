/**
 * Saving a generated file in browser mode: an `<a download>` click on an object URL. The URL
 * is revoked later, not right after the click: the browser starts the download
 * asynchronously, and revoking at once can cancel it.
 */

/** How long the object URL of a download stays valid. */
export const DOWNLOAD_URL_LIFETIME_MS = 60_000;

export function downloadBlob(name: string, blob: Blob): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), DOWNLOAD_URL_LIFETIME_MS);
}
