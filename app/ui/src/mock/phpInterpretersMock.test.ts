import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { CommandLine, RunConfiguration } from "@/ipc/run";
import {
  decodeInterpreter,
  decodeInterpreters,
  emptyInterpreter,
  encodeInterpreter,
  encodeInterpreters,
  findInterpreter,
  levelForVersion,
  type InterpreterInfo,
  type PathMapping,
  type PhpInterpreter,
} from "@/features/php/interpreters";
import { LANGUAGE_LEVELS } from "@/features/php/settings";
import { saveSettings } from "@/state/settings";
import "./settingsMock";
import "./runMock";
import "./phpInterpretersMock";

const docker: PhpInterpreter = {
  ...emptyInterpreter("docker"),
  id: "d1",
  name: "php:8.2-cli (Docker)",
  image: "php:8.2-cli",
  mappings: [{ local: "", remote: "/app" }],
  options: [{ name: "memory_limit", value: "-1" }],
};

describe("PHP interpreters", () => {
  it("round-trips through the settings tables like the Rust model", () => {
    const table = encodeInterpreter(docker);
    expect(table).toMatchObject({ kind: "docker", image: "php:8.2-cli", mappings: "\t/app", options: "memory_limit=-1" });
    expect(Object.values(table).every((v) => typeof v === "string")).toBe(true);
    expect(decodeInterpreter(table, false)).toEqual(docker);
    const project = { ...docker, id: "p1", name: "Local 8.3", kind: "local" as const, project: true, mappings: [], options: [] };
    const { global, project: projectList } = encodeInterpreters([docker, project]);
    const list = decodeInterpreters(global, projectList);
    expect(list.map((i) => [i.name, i.project])).toEqual([
      ["Local 8.3", true],
      ["php:8.2-cli (Docker)", false],
    ]);
    expect(findInterpreter(list, "p1")?.name).toBe("Local 8.3");
    expect(levelForVersion("8.2.27", LANGUAGE_LEVELS)).toBe("8.2");
    expect(levelForVersion("8.5.0", LANGUAGE_LEVELS)).toBe("8.4");
  });

  it("reloads, derives mappings and lists compose services", async () => {
    const info = await invoke<InterpreterInfo>("php_interpreter_info", { interpreter: docker });
    expect(info.version).toBe("8.2.12");
    expect(info.xdebug).toBe("3.4.1");
    expect(info.extensions).toContain("pdo_mysql");
    await expect(invoke("php_interpreter_info", { interpreter: { ...docker, image: "" } })).rejects.toThrow("Image name");
    const mappings = await invoke<PathMapping[]>("php_interpreter_mappings", {
      interpreter: { ...emptyInterpreter("compose"), service: "app" },
    });
    expect(mappings[0]).toEqual({ local: "/home/demo/Projects/shop", remote: "/var/www/html" });
    expect(await invoke<string[]>("php_interpreter_compose_services", { files: [] })).toContain("app");
  });

  it("runs PHP configurations through the selected interpreter", async () => {
    await saveSettings({ "php.interpreters": encodeInterpreters([docker]).global });
    const cfg: RunConfiguration = { name: "run", type: "php-script", interpreter: docker.name, options: { file: "bin/run.php" } };
    const cmd = await invoke<CommandLine>("run_command_line", { request: { configuration: cfg } });
    expect(cmd.program).toBe("docker");
    expect(cmd.args).toEqual(expect.arrayContaining(["run", "--rm", "php:8.2-cli", "php", "-dmemory_limit=-1", "bin/run.php"]));
    // A path that names no interpreter still runs locally.
    const local = await invoke<CommandLine>("run_command_line", { request: { configuration: { ...cfg, interpreter: "/usr/bin/php8.3" } } });
    expect(local.program).toBe("/usr/bin/php8.3");
    await saveSettings({ "php.interpreters": null });
  });
});
