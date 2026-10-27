/**
 * Vulnerable dependency check (Rust: `crates/forge-vulns`, `commands/vulnerabilities.rs`):
 * each ecosystem's audit tool runs on the project's machine; the problems of a manifest and
 * the Upgrade edit are computed from its text. Lines are 0-based, columns UTF-16.
 */
import { invoke } from "./index";
import type { CommandLine } from "./run";

export type Ecosystem = "composer" | "npm" | "pypi" | "cargo" | "go" | "maven";
export type Severity = "critical" | "high" | "moderate" | "low" | "unknown";
export type ToolId =
  | "composer"
  | "npm"
  | "pnpm"
  | "yarn-classic"
  | "yarn-berry"
  | "pip-audit"
  | "cargo-audit"
  | "govulncheck"
  | "dependency-check";

export interface Vulnerability {
  ecosystem: Ecosystem;
  package: string;
  installed?: string;
  /** A CVE when there is one, else the database's id. */
  id: string;
  aliases?: string[];
  severity: Severity;
  title: string;
  affected?: string;
  fixed?: string;
  url?: string;
  /** Direct dependencies that pull a transitive package in. */
  via?: string[];
  /** govulncheck: the project calls the vulnerable code. */
  reachable?: boolean;
}

export interface Notice {
  ecosystem: Ecosystem;
  package: string;
  kind: "abandoned" | "unmaintained" | "yanked";
  message: string;
  replacement?: string;
  url?: string;
}

export interface TargetResult {
  tool: ToolId;
  dir: string;
  manifest: string;
  lockfile?: string;
  annotate: string[];
  inputs: string[];
  blocked?: string;
  status: "ok" | "blocked" | "missing" | "failed";
  message?: string;
  cached: boolean;
  /** Unix seconds. */
  checkedAt?: number;
  vulnerabilities: Vulnerability[];
  notices?: Notice[];
}

export interface CheckResult {
  /** `vulnerabilities.enabled`. */
  enabled: boolean;
  results: TargetResult[];
}

export interface Upgrade {
  package: string;
  fixed: string;
  constraint: string;
}

export interface VulnerabilityProblem {
  line: number;
  start: number;
  end: number;
  severity: "error" | "warning" | "weak";
  rating: Severity;
  message: string;
  package: string;
  ids?: string[];
  upgrade?: Upgrade;
}

export const TOOL_LABELS: Record<ToolId, string> = {
  composer: "composer audit",
  npm: "npm audit",
  pnpm: "pnpm audit",
  "yarn-classic": "yarn audit",
  "yarn-berry": "yarn npm audit",
  "pip-audit": "pip-audit",
  "cargo-audit": "cargo audit",
  govulncheck: "govulncheck",
  "dependency-check": "OWASP dependency-check",
};

export const vulnerabilitiesApi = {
  check: (force: boolean, tools?: ToolId[]) => invoke<CheckResult>("vulnerabilities_check", { force, tools: tools ?? null }),
  problems: (path: string, text: string, vulnerabilities: Vulnerability[], notices: Notice[]) =>
    invoke<VulnerabilityProblem[]>("vulnerabilities_problems", { path, text, vulnerabilities, notices }),
  upgrade: (path: string, text: string, pkg: string, fixed: string) =>
    invoke<string | null>("vulnerabilities_upgrade", { path, text, package: pkg, fixed }),
  installCommand: (tool: ToolId, dir: string, pkg?: string, fixed?: string) =>
    invoke<CommandLine>("vulnerabilities_install_command", { tool, dir, package: pkg ?? null, fixed: fixed ?? null }),
};
