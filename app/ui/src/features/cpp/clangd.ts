/**
 * Download clangd with the user's consent: the pinned release from github.com/clangd/clangd
 * (Apache-2.0 WITH LLVM-exception), checked against its SHA-256 and unpacked into Forge's
 * data folder (`cpp_clangd_install`, events `cpp://install`). Offered once per session when
 * a C/C++ project opens without clangd.
 */
import { confirmDialog } from "@/features/browserCompanion/ConfirmDialog";
import { cppApi } from "@/ipc/cpp";
import { refreshCppStatus, showCppBalloon } from "./state";

let installing: Promise<boolean> | null = null;
let offered = false;

export function installClangd(): Promise<boolean> {
  installing ??= (async () => {
    const status = await refreshCppStatus();
    const version = status?.clangd.pinnedVersion;
    if (!version) {
      showCppBalloon("clangd", "The clangd releases have no build for this platform: install clangd with your package manager or LLVM.", 8000);
      return false;
    }
    const ok = await confirmDialog({
      title: "Download clangd",
      message: `Forge downloads clangd ${version} from the official releases at github.com/clangd/clangd (Apache-2.0 WITH LLVM-exception), checks its SHA-256 and keeps it in Forge's data folder.`,
      detail: `https://github.com/clangd/clangd/releases/tag/${version}`,
      question: "Download it now?",
      okLabel: "Download",
    });
    if (!ok) return false;
    let last = "";
    const off = await cppApi.onInstall((e) => {
      if ("message" in e) last = e.message;
    });
    try {
      const info = await cppApi.installClangd();
      showCppBalloon("clangd is installed", info.path ?? last, 5000);
      await refreshCppStatus();
      return true;
    } catch (e) {
      showCppBalloon("Cannot download clangd", String(e).replace(/^Error: /, ""), 10000);
      return false;
    } finally {
      off();
    }
  })().finally(() => (installing = null));
  return installing;
}

/** Offer the download when the project has no clangd (once per session). */
export function offerClangd(): void {
  if (offered) return;
  offered = true;
  showCppBalloon("clangd is not installed", "Code completion, navigation and clang-tidy checks for C and C++ need clangd.", 15000, [
    { label: "Download clangd…", run: () => void installClangd() },
  ]);
}
