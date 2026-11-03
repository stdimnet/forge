/**
 * Code hosts (GitLab, Forgejo/Gitea, GitHub, Azure DevOps): typed client for `commands/forges.rs`.
 * GitHub and Forgejo call merge requests "pull requests"; `capabilities.requestName`
 * carries the host's wording.
 */
import { invoke } from "./index";

export type HostKind = "gitlab" | "forgejo" | "github" | "azure";

export interface RepoRef {
  kind: HostKind;
  /** Web URL of the server, e.g. `https://git.acme.dev`. */
  server: string;
  /** `owner/repo` or `group/subgroup/project`. */
  path: string;
  remote: string;
}

export interface User {
  username: string;
  name: string | null;
  avatarUrl: string | null;
}

export interface Label {
  name: string;
  color: string | null;
}

export type MrState = "open" | "closed" | "merged";
export type CiStatus = "success" | "failed" | "running" | "pending" | "canceled" | "skipped" | "manual";

export interface MergeRequest {
  number: number;
  title: string;
  state: MrState;
  draft: boolean;
  author: User;
  sourceBranch: string;
  targetBranch: string;
  labels: Label[];
  comments: number | null;
  ci: CiStatus | null;
  createdAt: string;
  updatedAt: string;
  webUrl: string;
  headSha: string | null;
}

export type ReviewState = "requested" | "approved" | "changesRequested" | "commented";

export interface Reviewer {
  user: User;
  state: ReviewState;
}

export interface Job {
  name: string;
  stage: string | null;
  status: CiStatus;
  webUrl: string | null;
  duration: number | null;
}

export interface Pipeline {
  id: string;
  name: string | null;
  status: CiStatus;
  ref: string | null;
  sha: string | null;
  webUrl: string | null;
  createdAt: string | null;
  jobs: Job[];
}

export interface MergeRequestDetails extends MergeRequest {
  description: string;
  reviewers: Reviewer[];
  assignees: User[];
  approvals: { required: number; approvedBy: User[] };
  pipeline: Pipeline | null;
  mergeable: boolean | null;
  conflicts: boolean;
  baseSha: string | null;
  startSha: string | null;
  mergedBy: User | null;
}

export type LineKind = "context" | "added" | "removed";

export interface Hunk {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  header: string;
  lines: { kind: LineKind; text: string }[];
}

export interface ChangedFile {
  path: string;
  oldPath: string | null;
  status: "added" | "modified" | "deleted" | "renamed";
  additions: number;
  deletions: number;
  hunks: Hunk[];
}

export interface Note {
  id: string;
  author: User;
  body: string;
  createdAt: string;
  system: boolean;
}

export interface Position {
  path: string;
  oldPath?: string | null;
  newLine?: number | null;
  oldLine?: number | null;
}

export interface Discussion {
  id: string;
  notes: Note[];
  position: Position | null;
  resolvable: boolean;
  resolved: boolean;
}

export type StateFilter = "open" | "closed" | "merged" | "all";

export interface MrFilter {
  state?: StateFilter;
  author?: string | null;
  reviewer?: string | null;
  assignee?: string | null;
  label?: string | null;
  search?: string | null;
  page?: number;
  perPage?: number;
}

export interface IssueFilter {
  state?: "open" | "closed" | "all";
  author?: string | null;
  assignee?: string | null;
  label?: string | null;
  search?: string | null;
  page?: number;
  perPage?: number;
}

export interface Page<T> {
  items: T[];
  nextPage: number | null;
  total: number | null;
}

export interface Issue {
  number: number;
  title: string;
  state: "open" | "closed";
  author: User;
  labels: Label[];
  assignees: User[];
  comments: number;
  createdAt: string;
  updatedAt: string;
  webUrl: string;
}

export interface IssueDetails extends Issue {
  description: string;
  notes: Note[];
}

export interface NewMergeRequest {
  sourceBranch: string;
  targetBranch: string;
  title: string;
  description: string;
  draft: boolean;
  reviewers: string[];
  labels: string[];
  removeSourceBranch: boolean;
}

export type MergeMethod = "merge" | "squash" | "rebase";

export interface MergeOptions {
  method: MergeMethod;
  deleteSourceBranch: boolean;
  commitMessage?: string | null;
}

export interface Capabilities {
  resolveThreads: boolean;
  approvals: boolean;
  draft: boolean;
  mergeMethods: MergeMethod[];
  requestName: string;
  numberPrefix: string;
}

export interface Account {
  id: string;
  kind: HostKind;
  server: string;
  username: string | null;
  apiUrl?: string | null;
  tokenSource: "keychain" | "env" | "none";
}

export interface RepoContext {
  repo: RepoRef;
  repos: RepoRef[];
  account: Account | null;
  capabilities: Capabilities;
}

export const forgesApi = {
  accounts: () => invoke<Account[]>("forges_accounts"),
  login: (kind: HostKind, server: string, token: string, apiUrl?: string | null) =>
    invoke<Account>("forges_login", { kind, server, token, apiUrl: apiUrl ?? null }),
  logout: (id: string) => invoke<void>("forges_logout", { id }),
  verify: (id: string) => invoke<User>("forges_verify", { id }),
  repo: () => invoke<RepoContext | null>("forges_repo"),
  mergeRequests: (filter: MrFilter) => invoke<Page<MergeRequest>>("forges_merge_requests", { filter }),
  mergeRequest: (number: number) => invoke<MergeRequestDetails>("forges_merge_request", { number }),
  files: (number: number) => invoke<ChangedFile[]>("forges_mr_files", { number }),
  discussions: (number: number) => invoke<Discussion[]>("forges_discussions", { number }),
  create: (request: NewMergeRequest) => invoke<MergeRequest>("forges_create_mr", { request }),
  comment: (number: number, body: string, position?: Position | null) =>
    invoke<void>("forges_comment", { number, body, position: position ?? null }),
  reply: (number: number, discussionId: string, body: string) =>
    invoke<void>("forges_reply", { number, discussionId, body }),
  resolve: (number: number, discussionId: string, resolved: boolean) =>
    invoke<void>("forges_resolve", { number, discussionId, resolved }),
  approve: (number: number) => invoke<void>("forges_approve", { number }),
  merge: (number: number, options: MergeOptions) => invoke<void>("forges_merge", { number, options }),
  close: (number: number) => invoke<void>("forges_close", { number }),
  issues: (filter: IssueFilter) => invoke<Page<Issue>>("forges_issues", { filter }),
  issue: (number: number) => invoke<IssueDetails>("forges_issue", { number }),
  commentIssue: (number: number, body: string) => invoke<void>("forges_issue_comment", { number, body }),
  pipelines: (branch?: string | null) => invoke<Pipeline[]>("forges_pipelines", { branch: branch ?? null }),
  ciStatus: (gitRef: string) => invoke<Pipeline | null>("forges_ci_status", { gitRef }),
  members: () => invoke<User[]>("forges_members"),
  file: (path: string, gitRef: string) => invoke<string | null>("forges_file", { path, gitRef }),
  checkout: (number: number, sourceBranch: string) => invoke<string>("forges_checkout", { number, sourceBranch }),
};

export const hostNames: Record<HostKind, string> = {
  gitlab: "GitLab",
  forgejo: "Forgejo / Gitea",
  github: "GitHub",
  azure: "Azure DevOps",
};

export const defaultServers: Record<HostKind, string> = {
  gitlab: "https://gitlab.com",
  forgejo: "https://codeberg.org",
  github: "https://github.com",
  azure: "https://dev.azure.com/",
};
