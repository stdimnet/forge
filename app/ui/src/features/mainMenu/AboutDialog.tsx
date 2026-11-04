/** Help | About: product name, version and runtime, like JetBrains' About dialog. */
import { Show, createResource, createSignal, onCleanup } from "solid-js";
import { Hammer } from "lucide-solid";
import { isTauri } from "@/ipc";
import { suspendKeymap } from "@/keymap";
import { version as uiVersion } from "../../../package.json";
import { t } from "@/i18n";
import { writeClipboard } from "@/features/clipboard/system";

const [aboutOpen, setAboutOpen] = createSignal(false);
export const openAbout = () => setAboutOpen(true);

async function appVersion(): Promise<string> {
  if (!isTauri()) return uiVersion;
  try {
    const { getVersion } = await import("@tauri-apps/api/app");
    return await getVersion();
  } catch {
    return uiVersion;
  }
}

/** Lines of the About dialog (also what "Copy and Close" copies). */
export function aboutLines(version: string, desktop: boolean, platform: string): string[] {
  return [
    `Forge ${version}`,
    desktop ? "Desktop app (Tauri 2)" : "Browser mode (mocked backend)",
    "Rust backend, SolidJS UI, CodeMirror 6 editor",
    `Platform: ${platform || "unknown"}`,
  ];
}

function AboutDialog() {
  const release = suspendKeymap();
  onCleanup(release);
  const [version] = createResource(appVersion);
  const lines = () => aboutLines(version() ?? uiVersion, isTauri(), navigator.platform);
  const close = () => setAboutOpen(false);
  const copyAndClose = async () => {
    await writeClipboard(lines().join("\n"));
    close();
  };
  return (
    <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        class="dialog about-dialog"
        role="dialog"
        aria-label={t("tools.mainMenu.aboutForge")}
        tabindex="-1"
        ref={(el) => queueMicrotask(() => el.focus())}
        onKeyDown={(e) => {
          if (e.key !== "Escape" && e.key !== "Enter") return;
          e.preventDefault();
          e.stopPropagation();
          close();
        }}
      >
        <div class="about-body">
          <div class="about-logo">
            <Hammer size={30} />
          </div>
          <div class="about-text">
            <div class="about-name">Forge</div>
            <div class="about-version">{t("tools.mainMenu.version", { value: version() ?? uiVersion })}</div>
            <div class="about-lines">
              <div>{lines()[1]}</div>
              <div>{lines()[2]}</div>
              <div>{lines()[3]}</div>
            </div>
            <div class="about-tagline">{t("tools.mainMenu.aPhpstormStyleIdeFor")}</div>
          </div>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={() => void copyAndClose()}>
            {t("tools.mainMenu.copyAndClose")}
          </button>
          <button class="btn-primary" onClick={close}>
            {t("common.close")}
          </button>
        </div>
      </div>
    </div>
  );
}

export function AboutHost() {
  return (
    <Show when={aboutOpen()}>
      <AboutDialog />
    </Show>
  );
}
