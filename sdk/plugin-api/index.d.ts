/**
 * The Forge plugin API, version 1.
 *
 * A UI plugin is a script that runs in a sandboxed iframe (no DOM access to the IDE, no
 * network, no storage). It exports `activate(context)` (and optionally `deactivate()`);
 * everything else goes through the `forge` global. Calls that leave the sandbox are checked
 * against the permissions declared in `forge-plugin.json` and granted by the user.
 *
 * ```ts
 * import type { PluginContext } from "@forge/plugin-api";
 *
 * export function activate(context: PluginContext): void {
 *   forge.commands.register("acme.hello.sayHello", () => forge.window.showMessage("Hello!"));
 * }
 * ```
 */

export interface Disposable {
  dispose(): void;
}

export interface PluginInfo {
  id: string;
  name: string;
  version: string;
}

export interface PluginContext {
  /** Disposables released when the plugin is unloaded. */
  subscriptions: Disposable[];
  plugin: PluginInfo;
}

export interface ActiveEditor {
  /** Absolute path of the file (or a virtual path such as a scratch). */
  path: string;
  /** Forge file type id (`php`, `markdown`, …) or null for plain text. */
  language: string | null;
  text: string;
  /** Offsets into `text`. */
  selection: { from: number; to: number };
}

export interface StatusItemState {
  text: string;
  tooltip?: string;
}

export interface FetchInit {
  method?: string;
  headers?: Record<string, string>;
  body?: string;
}

export interface FetchResponse {
  status: number;
  headers: Record<string, string>;
  body: string;
}

export interface ProjectChange {
  phase: "closing" | "opened" | "ready";
  name: string;
  /** The project's folder (`user@host:/path` for a remote project). */
  root: string;
}

export interface RunOutput {
  exitCode: number | null;
  stdout: string;
  stderr: string;
}

export interface ForgeApi {
  readonly apiVersion: 1;
  readonly plugin: PluginInfo;
  commands: {
    /** Implement a command declared in `contributes.commands`. */
    register(id: string, handler: (...args: unknown[]) => unknown): Disposable;
    /** Run any IDE action by its PHPStorm id (`ReformatCode`) or a plugin command. */
    execute(id: string): Promise<void>;
  };
  statusBar: {
    /** Set the text of a status item declared in `contributes.statusItems`; empty text hides it. */
    update(id: string, item: StatusItemState): Promise<void>;
  };
  toolWindows: {
    /**
     * Content of a tool window declared in `contributes.toolWindows`: HTML rendered in its own
     * sandboxed frame with the IDE's colors as CSS variables (`--bg`, `--fg`, `--fg-muted`,
     * `--accent`, `--border`). Its scripts talk to the plugin with `forgeView.postMessage(data)`
     * and `forgeView.onMessage(callback)`.
     */
    setHtml(id: string, html: string): Promise<void>;
    postMessage(id: string, data: unknown): Promise<void>;
    onMessage(id: string, callback: (data: unknown) => void): Disposable;
  };
  editor: {
    /** Needs `"editor": ["read"]`. */
    getActive(): Promise<ActiveEditor | null>;
    /** Needs `"editor": ["write"]`. */
    replaceSelection(text: string): Promise<void>;
    /** The active editor or its text changed (debounced). Needs `"editor": ["read"]`. */
    onDidChange(callback: (editor: ActiveEditor | null) => void): Disposable;
  };
  window: {
    showMessage(text: string, type?: "info" | "warning" | "error"): Promise<void>;
  };
  settings: {
    /** The plugin's own settings (`<plugin id>.…`), or keys granted by `"settings"`. */
    get<T = unknown>(key: string): Promise<T | null>;
    set(key: string, value: unknown): Promise<void>;
    onDidChange(callback: (change: { key: string; value: unknown }) => void): Disposable;
  };
  workspace: {
    /** The open project's folder, or null. */
    root(): Promise<string | null>;
    /**
     * The window's project lifecycle: `closing` (the old project closes), `opened` (another
     * project opened in this window) and `ready` (its index is built).
     */
    onDidChangeProject(callback: (change: ProjectChange) => void): Disposable;
  };
  fs: {
    /** Absolute paths inside the granted `"fs"` scopes (`${project}`, `${pluginData}`). */
    readFile(path: string): Promise<string>;
    writeFile(path: string, contents: string): Promise<void>;
  };
  http: {
    /** Requests to the hosts granted by `"network"`. */
    fetch(url: string, init?: FetchInit): Promise<FetchResponse>;
  };
  process: {
    /** Programs granted by `"process"`, run in the project folder. Refused in Safe Mode. */
    run(program: string, args?: string[]): Promise<RunOutput>;
  };
}

declare global {
  const forge: ForgeApi;
}
