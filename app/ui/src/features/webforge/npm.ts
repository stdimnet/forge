/**
 * Running package.json scripts: a temporary "npm" run configuration per script (reused on
 * the next run, like WebStorm), and Debug with the inspector enabled through NODE_OPTIONS.
 */
import type { RunConfiguration } from "@/ipc/run";
import type { NpmPackage, NpmScript } from "@/ipc/web";
import { registerDebugLauncher } from "@/features/run";
import { useTemporary } from "@/features/run/configStore";
import { debugConfiguration, runConfiguration } from "@/features/run/runStore";
import { getSetting } from "@/state/settings";
import { openFileAt } from "@/editor/views";
import { packages } from "./store";

/** The temporary configuration for a script; names follow WebStorm (`dev`, `ui: build`). */
export function scriptConfiguration(pkg: NpmPackage, script: string): RunConfiguration {
  const root = !pkg.dir;
  return {
    name: root ? script : `${pkg.name ?? pkg.dir}: ${script}`,
    type: "npm",
    options: {
      packageJson: pkg.path,
      command: "run",
      script,
      packageManager: pkg.packageManager.manager,
    },
  };
}

export async function runScript(pkg: NpmPackage, script: string, debug = false): Promise<void> {
  const cfg = useTemporary(scriptConfiguration(pkg, script));
  await (debug ? debugConfiguration(cfg) : runConfiguration(cfg));
}

/** `<package manager> install` in the package's directory. */
export async function installDependencies(pkg: NpmPackage): Promise<void> {
  const cfg = useTemporary({
    name: `${pkg.packageManager.manager} install`,
    type: "npm",
    options: { packageJson: pkg.path, command: "install", packageManager: pkg.packageManager.manager },
  });
  await runConfiguration(cfg);
}

/** The package of a package.json path (from the npm tool window's data). */
export const packageFor = (path: string): NpmPackage | undefined => packages().find((p) => p.path === path);

/** Script runs from an editor that may hold unsaved text: fall back to the root package. */
export function packageOrDefault(path: string): NpmPackage {
  return (
    packageFor(path) ?? {
      path,
      dir: path.split("/").slice(0, -1).join("/"),
      packageManager: packages()[0]?.packageManager ?? { manager: "npm", reason: { kind: "default" } },
      scripts: [],
    }
  );
}

export function jumpToScript(pkg: NpmPackage, script?: NpmScript): void {
  void openFileAt(pkg.path, script ? script.line + 1 : 1, script ? script.column + 1 : 1);
}

registerDebugLauncher({
  id: "node-inspector",
  canDebug: (cfg) => cfg.type === "npm",
  async launch(cfg, ctx) {
    const options = getSetting<string>("web.npm.debugArguments", "--inspect").trim() || "--inspect";
    const existing = cfg.env?.NODE_OPTIONS;
    await ctx.run({ env: { NODE_OPTIONS: existing ? `${existing} ${options}` : options } });
  },
});
