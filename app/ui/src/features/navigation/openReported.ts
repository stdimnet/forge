import { showBalloon } from "@/keymap";
import { openFile } from "@/state/editor";
import { forgetRecent } from "./history";

/** Open a file the user picked; a refusal (a guest asking for a hidden file) becomes a balloon. */
export async function openFileReported(path: string): Promise<void> {
  try {
    await openFile(path);
  } catch (e) {
    forgetRecent(path);
    showBalloon(`Cannot open ${path.split("/").pop() ?? path}`, String((e as Error)?.message ?? e));
  }
}
