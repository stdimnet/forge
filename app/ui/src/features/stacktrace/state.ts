/** The Analyze Stack Trace dialog: open with the clipboard's text when it holds a trace. */
import { createSignal } from "solid-js";
import { readClipboard } from "@/features/clipboard/system";

const [dialog, setDialog] = createSignal<{ text: string } | null>(null);
export { dialog as analyzeDialog };
export const closeAnalyzeDialog = () => setDialog(null);

/** Something that looks like a stack trace frame (PHP, Node, Lua, `file:line`). */
const TRACE = /(^|\n)\s*(#\d+ |at .+[:(]\d+|.+\.\w+:\d+)/;

export async function openAnalyzeDialog(text?: string): Promise<void> {
  if (text === undefined) {
    // Reading the clipboard may wait for a permission prompt; do not wait long for it.
    const clip = await readClipboard({ timeoutMs: 300, quiet: true });
    text = clip && TRACE.test(clip) ? clip : "";
  }
  setDialog({ text });
}
