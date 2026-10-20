/** Toolbar popup: connection status, pairing, inspect mode, recent files and options. */
import { formatMappings, parseMappings } from "../lib/paths";
import type { BackgroundRequest, OpenResult, RecentEntry, Status } from "../lib/types";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const send = <T>(msg: BackgroundRequest) => chrome.runtime.sendMessage<BackgroundRequest, T>(msg);

const statusEl = $("status");
const statusText = $("status-text");
const pairSection = $("pair");
const codeInput = $<HTMLInputElement>("code");
const pairError = $("pair-error");
const inspect = $<HTMLInputElement>("inspect");
const altClick = $<HTMLInputElement>("alt-click");
const recentList = $("recent");
const recentEmpty = $("recent-empty");
const portInput = $<HTMLInputElement>("port");
const mappingsInput = $<HTMLTextAreaElement>("mappings");

function renderStatus(s: Status): void {
  const paired = !!s.settings.token && s.ping?.paired;
  statusEl.className = `status ${s.connected ? (paired ? "ok" : "warn") : "bad"}`;
  if (!s.connected) statusText.textContent = "Forge is not running";
  else if (!paired) statusText.textContent = `Forge ${s.ping!.version} found · not paired`;
  else {
    const project = s.ping?.projects?.[0]?.name;
    statusText.textContent = `Connected to Forge ${s.ping!.version}${project ? ` · ${project}` : ""}`;
  }
  pairSection.hidden = !s.connected || !!paired;
  inspect.checked = s.inspecting;
  altClick.checked = s.settings.altClick;
  if (document.activeElement !== portInput) portInput.value = s.settings.port ? String(s.settings.port) : "";
  if (document.activeElement !== mappingsInput) mappingsInput.value = formatMappings(s.settings.mappings);
  renderRecent(s.settings.recent);
}

function renderRecent(recent: RecentEntry[]): void {
  recentList.replaceChildren(
    ...recent.slice(0, 6).map((entry) => {
      const li = document.createElement("li");
      li.title = `${entry.file}${entry.line ? `:${entry.line}` : ""}`;
      const name = Object.assign(document.createElement("span"), { className: "recent-name", textContent: entry.label });
      const shortPath = entry.path ?? entry.file.split(/[\\/]/).slice(-3).join("/");
      const path = Object.assign(document.createElement("span"), {
        className: "recent-path",
        textContent: `${shortPath}${entry.line ? `:${entry.line}` : ""}`,
      });
      li.append(name, path);
      li.addEventListener("click", async () => {
        const r = await send<OpenResult>({ type: "openRecent", entry });
        if (r.ok) window.close();
      });
      return li;
    }),
  );
  recentEmpty.hidden = recent.length > 0;
}

async function refresh(): Promise<void> {
  renderStatus(await send<Status>({ type: "status" }));
}

$("pair-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  pairError.hidden = true;
  const r = await send<Status & { ok?: false; error?: string }>({ type: "pair", code: codeInput.value });
  if (r.ok === false) {
    pairError.textContent = r.error ?? "Pairing failed";
    pairError.hidden = false;
    return;
  }
  codeInput.value = "";
  renderStatus(r);
});

codeInput.addEventListener("input", () => {
  const digits = codeInput.value.replace(/\D/g, "").slice(0, 6);
  codeInput.value = digits.length > 3 ? `${digits.slice(0, 3)} ${digits.slice(3)}` : digits;
});

inspect.addEventListener("change", async () => {
  await send({ type: "setInspect", enabled: inspect.checked });
  if (inspect.checked) window.close();
});

altClick.addEventListener("change", () => void send({ type: "saveOptions", altClick: altClick.checked }));

$("save").addEventListener("click", async () => {
  const port = Number.parseInt(portInput.value, 10);
  renderStatus(
    await send<Status>({
      type: "saveOptions",
      port: Number.isFinite(port) && port > 0 && port < 65536 ? port : null,
      mappings: parseMappings(mappingsInput.value),
    }),
  );
});

$("unpair").addEventListener("click", async () => renderStatus(await send<Status>({ type: "unpair" })));

void chrome.commands.getAll().then((commands) => {
  const shortcut = commands.find((c) => c.name === "toggle-inspect")?.shortcut;
  $("shortcut").textContent = shortcut || "no shortcut";
});

void refresh();
