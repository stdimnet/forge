/**
 * Browser-mode terminal: a tiny fake shell with line editing and canned answers for `ls`,
 * `php -v`, `git status`, `pwd`, `echo`, `clear` and a few more. SSH sessions (`options.ssh`)
 * greet like an Ubuntu host and prompt as the configuration's account. `claude` (and
 * `replay <name>`) replays a recorded TUI session from `terminalReplays.ts`; Ctrl+C ends it.
 * It never touches files.
 */
import { emitMock, invoke, registerMock } from "@/ipc";
import { mockSafeMode } from "./trustMock";
import type { RunningCommand, TerminalEvent, TerminalLayout, TerminalOpenOptions } from "@/ipc/terminal";
import { mockSettingValue } from "./settingsMock";
import { mockOpenRoot, onMockProjectClosing, registerMockObjections } from "./lifecycleHooks";
import { demoProjectName } from "./demoProject";
import { initialLogs } from "./servicesDemoData";
import { mockContainerFor } from "./servicesMock";
import { currentRemote } from "./remoteMock";
import { demoStatus } from "./demoStatus";
import { REPLAYS, replayExit, replayFrames, type ReplayName } from "./terminalReplays";

/** Prompt user and project folder: the remote host's for a remote project. */
const who = () => (currentRemote() ? `${currentRemote()!.user}@shop-web-01` : "dev@forge");
const projectDir = () => currentRemote()?.path ?? localRoot() ?? `~/Projects/${demoProjectName}`;
/** The open local project (a Code With Me guest's `cwm://…` project is the host's demo). */
const localRoot = () => {
  const root = mockOpenRoot();
  return root && /^[~/]/.test(root) ? root : null;
};

interface FakeShell {
  line: string;
  cwd: string;
  history: string[];
  historyPos: number;
  /** A shell inside a container or pod (`docker exec -it`, `kubectl exec -it`). */
  container?: { host: string; service: string };
  /** An SSH session (Tools | Start SSH Session…) to this account. */
  ssh?: { user: string; host: string };
  /** `docker attach`: output only, no prompt. */
  attached?: boolean;
  /** Timer of a running replay; input other than Ctrl+C is ignored meanwhile. */
  replay?: ReturnType<typeof setTimeout>;
  /** A program in the foreground (`sleep 100`) until it ends or Ctrl+C. */
  busy?: { process: string; timer: ReturnType<typeof setTimeout> };
}

const shells = new Map<string, FakeShell>();
let seq = 0;

const C = {
  reset: "\x1b[0m",
  bold: "\x1b[1m",
  dim: "\x1b[2m",
  red: "\x1b[31m",
  green: "\x1b[32m",
  yellow: "\x1b[33m",
  blue: "\x1b[34m",
  magenta: "\x1b[35m",
  cyan: "\x1b[36m",
  gray: "\x1b[90m",
};

const emit = (id: string, e: TerminalEvent) => emitMock(`terminal://${id}`, e);
/** What each session printed (the last 64 KB), for `terminal_attach`. */
const printed = new Map<string, string>();
const out = (id: string, data: string) => {
  const text = data.replace(/\n/g, "\r\n");
  printed.set(id, ((printed.get(id) ?? "") + text).slice(-64 * 1024));
  emit(id, { type: "output", data: text });
};

const homePath = (cwd: string) => cwd.replace(/^~/, "/home/dev");

/** OSC 7 (working directory) and OSC 133;A (prompt start), like shells with integration. */
const promptMarks = (s: FakeShell) => `\x1b]7;file://forge${encodeURI(homePath(s.cwd))}\x07\x1b]133;A\x07`;

const prompt = (s: FakeShell) =>
  s.attached
    ? ""
    : s.container
      ? `root@${s.container.host}:${s.cwd}# `
      : s.ssh
        ? `${C.green}${C.bold}${s.ssh.user}@${s.ssh.host.split(".")[0]}${C.reset}:${C.blue}${C.bold}${s.cwd}${C.reset}$ `
      : promptMarks(s) + `${C.green}${C.bold}${who()}${C.reset} ${C.blue}${C.bold}${s.cwd}${C.reset} ${C.magenta}(${demoStatus().branch})${C.reset} $ `;

