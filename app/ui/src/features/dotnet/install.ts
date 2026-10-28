/**
 * Installs DotNetForge starts only with the user's consent: the Roslyn language server
 * (Microsoft's public vs-impl NuGet feed, pinned version, SHA-256 checked) and netcoredbg
 * (Samsung's GitHub release, SHA-256 checked), both into Forge's data folder, and the .NET
 * SDK through the platform's package manager in the Terminal (or the download page).
 */
import { dotnetApi, type InstallOption } from "@/ipc/dotnet";
import { confirmDialog } from "@/features/browserCompanion/ConfirmDialog";
import { setChooserDialog } from "@/features/git/overlays";
import { openTerminalWith } from "@/features/terminal";
import { dotnetStatus, errorText, refreshDotnetStatus, showDotnetBalloon } from "./state";

const megabytes = (n?: number | null) => (n ? `${Math.round(n / 1e6)} MB` : "");

export async function installRoslyn(): Promise<boolean> {
  const s = dotnetStatus() ?? (await refreshDotnetStatus());
  const ok = await confirmDialog({
    title: "Download the Roslyn Language Server",
    message: `Code insight for C# comes from Microsoft.CodeAnalysis.LanguageServer (MIT), the server of VS Code's C# extension. Forge downloads version ${s?.roslynVersion ?? ""}${s?.roslynSize ? ` (${megabytes(s.roslynSize)})` : ""} for this platform from Microsoft's public vs-impl NuGet feed, checks its SHA-256 and keeps it in Forge's data folder. It runs on your .NET runtime (9 or newer).`,
    detail: "https://pkgs.dev.azure.com/azure-public/vside/_packaging/vs-impl/nuget/v3/index.json",
    question: "Download it now?",
    okLabel: "Download",
  });
  if (!ok) return false;
  showDotnetBalloon("Downloading the Roslyn language server…", "", 4000);
  try {
    const version = await dotnetApi.installRoslyn();
    await refreshDotnetStatus(true);
    showDotnetBalloon("The Roslyn language server is ready", `Version ${version}`, 5000);
    return true;
  } catch (e) {
    showDotnetBalloon("Cannot download the Roslyn language server", errorText(e), 10000);
    return false;
  }
}

export async function installNetcoredbg(): Promise<boolean> {
  const s = dotnetStatus() ?? (await refreshDotnetStatus());
  const ok = await confirmDialog({
    title: "Download netcoredbg",
    message: `Debugging .NET programs needs netcoredbg (MIT, Samsung). Forge downloads release ${s?.netcoredbgVersion ?? ""} for this platform from GitHub, checks the SHA-256 GitHub publishes for it and keeps it in Forge's data folder.`,
    detail: "https://github.com/Samsung/netcoredbg/releases",
    question: "Download it now?",
    okLabel: "Download",
  });
  if (!ok) return false;
  showDotnetBalloon("Downloading netcoredbg…", "", 4000);
  try {
    const version = await dotnetApi.installNetcoredbg();
    await refreshDotnetStatus(true);
    showDotnetBalloon("netcoredbg is ready", `Version ${version}`, 5000);
    return true;
  } catch (e) {
    showDotnetBalloon("Cannot download netcoredbg", errorText(e), 10000);
    return false;
  }
}

function runInstall(o: InstallOption): void {
  if (o.url) {
    window.open(o.url, "_blank", "noopener");
    return;
  }
  const [program, ...args] = (o.command ?? "").split(/\s+/).filter(Boolean);
  if (!program) return;
  openTerminalWith({ title: `Install .NET SDK (${o.label})`, program, args, local: true });
}

/** Install .NET SDK…: the platform's package managers (in the Terminal) or the download page. */
export async function installSdk(): Promise<void> {
  const s = dotnetStatus() ?? (await refreshDotnetStatus());
  const options = s?.install ?? [];
  setChooserDialog({
    title: "Install .NET SDK",
    items: options.map((o, i) => ({ id: String(i), label: o.label, detail: o.command || o.url })),
    empty: "Download the SDK from https://dotnet.microsoft.com/download",
    okLabel: "Install",
    onOk: (item) => {
      const o = options[Number(item.id)];
      if (o) runInstall(o);
    },
  });
}
