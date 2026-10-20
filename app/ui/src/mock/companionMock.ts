/**
 * Browser-mode Browser Companion: a server that pretends to run, one paired Chrome, and a
 * project profile read from the demo project. "Apply" writes go to the in-memory demo files
 * through `fs_write_file`, never to disk.
 */
import { emitMock, invoke, registerMock } from "@/ipc";
import {
  COMPANION_OPEN_EVENT,
  type OpenEvent,
  type PairedClient,
  type PairingCode,
  type ProjectProfile,
} from "@/ipc/companion";
import { demoProjectName } from "./demoProject";

const ROOT = `~/Projects/${demoProjectName}`;
const now = () => Math.floor(Date.now() / 1000);

let pairing: PairingCode | null = null;
let clients: PairedClient[] = [
  {
    id: "c1",
    name: "Chrome 141 on Linux",
    origin: "chrome-extension://kfbmhlkcjgmbaaanpcjmobokpcjbinea",
    createdAt: now() - 3 * 86400,
    lastUsedAt: now() - 120,
  },
];

registerMock("companion_status", () => ({
  server: { running: true, port: 63342, error: null },
  pairing: pairing && pairing.expiresAt > now() ? pairing : null,
  clients,
}));

registerMock("companion_start_pairing", () => {
  pairing = { code: "482913", expiresAt: now() + 300 };
  return pairing;
});

registerMock("companion_cancel_pairing", () => {
  pairing = null;
});

registerMock("companion_revoke_client", ({ id }) => {
  const before = clients.length;
  clients = clients.filter((c) => c.id !== id);
  return clients.length !== before;
});

registerMock("companion_ui_ready", () => []);

/** Same shape the Rust side sends; files under the demo root open directly. */
export function mockOpenEvent(file: string, line?: number, column?: number, source: OpenEvent["source"] = "browser"): OpenEvent {
  const abs = file.startsWith("/") || file.startsWith("~") ? file : `${ROOT}/${file}`;
  const inside = abs.startsWith(`${ROOT}/`);
  // The backend walks up to a folder with .git / composer.json / package.json; the mock
  // takes the folder above `src/` or `app/`.
  const marker = abs.search(/\/(src|app|lib)\//);
  const suggestedRoot = marker > 0 ? abs.slice(0, marker) : abs.slice(0, abs.lastIndexOf("/"));
  return {
    source,
    client: source === "browser" ? clients[0]?.name : null,
    file: abs,
    line: line ?? null,
    column: column ?? null,
    target: inside
      ? { kind: "open", root: ROOT, path: abs.slice(ROOT.length + 1) }
      : { kind: "outside", suggestedRoot, path: abs.slice(suggestedRoot.length + 1) },
  };
}

registerMock("companion_open_link", ({ url }) => {
  const u = new URL(String(url).replace(/^forge:\/\//, "http://forge/"));
  const file = u.searchParams.get("file") ?? u.searchParams.get("path");
  if (!file) throw new Error("file is missing");
  const num = (k: string) => Number(u.searchParams.get(k)) || undefined;
  emitMock(COMPANION_OPEN_EVENT, mockOpenEvent(file, num("line"), num("column"), "link"));
});

async function readDemo(path: string): Promise<string | null> {
  try {
    return await invoke<string>("fs_read_file", { path });
  } catch {
    return null;
  }
}

const packages = (json: string | null, sections: string[]) => {
  if (!json) return [];
  try {
    const v = JSON.parse(json) as Record<string, Record<string, string> | undefined>;
    return [...new Set(sections.flatMap((s) => Object.keys(v[s] ?? {})))].sort();
  } catch {
    return [];
  }
};

registerMock("companion_project_profile", async ({ envKeys, files }) => {
  const env: ProjectProfile["env"] = {};
  for (const file of [".env", ".env.local"]) {
    const content = await readDemo(file);
    if (content === null) continue;
    env[file] = {};
    for (const key of envKeys as string[]) {
      const m = new RegExp(`^\\s*${key}\\s*=\\s*"?([^"\\n]*)"?`, "m").exec(content);
      if (m) env[file][key] = m[1];
    }
  }
  const present: string[] = [];
  for (const f of files as string[]) if ((await readDemo(f)) !== null) present.push(f);
  return {
    composer: packages(await readDemo("composer.json"), ["require", "require-dev"]),
    npm: packages(await readDemo("package.json"), ["dependencies", "devDependencies"]),
    env,
    files: present,
  } satisfies ProjectProfile;
});

registerMock("companion_set_env_var", async ({ file, key, value }) => {
  const current = (await readDemo(String(file))) ?? "";
  const line = `${key}=${/^[\w./:-]+$/.test(String(value)) ? value : JSON.stringify(value)}`;
  const re = new RegExp(`^\\s*${key}\\s*=.*$`, "m");
  const next = re.test(current) ? current.replace(re, line) : `${current.replace(/\n*$/, "\n")}\n${line}\n`;
  await invoke("fs_write_file", { path: file, contents: next });
});

/** Lets screenshot scenarios simulate a click in the browser extension. */
declare global {
  interface Window {
    __forgeCompanionMock?: { open: (file: string, line?: number, column?: number) => void };
  }
}
window.__forgeCompanionMock = {
  open: (file, line, column) => emitMock(COMPANION_OPEN_EVENT, mockOpenEvent(file, line, column)),
};
