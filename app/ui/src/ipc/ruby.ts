/**
 * RubyForge (`app/src-tauri/src/commands/ruby.rs`, logic in `packs/rubyforge/*`): the
 * project's Ruby (interpreters of version managers, the version file, gem executables),
 * the bundle (Gemfile + Gemfile.lock), Rake tasks, Bundler / gem / Rake / IRB command
 * lines, script run targets, RuboCop problems and autocorrect, Ruby LSP installs and rdbg
 * debug runs. Everything is computed in Rust.
 */
import { invoke } from "./index";
import type { CommandLine, RunConfiguration } from "./run";

export interface RubyInterpreter {
  /** The `ruby` executable. */
  path: string;
  /** `3.3.6`, `jruby-9.4.8.0` (empty when unknown). */
  version: string;
  /** `rbenv`, `rvm`, `chruby`, `asdf`, `mise`, `frum`, `rv`, `homebrew`, `rubyinstaller`, `system`, `custom`. */
  manager: string;
  /** `rbenv: 3.3.6`. */
  name: string;
  binDir: string;
  gemPaths?: string[];
}

export interface VersionRequest {
  version: string;
  /** `.ruby-version`, `.tool-versions`, `mise.toml`, `Gemfile`. */
  file: string;
}

export interface ChosenInterpreter {
  interpreter: RubyInterpreter;
  /** `setting`, `versionFile`, `global` or `newest`. */
  reason: "setting" | "versionFile" | "global" | "newest";
  request?: VersionRequest;
}

export interface RubyStatus {
  interpreters: RubyInterpreter[];
  chosen?: ChosenInterpreter;
  request?: VersionRequest;
  /** Gem executables of the chosen interpreter: `rspec`, `rake`, `rubocop`, `rdbg`, `irb`, `ruby-lsp`, `bundle`, `gem`. */
  tools: Record<string, string>;
  probe?: { version: string; userDir: string; gemPath: string[] };
  managers: string[];
  /** The interpreter chosen in Settings (empty: automatic). */
  setting: string;
  /** The project has a Gemfile. */
  bundler: boolean;
  lsp: { path?: string | null; managed: boolean; installDir?: string | null };
}

export interface BundleGem {
  name: string;
  /** Locked version (empty: not locked yet). */
  version: string;
  direct: boolean;
  groups: string[];
  requirements?: string[];
  dependencies?: string[];
  /** 0-based Gemfile line of a direct gem. */
  line?: number;
  /** `rubygems`, `git` or `path`. */
  source: string;
}

export interface Bundle {
  gemfile: { sources: string[]; ruby?: string; gemspec?: boolean; gems: unknown[] };
  lock?: { platforms: string[]; rubyVersion?: string; bundledWith?: string } | null;
  gems: BundleGem[];
  /** Gemfile gems the lock file lacks: `bundle install` is due. */
  missing: string[];
}

export interface RakeTask {
  /** `db:migrate`. */
  name: string;
  args?: string[];
  description?: string;
  file?: string;
  /** 0-based. */
  line?: number;
}

export interface RakeTasks {
  tasks: RakeTask[];
  /** `rake` (rake --tasks --all) or `files` (read from the Rakefile without running Rake). */
  source: "rake" | "files";
  error?: string;
}

export interface RubyProblem {
  path: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  severity: "error" | "warning" | "weak" | "info";
  /** RuboCop's level: `convention`, `refactor`, `warning`, `error`, `fatal`. */
  level: string;
  message: string;
  cop: string;
  correctable: boolean;
  source: string;
}

export interface RubyRunTarget {
  line: number;
  configuration: RunConfiguration;
}

export interface RdbgPlan {
  /** The port rdbg listens on: pass it as `FORGE_RDBG_PORT` with the run. */
  port: number;
  found: boolean;
  rdbg: string;
}

export type BundlerAction = "install" | "update" | "outdated" | "check" | "lock" | "init";

/** The run environment variable that makes a Ruby run a debug run under rdbg. */
export const RDBG_PORT_ENV = "FORGE_RDBG_PORT";

export const rubyApi = {
  status: (refresh = false) => invoke<RubyStatus>("ruby_status", { refresh }),
  probeInterpreter: (path: string) => invoke<RubyInterpreter>("ruby_interpreter_probe", { path }),
  bundle: () => invoke<Bundle | null>("ruby_bundle"),
  bundleCommand: (action: BundlerAction, gem?: string) => invoke<CommandLine>("ruby_bundle_command", { action, gem }),
  gemInstallCommand: (gem: string) => invoke<CommandLine>("ruby_gem_install_command", { gem }),
  lspInstallCommand: () => invoke<CommandLine>("ruby_lsp_install_command"),
  lspRefresh: () => invoke<void>("ruby_lsp_refresh"),
  rakeTasks: (refresh = false) => invoke<RakeTasks>("ruby_rake_tasks", { refresh }),
  rakeCommand: (task: string, args?: string) => invoke<CommandLine>("ruby_rake_command", { task, args }),
  fileTargets: (path: string, content: string) => invoke<RubyRunTarget[]>("ruby_file_targets", { path, content }),
  irbCommand: () => invoke<CommandLine>("ruby_irb_command"),
  rubocop: (path: string, text?: string) => invoke<RubyProblem[]>("ruby_rubocop", { path, text }),
  format: (path: string, content: string, mode?: "safe" | "all") => invoke<string | null>("ruby_format", { path, content, mode }),
  debugPlan: () => invoke<RdbgPlan>("ruby_debug_plan"),
};
