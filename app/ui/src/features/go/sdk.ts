/**
 * Download Go SDK…: the stable releases of go.dev/dl for this platform, a choice, the
 * user's consent, then the SHA-256-checked download (`go_sdk_download`, progress on
 * `go://install`), which becomes the project's GOROOT.
 */
import { goApi, type GoSdk } from "@/ipc/go";
import { confirmDialog } from "@/features/browserCompanion/ConfirmDialog";
import { setChooserDialog } from "@/features/git/overlays";
import { saveSettings } from "@/state/settings";
import { showGoBalloon } from "./state";

const megabytes = (n: number) => `${(n / 1024 / 1024).toFixed(0)} MB`;

async function download(version: string, size: number, url: string, sha: string): Promise<GoSdk | null> {
  const ok = await confirmDialog({
    title: "Download Go SDK",
    message: `Forge downloads Go ${version} (${megabytes(size)}, BSD-3-Clause) from go.dev, checks its SHA-256 checksum and keeps it in Forge's data folder. The project then uses it as GOROOT.`,
    detail: `${url}\nSHA-256 ${sha}`,
    question: "Download it now?",
    okLabel: "Download",
  });
  if (!ok) return null;
  showGoBalloon(`Downloading Go ${version}…`, "", 4000);
  try {
    const sdk = await goApi.sdkDownload(version);
    await saveSettings({ "go.goroot": sdk.root });
    showGoBalloon(`Go ${sdk.version} is ready`, sdk.root, 5000);
    return sdk;
  } catch (e) {
    showGoBalloon("Cannot download the Go SDK", String(e).replace(/^Error: /, ""), 10000);
    return null;
  }
}

/** Choose a release and download it. */
export async function downloadSdk(): Promise<GoSdk | null> {
  let releases;
  try {
    releases = await goApi.sdkReleases();
  } catch (e) {
    showGoBalloon("Cannot reach go.dev", String(e).replace(/^Error: /, ""), 8000);
    return null;
  }
  return new Promise((resolve) => {
    setChooserDialog({
      title: "Download Go SDK",
      items: releases.map((r) => ({ id: r.version, label: `Go ${r.version}`, detail: `${r.file.filename} · ${megabytes(r.file.size)}` })),
      empty: "No Go release for this platform",
      okLabel: "Download…",
      onOk: async (item) => {
        const r = releases.find((x) => x.version === item.id)!;
        resolve(await download(r.version, r.file.size, `https://go.dev/dl/${r.file.filename}`, r.file.sha256));
      },
    });
  });
}