const LS_ROOT = [
  ["app", "dir"],
  ["database", "dir"],
  ["public", "dir"],
  ["resources", "dir"],
  ["routes", "dir"],
  ["tests", "dir"],
  ["vendor", "dir"],
  ["artisan", "exec"],
  ["composer.json", "file"],
  ["composer.lock", "file"],
  ["docker-compose.yml", "file"],
  ["package.json", "file"],
  ["phpunit.xml", "file"],
  ["README.md", "file"],
] as const;

function ls(): string {
  return (
    LS_ROOT.map(([n, k]) => (k === "dir" ? `${C.blue}${C.bold}${n}${C.reset}` : k === "exec" ? `${C.green}${C.bold}${n}${C.reset}` : n)).join("  ") +
    "\n"
  );
}

const COMMANDS: Record<string, (args: string[], s: FakeShell) => string> = {
  ls: () => ls(),
  ll: () =>
    LS_ROOT.map(([n, k]) => `${k === "dir" ? "drwxr-xr-x" : k === "exec" ? "-rwxr-xr-x" : "-rw-r--r--"}  dev dev  ${k === "dir" ? "4096" : " 812"} Oct 20 09:12 ${k === "dir" ? `${C.blue}${C.bold}${n}${C.reset}` : n}`).join("\n") + "\n",
  pwd: (_a, s) => `${homePath(s.cwd)}\n`,
  echo: (a) => `${a.join(" ")}\n`,
  whoami: () => "dev\n",
  php: (a) => {
    if (a[0] === "-v" || a[0] === "--version")
      return "PHP 8.3.12 (cli) (built: Sep 24 2026 18:08:04) (NTS)\nCopyright (c) The PHP Group\nZend Engine v4.3.12, Copyright (c) Zend Technologies\n    with Zend OPcache v8.3.12, Copyright (c), by Zend Technologies\n    with Xdebug v3.4.1, Copyright (c) 2002-2026, by Derick Rethans\n";
    if (a[0] === "-l" && a[1]) return `No syntax errors detected in ${a[1]}\n`;
    if (a[0] === "artisan" && a[1] === "--version") return "Laravel Framework 12.3.0\n";
    if (a[0] === "artisan") return `\n   ${C.yellow}INFO${C.reset}  Use the Run tool window for long-running artisan commands.\n\n`;
    return "Usage: php [options] [-f] <file> [--] [args...]\n";
  },
  composer: (a) =>
    a[0] === "--version" || a[0] === "-V"
      ? `${C.green}Composer${C.reset} version ${C.yellow}2.8.4${C.reset} 2026-09-30 11:02:17\n`
      : `${C.green}Composer${C.reset} version ${C.yellow}2.8.4${C.reset}\n\n${C.yellow}Usage:${C.reset}\n  command [options] [arguments]\n`,
  git: (a) => {
    if (a[0] === "status")
      return [
        "On branch feature/order-totals",
        "Your branch is up to date with 'origin/feature/order-totals'.",
        "",
        "Changes not staged for commit:",
        '  (use "git add <file>..." to update what will be committed)',
        '  (use "git restore <file>..." to discard changes in working directory)',
        `\t${C.red}modified:   app/Models/User.php${C.reset}`,
        `\t${C.red}modified:   routes/web.php${C.reset}`,
        "",
        "Untracked files:",
        '  (use "git add <file>..." to include in what will be committed)',
        `\t${C.red}tests/Unit/CheckoutTest.php${C.reset}`,
        "",
        'no changes added to commit (use "git add" and/or "git commit -a")',
        "",
      ].join("\n");
    if (a[0] === "branch") return `  main\n* ${C.green}feature/order-totals${C.reset}\n`;
    if (a[0] === "log")
      return `${C.yellow}3f9c2a1${C.reset} (${C.cyan}HEAD -> ${C.green}feature/order-totals${C.reset}) Add order invoices\n${C.yellow}b81e0d4${C.reset} Normalize customer e-mails\n`;
    return "usage: git [-v | --version] [-h | --help] <command> [<args>]\n";
  },
  grep: (a) =>
    a.includes("-n") || a.includes("-rn")
      ? `${C.magenta}app/Models/User.php${C.reset}${C.cyan}:${C.reset}${C.green}33${C.reset}${C.cyan}:${C.reset}    public function orders(): HasMany\n${C.magenta}app/Http/Controllers/OrderController.php${C.reset}${C.cyan}:${C.reset}${C.green}12${C.reset}${C.cyan}:${C.reset}        $orders = $request->user()->orders;\n`
      : "",
  help: () =>
    `This is Forge's browser-mode terminal. Try ${C.bold}ls${C.reset}, ${C.bold}php -v${C.reset}, ${C.bold}git status${C.reset}, ${C.bold}grep -rn orders app${C.reset}, ${C.bold}claude${C.reset} (a recorded session), ${C.bold}clear${C.reset}.\n`,
};

