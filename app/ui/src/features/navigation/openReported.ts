import { showBalloon } from "@/keymap";
import { openFile } from "@/state/editor";
import { forgetRecent } from "./history";
import { t } from "@/i18n";

/** Open a file the user picked; a refusal (a guest asking for a hidden file) becomes a balloon. */
export async function openFileReported(path: string): Promise<void> {
  try {
    await openFile(path);
  } catch (e) {
    forgetRecent(path);
    showBalloon(t("tools.navigation.cannotOpen", { value: path.split("/").pop() ?? path }), String((e as Error)?.message ?? e));
  }
}
