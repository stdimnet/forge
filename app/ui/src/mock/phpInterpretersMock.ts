/**
 * Browser-mode PHP CLI interpreters: detection lists demo executables, Reload parses canned
 * `php -v` / `php --ini` / `php -m` output with the Rust parsers (WebAssembly), and the
 * automatic path mappings come from the same Rust code as in the desktop app. Nothing runs.
 */
import { registerMock } from "@/ipc";
import type { InterpreterInfo, PhpInterpreter } from "@/features/php/interpreters";
import { callWasm, forgeWasm } from "@/wasm";
import { demoProjectName } from "./demoProject";

/** The demo project's root as the mocks show it. */
export const DEMO_ROOT = `~/Projects/${demoProjectName}`;

/** The demo compose file's PHP service mounts the project at /var/www/html. */
export const DEMO_COMPOSE_VOLUMES: Record<string, string[]> = {
  app: [".:/var/www/html", "./storage/logs:/var/log/shop"],
  mysql: ["db-data:/var/lib/mysql"],
  redis: [],
};

const DETECTED = [
  { path: "/usr/bin/php", version: "8.3.12" },
  { path: "/usr/bin/php8.2", version: "8.2.27" },
  { path: "/opt/homebrew/opt/php@8.4/bin/php", version: "8.4.3" },
];

/** The version an interpreter would report: from its image tag, path or name. */
function versionOf(i: PhpInterpreter): string {
  const hint = `${i.image} ${i.path} ${i.name}`;
  const found = DETECTED.find((d) => d.path === i.path.trim());
  if (found) return found.version;
  const m = /(\d)\.(\d)/.exec(hint);
  if (m) return `${m[1]}.${m[2]}.${m[1] === "8" && m[2] === "4" ? "3" : "12"}`;
  return i.kind === "local" ? "8.3.12" : "8.3.14";
}

function probeOutputs(i: PhpInterpreter) {
  const version = versionOf(i);
  const minor = version.split(".").slice(0, 2).join(".");
  const xdebug = i.kind !== "ssh";
  const ini = i.kind === "local" ? `/etc/php/${minor}/cli` : "/usr/local/etc/php";
  return {
    phpV:
      `PHP ${version} (cli) (built: Oct  1 2025 10:12:44) (NTS)\nCopyright (c) The PHP Group\nZend Engine v4.${minor.split(".")[1]}.0, Copyright (c) Zend Technologies\n` +
      `    with Zend OPcache v${version}, Copyright (c), by Zend Technologies\n` +
      (xdebug ? "    with Xdebug v3.4.1, Copyright (c) 2002-2025, by Derick Rethans\n" : ""),
    phpIni:
      `Configuration File (php.ini) Path: ${ini}\nLoaded Configuration File:         ${ini}/php.ini\n` +
      `Scan for additional .ini files in: ${ini}/conf.d\nAdditional .ini files parsed:      ${ini}/conf.d/10-opcache.ini,\n${ini}/conf.d/20-pdo_mysql.ini${xdebug ? `,\n${ini}/conf.d/20-xdebug.ini` : ""}\n`,
    phpM: `[PHP Modules]\nCore\nctype\ncurl\ndate\ndom\nfileinfo\nfilter\nhash\niconv\njson\nmbstring\nopenssl\npcre\nPDO\npdo_mysql\nreadline\nsession\nSPL\nstandard\ntokenizer\nxml\nzip\n${xdebug ? "xdebug\n" : ""}\n[Zend Modules]\n${xdebug ? "Xdebug\n" : ""}Zend OPcache\n`,
  };
}

const interp = (args: Record<string, unknown>) => args.interpreter as PhpInterpreter;

registerMock("php_interpreters_detect", () => DETECTED.map((d) => ({ ...d })));

registerMock("php_interpreter_info", async (args) => {
  const i = interp(args);
  await new Promise((r) => setTimeout(r, 250));
  if (i.kind === "docker" && !i.image.trim()) throw new Error("Image name is not specified");
  if (i.kind === "ssh" && !i.sshConfig) throw new Error("SSH configuration is not specified");
  return callWasm<InterpreterInfo>((await forgeWasm()).php_interpreter_parse_info, probeOutputs(i));
});

registerMock("php_interpreter_phpinfo", (args) => {
  const i = interp(args);
  const out = probeOutputs(i);
  const version = versionOf(i);
  return [
    "phpinfo()",
    `PHP Version => ${version}`,
    "",
    `System => Linux ${i.kind === "local" ? "workstation" : "a1b2c3d4e5f6"} 6.8.0 x86_64`,
    "Server API => Command Line Interface",
    "Virtual Directory Support => disabled",
    out.phpIni.split("\n").find((l) => l.startsWith("Loaded"))?.replace(/:\s+/, " => ") ?? "",
    "PHP API => 20230831",
    "Debug Build => no",
    "Thread Safety => disabled",
    "",
    "Core",
    "",
    "PHP Version => " + version,
    "",
    "Directive => Local Value => Master Value",
    "display_errors => STDOUT => STDOUT",
    "error_reporting => 32767 => 32767",
    "memory_limit => 512M => 512M",
    ...i.options.map((o) => `${o.name} => ${o.value} => ${o.value}`),
    "",
    "xdebug",
    "",
    "Version => 3.4.1",
    "xdebug.mode => develop,debug => develop,debug",
    "xdebug.client_port => 9003 => 9003",
  ].join("\n");
});

registerMock("php_interpreter_mappings", async (args) => {
  const i = interp(args);
  const wasm = await forgeWasm();
  return callWasm(wasm.php_interpreter_automatic_mappings, {
    interpreter: i,
    root: DEMO_ROOT,
    composeDir: DEMO_ROOT,
    volumes: DEMO_COMPOSE_VOLUMES[i.service.trim()] ?? [],
  });
});

registerMock("php_interpreter_compose_services", () => Object.keys(DEMO_COMPOSE_VOLUMES));

registerMock("php_interpreter_wsl_distributions", () => ["Ubuntu-24.04", "Debian"]);