const CONTAINER_COMMANDS: Record<string, (args: string[], s: FakeShell) => string> = {
  ls: (_a, s) =>
    s.cwd === "/var/www/html"
      ? `${C.blue}${C.bold}app${C.reset}  artisan  ${C.blue}${C.bold}bootstrap${C.reset}  composer.json  ${C.blue}${C.bold}config${C.reset}  ${C.blue}${C.bold}database${C.reset}  ${C.blue}${C.bold}public${C.reset}  ${C.blue}${C.bold}resources${C.reset}  ${C.blue}${C.bold}routes${C.reset}  ${C.blue}${C.bold}storage${C.reset}  ${C.blue}${C.bold}vendor${C.reset}\n`
      : `${C.blue}${C.bold}bin${C.reset}  ${C.blue}${C.bold}data${C.reset}  ${C.blue}${C.bold}etc${C.reset}  ${C.blue}${C.bold}usr${C.reset}  ${C.blue}${C.bold}var${C.reset}\n`,
  pwd: (_a, s) => `${s.cwd}\n`,
  hostname: (_a, s) => `${s.container!.host}\n`,
  whoami: () => "root\n",
  php: (a) => (a[0] === "-v" ? "PHP 8.3.12 (cli) (built: Oct  1 2026 21:30:11) (NTS)\nCopyright (c) The PHP Group\nZend Engine v4.3.12, Copyright (c) Zend Technologies\n" : COMMANDS.php(a, {} as FakeShell)),
  env: (_a, s) => (s.container!.service === "app" ? "HOSTNAME=4f2a9c1be8d3\nDB_HOST=mysql\nXDEBUG_MODE=debug\nPHP_VERSION=8.3.12\nHOME=/root\n" : "HOME=/root\n"),
  ps: () => "PID   USER     TIME  COMMAND\n    1 root      0:04 php artisan serve --host=0.0.0.0\n   42 root      0:00 sh\n   48 root      0:00 ps\n",
  echo: (a) => `${a.join(" ")}\n`,
  help: () => "Shell inside the container (browser-mode demo). Try ls, php -v, env, ps, exit.\n",
};

/** Replay a recorded stream; the fake shell stays busy until Ctrl+C, like a running TUI. */
function startReplay(id: string, s: FakeShell, name: ReplayName): void {
  const frames = replayFrames(name, homePath(s.cwd));
  let i = 0;
  const next = () => {
    const f = frames[i++];
    if (!f || !shells.has(id)) return;
    s.replay = setTimeout(() => {
      out(id, f.data);
      next();
    }, f.delay);
  };
  // Keep the shell busy even after the last frame: the TUI keeps running.
  s.replay = setTimeout(next, 0);
}

function stopReplay(id: string, s: FakeShell): void {
  clearTimeout(s.replay);
  s.replay = undefined;
  out(id, replayExit());
}

