/**
 * Browser-mode terminal: a tiny fake shell with line editing and canned answers for `ls`,
 * `php -v`, `git status`, `pwd`, `echo`, `clear` and a few more. It never touches files.
 */
import { emitMock, registerMock } from "@/ipc";
import type { TerminalEvent, TerminalOpenOptions } from "@/ipc/terminal";
import { demoProjectName } from "./demoProject";
import { initialLogs } from "./servicesDemoData";
import { mockContainerFor } from "./servicesMock";
import { currentRemote } from "./remoteMock";

/** Prompt user and project folder: the remote host's for a remote project. */
const who = () => (currentRemote() ? `${currentRemote()!.user}@shop-web-01` : "dev@forge");
const projectDir = () => currentRemote()?.path ?? `~/Projects/${demoProjectName}`;

interface FakeShell {
  line: string;
  cwd: string;
  history: string[];
  historyPos: number;
  /** A shell inside a container or pod (`docker exec -it`, `kubectl exec -it`). */
  container?: { host: string; service: string };
  /** `docker attach`: output only, no prompt. */
  attached?: boolean;
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
const out = (id: string, data: string) => emit(id, { type: "output", data: data.replace(/\n/g, "\r\n") });

const prompt = (s: FakeShell) =>
  s.attached ? "" : s.container ? `root@${s.container.host}:${s.cwd}# ` : `${C.green}${C.bold}${who()}${C.reset} ${C.blue}${C.bold}${s.cwd}${C.reset} ${C.magenta}(feature/order-totals)${C.reset} $ `;

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
  pwd: (_a, s) => `${s.cwd.replace(/^~/, "/home/dev")}\n`,
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
    `This is Forge's browser-mode terminal. Try ${C.bold}ls${C.reset}, ${C.bold}php -v${C.reset}, ${C.bold}git status${C.reset}, ${C.bold}grep -rn orders app${C.reset}, ${C.bold}clear${C.reset}.\n`,
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

function run(id: string, s: FakeShell, line: string): void {
  const [cmd, ...args] = line.trim().split(/\s+/);
  if (!cmd) return;
  if (s.container && cmd !== "exit" && cmd !== "clear" && cmd !== "cd") {
    const handler = CONTAINER_COMMANDS[cmd];
    out(id, handler ? handler(args, s) : `sh: ${cmd}: not found\n`);
    return;
  }
  if (cmd === "clear") {
    out(id, "\x1b[2J\x1b[3J\x1b[H");
    return;
  }
  if (cmd === "exit") {
    emit(id, { type: "exited", code: 0 });
    shells.delete(id);
    return;
  }
  if (cmd === "cd") {
    const target = args[0] ?? "";
    if (!target || target === "~") s.cwd = projectDir();
    else if (target === "..") s.cwd = s.cwd.replace(/\/[^/]+$/, "") || "~";
    else s.cwd = `${s.cwd}/${target.replace(/\/$/, "")}`;
    return;
  }
  const handler = COMMANDS[cmd];
  out(id, handler ? handler(args, s) : `bash: ${cmd}: command not found\n`);
}

registerMock("terminal_open", ({ options }) => {
  const o = options as TerminalOpenOptions;
  const id = o.id ?? `mock-term-${++seq}`;
  const s: FakeShell = { line: "", cwd: `${projectDir()}${o.cwd ? `/${o.cwd}` : ""}`, history: [], historyPos: 0 };
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
    if (ch === "\r") {
      out(key, "\n");
      if (s.line.trim()) s.history.push(s.line);
      s.historyPos = s.history.length;
      run(key, s, s.line);
      s.line = "";
      if (shells.has(key)) out(key, prompt(s));
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

registerMock("terminal_resize", () => undefined);
registerMock("terminal_close", ({ id }) => {
  shells.delete(String(id));
});
