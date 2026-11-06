/**
 * Browser-mode LightEdit window. The tab model is the Rust one (`forge_window::light_edit`
 * through WebAssembly); the files are demo text kept in memory, nothing is written.
 * `?dialog=light-edit&file=/home/me/notes.md&file=/etc/hosts:3` starts the window as if
 * those files were opened with "Open With → Forge" (a `:line` suffix places the caret).
 */
import { emitMock, registerMock } from "@/ipc";
import { LIGHT_EDIT_CHANGED_EVENT, LIGHT_EDIT_PREFIX, type LightEditSnapshot } from "@/ipc/lightEdit";
import { callWasm, forgeWasm } from "@/wasm";

const DEMO_FILES: Record<string, string> = {
  "/home/me/notes/todo.md": "# Release checklist\n\n- [x] Tag the release\n- [ ] Update the changelog\n- [ ] Announce on the forum\n",
  "/etc/hosts": "127.0.0.1\tlocalhost\n::1\t\tlocalhost\n127.0.1.1\tworkstation\n\n192.168.1.20\tstaging.shop.test\n",
  "/home/me/.config/forge/notes.txt": "Meeting notes\n\n- Ship LightEdit\n- Register Forge as a text editor\n",
  "/home/me/Downloads/nginx.conf":
    "server {\n    listen 80;\n    server_name shop.test;\n    root /var/www/shop/public;\n\n    location / {\n        try_files $uri $uri/ /index.php?$query_string;\n    }\n}\n",
};

const texts = new Map<string, string>();
let tabs: unknown = null;

type Op = { op: "open"; path: string; line?: number | null; column?: number | null } | { op: "close" | "activate"; id: string };

async function apply(ops: Op[]): Promise<LightEditSnapshot> {
  const wasm = await forgeWasm();
  const out = await callWasm<{ tabs: unknown; snapshot: LightEditSnapshot }>(wasm.light_edit_apply, { tabs, ops });
  tabs = out.tabs;
  return out.snapshot;
}

/** `/etc/hosts:3` → path and line. */
function parseFileParam(value: string): Op {
  const m = /^(.*?):(\d+)$/.exec(value);
  return m ? { op: "open", path: m[1], line: Number(m[2]) } : { op: "open", path: value };
}

let seeded: Promise<LightEditSnapshot> | null = null;
function seed(): Promise<LightEditSnapshot> {
  seeded ??= apply(
    typeof location === "undefined" ? [] : new URLSearchParams(location.search).getAll("file").map(parseFileParam),
  );
  return seeded;
}

const pathOf = (id: string) => id.slice(LIGHT_EDIT_PREFIX.length);

registerMock("light_edit_state", () => seed().then(() => apply([])));
registerMock("light_edit_open", async ({ paths }) => {
  await seed();
  return apply((paths as string[]).map((path) => ({ op: "open", path })));
});
registerMock("light_edit_close", async ({ id }) => {
  await seed();
  return apply([{ op: "close", id: String(id) }]);
});
registerMock("light_edit_activate", async ({ id }) => {
  await seed();
  return apply([{ op: "activate", id: String(id) }]);
});
registerMock("light_edit_read", ({ id }) => {
  const path = pathOf(String(id));
  return texts.get(path) ?? DEMO_FILES[path] ?? `Demo content of ${path}\n`;
});
registerMock("light_edit_write", ({ id, text }) => {
  texts.set(pathOf(String(id)), String(text));
});
registerMock("light_edit_open_project", () => undefined);

/** Simulate a later "Open With → Forge" while the window is open (tests, screenshots). */
export async function mockLightEditLaunch(path: string): Promise<void> {
  await seed();
  emitMock(LIGHT_EDIT_CHANGED_EVENT, await apply([{ op: "open", path }]));
}