/** Recording for `claude [--thinking | --error]` or `replay <name>`: null if unknown, undefined if not a replay. */
function replayFor(cmd: string, args: string[]): ReplayName | null | undefined {
  if (cmd === "claude") return args.includes("--error") ? "claude-error" : args.includes("--thinking") ? "claude-thinking" : "claude";
  if (cmd === "replay") return REPLAYS.includes(args[0] as ReplayName) ? (args[0] as ReplayName) : null;
  return undefined;
}

function run(id: string, s: FakeShell, line: string): void {
  const [cmd, ...args] = line.trim().split(/\s+/);
  if (!cmd) return;
  if (s.container && cmd !== "exit" && cmd !== "clear" && cmd !== "cd") {
    const handler = CONTAINER_COMMANDS[cmd];
    out(id, handler ? handler(args, s) : `sh: ${cmd}: not found\n`);
    return;
  }
  const replay = replayFor(cmd, args);
  if (replay) {
    startReplay(id, s, replay);
    return;
  }
  if (replay === null) {
    out(id, `replay: unknown recording. Available: ${REPLAYS.join(", ")}\n`);
    return;
  }
  if (cmd === "clear") {
    out(id, "\x1b[2J\x1b[3J\x1b[H");
    return;
  }
  if (cmd === "sleep") {
    const seconds = Number(args[0] ?? "1");
    if (!Number.isFinite(seconds) || seconds < 0) {
      out(id, `sleep: invalid time interval '${args[0]}'\n`);
      return;
    }
    // The terminal's foreground process until it ends (terminal_running reports it).
    s.busy = {
      process: "sleep",
      timer: setTimeout(() => {
        s.busy = undefined;
        if (shells.has(id)) out(id, prompt(s));
      }, seconds * 1000),
    };
    return;
  }
  if (cmd === "exit") {
    emit(id, { type: "exited", code: 0 });
    shells.delete(id);
    return;
  }
  if (cmd === "cd") {
    const target = args[0] ?? "";
    if (!target || target === "~") s.cwd = s.ssh ? "~" : projectDir();
    else if (target === "..") s.cwd = s.cwd.replace(/\/[^/]+$/, "") || "~";
    else s.cwd = `${s.cwd}/${target.replace(/\/$/, "")}`;
    return;
  }
  const handler = COMMANDS[cmd];
  out(id, handler ? handler(args, s) : `bash: ${cmd}: command not found\n`);
}

/** The account of an SSH session: the saved configuration, or the one of a deployment server. */
async function sshAccount(o: TerminalOpenOptions): Promise<{ user: string; host: string }> {
  const target = o.ssh!;
  let configId = target.configId;
  if (target.serverId) {
    const config = await invoke<{ servers: { id: string; name: string; kind: string; sshConfig: string }[] }>("deploy_config");
    const server = config.servers.find((x) => x.id === target.serverId);
    if (!server) throw new Error(`The deployment server ${target.serverId} does not exist`);
    if (mockSafeMode() && !target.confirmed) {
      throw new Error(`The deployment server ${server.name} is defined in the files of an untrusted project. Confirm the SSH session to it first, or trust the project.`);
    }
    if (server.kind !== "sftp" || !server.sshConfig) throw new Error(`${server.name} is not an SFTP server: SSH sessions need an SSH configuration`);
    configId = server.sshConfig;
  }
  const configs = await invoke<{ id: string; host: string; user: string; auth: string; hasSecret: boolean }[]>("remote_ssh_configs");
  const c = configs.find((x) => x.id === configId);
  if (!c) throw new Error(`The SSH configuration ${configId} does not exist anymore. Select another one.`);
  if (c.auth === "password" && !c.hasSecret && !target.secret) throw new Error(`Authentication failed for ${c.user}@${c.host}`);
  return { user: c.user, host: c.host };
}

