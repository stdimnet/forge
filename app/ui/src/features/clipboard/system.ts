/**
 * The system clipboard for every feature: Paste, terminal paste, Compare with Clipboard,
 * Apply Patch from Clipboard, Copy actions, … The desktop app reads and writes it natively
 * (`clipboard_read_text` / `clipboard_write_text`, `tauri-plugin-clipboard-manager`),
 * because WebKitGTK refuses `navigator.clipboard.readText()` outside a native paste event;
 * browser mode uses `navigator.clipboard`. A failure shows a balloon instead of doing
 * nothing.
 */
import { clipboardApi } from "@/ipc/clipboard";
import { showBalloon } from "@/keymap";
import { t } from "@/i18n";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e ?? ""));

export interface ReadOptions {
  /** Give up after this many milliseconds (a browser permission prompt may never be answered); resolves null then. */
  timeoutMs?: number;
  /** No balloon on failure (an optional prefill, like Analyze Stack Trace). */
  quiet?: boolean;
}

/** The clipboard text ("" when it holds none), or null when it cannot be read (after a balloon). */
export async function readClipboard(opts: ReadOptions = {}): Promise<string | null> {
  const read = clipboardApi.readText().then(
    (text) => text ?? "",
    (e: unknown) => {
      if (opts.quiet) console.warn("Cannot read the clipboard:", e);
      else showBalloon(t("clipboard.readFailed"), message(e), 5000, undefined, { type: "error" });
      return null;
    },
  );
  if (!opts.timeoutMs) return read;
  const timeout = new Promise<null>((resolve) => setTimeout(() => resolve(null), opts.timeoutMs));
  return Promise.race([read, timeout]);
}

/** Put a text on the clipboard; false (after a balloon) when it cannot be written. */
export async function writeClipboard(text: string): Promise<boolean> {
  try {
    await clipboardApi.writeText(text);
    return true;
  } catch (e) {
    showBalloon(t("clipboard.writeFailed"), message(e), 5000, undefined, { type: "error" });
    return false;
  }
}

/** Put a text on the clipboard and reject on failure, for callers that report errors themselves. */
export const writeClipboardText = (text: string): Promise<void> => clipboardApi.writeText(text);