const SSH_MOTD = (host: string) =>
  `Welcome to Ubuntu 24.04.1 LTS (GNU/Linux 6.8.0-45-generic x86_64)\n\n` +
  `${C.dim} * Documentation:  https://help.ubuntu.com\n * Management:     https://landscape.canonical.com${C.reset}\n\n` +
  `  System load:  0.08               Processes:             142\n` +
  `  Usage of /:   41.7% of 77.35GB   Users logged in:       1\n` +
  `  Memory usage: 38%                IPv4 address for eth0: 10.0.4.21\n\n` +
  `Last login: Mon Oct 26 09:12:44 2026 from 192.168.1.20 on ${host}\n`;

registerMock("terminal_open", async ({ options }) => {
  const o = options as TerminalOpenOptions;
  const id = o.id ?? `mock-term-${++seq}`;
  if (o.ssh) {
    const account = await sshAccount(o);
    await new Promise((r) => setTimeout(r, 250));
    const s: FakeShell = { line: "", cwd: "~", history: [], historyPos: 0, ssh: account };
    shells.set(id, s);
    setTimeout(() => out(id, SSH_MOTD(account.host) + prompt(s)), 30);
    return { id, shell: `${account.user}@${account.host}`, cwd: "~" };
  }
  // An absolute start directory (a restored tab's last directory, OSC 7 spelling) as the prompt shows it.
  const start = o.cwd?.startsWith("/") ? o.cwd.replace(/^\/home\/dev(?=\/|$)/, "~") : o.cwd ? `${projectDir()}/${o.cwd}` : projectDir();
  const s: FakeShell = { line: "", cwd: start, history: [], historyPos: 0 };
  // `docker exec -it <id> sh …`, `docker attach <id>`, `kubectl … exec -it <pod> …`.
  const args = o.args ?? [];
  if (o.shell === "docker" || o.shell === "kubectl") {
    const execAt = args.indexOf("exec");
    // exec -it <id> … / attach --sig-proxy=false <id> / kubectl … exec -it <pod> …
    const ref = o.shell === "kubectl" ? args[execAt + 2] : args[2];
    const c = o.shell === "docker" ? mockContainerFor(ref ?? "") : null;
    const host = o.shell === "kubectl" ? (ref ?? "pod") : (c?.name ? (ref ?? "").slice(0, 12) : "container");
    s.container = { host, service: c?.service ?? "app" };
    s.cwd = c?.service === "app" || o.shell === "kubectl" ? "/var/www/html" : "/data";
    if (args[0] === "attach") {
      s.attached = true;
      shells.set(id, s);
      setTimeout(() => out(id, initialLogs(c?.service ?? "app").slice(-6).join("")), 30);
      return { id, shell: o.shell, cwd: s.cwd };
    }
  }
  shells.set(id, s);
  setTimeout(() => out(id, prompt(s)), 30);
  return { id, shell: o.shell === "docker" || o.shell === "kubectl" ? o.shell : "/bin/bash", cwd: s.cwd };
});

registerMock("terminal_write", ({ id, data }) => {
  const key = String(id);
  const s = shells.get(key);
  if (!s) return;
  if (s.attached) {
    // Attached consoles only forward Ctrl+C (detach keys) in the demo.
    if (String(data).includes("\x03")) out(key, "^C\n");
    return;
  }
  for (const ch of String(data).match(/\x1b\[[A-D]|[\s\S]/g) ?? []) {
    if (s.busy) {
      if (ch === "\x03") {
        clearTimeout(s.busy.timer);
        s.busy = undefined;
        out(key, "^C\n" + prompt(s));
      }
      continue;
    }
    if (s.replay !== undefined) {
      if (ch === "\x03") {
        stopReplay(key, s);
        out(key, prompt(s));
      }
      continue;
    }
    if (ch === "\r") {
      out(key, "\n");
      if (s.line.trim()) s.history.push(s.line);
      s.historyPos = s.history.length;
      run(key, s, s.line);
      s.line = "";
      if (shells.has(key) && s.replay === undefined && !s.busy) out(key, prompt(s));
    } else if (ch === "\x7f" || ch === "\b") {
      if (s.line) {
        s.line = s.line.slice(0, -1);
        out(key, "\b \b");
      }
    } else if (ch === "\x03") {
      out(key, "^C\n" + prompt(s));
      s.line = "";
    } else if (ch === "\x0c") {
      out(key, "\x1b[2J\x1b[H" + prompt(s) + s.line);
    } else if (ch === "\x1b[A" || ch === "\x1b[B") {
      if (!s.history.length) continue;
      s.historyPos = Math.max(0, Math.min(s.history.length, s.historyPos + (ch === "\x1b[A" ? -1 : 1)));
      const next = s.history[s.historyPos] ?? "";
      out(key, "\b \b".repeat(s.line.length) + next);
      s.line = next;
    } else if (ch >= " " && ch.length === 1) {
      s.line += ch;
      out(key, ch);
    }
  }
});

// ---- Terminals belong to a project (lifecycle) -----------------------------------------

/** What runs in the foreground of each fake shell, like `TerminalState::running` in Rust. */
const running = (): RunningCommand[] =>
  [...shells.entries()]
    .flatMap(([id, s]) => (s.busy ? [{ id, process: s.busy.process }] : s.replay !== undefined ? [{ id, process: "claude" }] : []))
    .sort((a, b) => a.id.localeCompare(b.id));

registerMock("terminal_running", () => running());
registerMockObjections(() => {
  const list = running();
  if (!list.length) return [];
  const message = list.length === 1 ? "A command is still running in the terminal." : `${list.length} commands are still running in the terminal.`;
  return [{ source: "terminals", kind: "terminal.running", message, items: list.map((r) => r.process), targets: list.map((r) => r.id) }];
});

/** Saved layouts and output per project root (Rust keeps them in the state and cache dirs). */
const layouts = new Map<string, TerminalLayout>();
const histories = new Map<string, Map<string, string>>();

// The project closes: keep each shell's output (setting `terminal.restoreScrollback`) and end
// the shells without reporting their exit.
onMockProjectClosing((project) => {
  const keep = mockSettingValue("terminal.restoreScrollback") !== false;
  histories.set(project.root, keep ? new Map(printed) : new Map());
  for (const s of shells.values()) {
    clearTimeout(s.replay);
    if (s.busy) clearTimeout(s.busy.timer);
  }
  shells.clear();
  printed.clear();
});

registerMock("terminal_layout_save", ({ root, layout }) => {
  const l = layout as TerminalLayout;
  if (l.tabs.length) layouts.set(String(root), structuredClone({ ...l, tabs: l.tabs.map(({ scrollback: _s, ...t }) => t) }));
  else layouts.delete(String(root));
});
registerMock("terminal_layout_load", ({ root }): TerminalLayout | null => {
  const saved = layouts.get(String(root));
  if (!saved) return null;
  const keep = mockSettingValue("terminal.restoreScrollback") !== false;
  const history = histories.get(String(root));
  return { ...saved, tabs: saved.tabs.map((t) => ({ ...t, ...(keep && history?.get(t.id) ? { scrollback: history.get(t.id) } : {}) })) };
});

registerMock("terminal_resize", () => undefined);
registerMock("terminal_close", ({ id }) => {
  clearTimeout(shells.get(String(id))?.replay);
  shells.delete(String(id));
  printed.delete(String(id));
});

/**
 * Start a fake shell for `id` without a tab (Code With Me: a guest's terminal on the host, or
 * the host's shared terminal in a guest's window). `before` is printed first, then the prompt.
 */
export function startMockShell(id: string, before = ""): void {
  const s: FakeShell = { line: "", cwd: projectDir(), history: [], historyPos: 0 };
  shells.set(id, s);
  out(id, before + prompt(s));
}

/** Type into a fake shell as if keys arrived (a scripted Code With Me guest). */
export const typeIntoMockShell = (id: string, data: string) => invoke("terminal_write", { id, data });

registerMock("terminal_attach", ({ id }) => {
  const key = String(id);
  if (!shells.has(key)) throw new Error(`no terminal session ${key}`);
  return { scrollback: printed.get(key) ?? "", cols: 100, rows: 24 };
});
