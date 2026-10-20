/** Small building blocks shared by the merge request views. */
import { Match, Show, Switch, type JSX } from "solid-js";
import {
  CircleCheck,
  CircleDashed,
  CircleDot,
  CircleMinus,
  CirclePause,
  CircleX,
  GitMerge,
  GitPullRequest,
  GitPullRequestClosed,
  GitPullRequestDraft,
  LoaderCircle,
  Play,
} from "lucide-solid";
import type { CiStatus, Label, MergeRequest, User } from "@/ipc/forges";
import { timeAgo } from "@/features/git/format";

export const ago = (iso: string | null | undefined) => (iso ? timeAgo(Date.parse(iso) / 1000) : "");

export const displayName = (u: User | null | undefined) => u?.name || u?.username || "unknown";

const avatarPalette = ["--graph-0", "--graph-1", "--graph-2", "--graph-3", "--graph-4", "--graph-5", "--graph-6"];

function hue(s: string): string {
  let h = 0;
  for (const c of s) h = (Math.imul(h, 31) + c.charCodeAt(0)) | 0;
  return `var(${avatarPalette[Math.abs(h) % avatarPalette.length]})`;
}

export function Avatar(props: { user: User | null | undefined; size?: number; title?: string }) {
  const size = () => props.size ?? 20;
  const initials = () =>
    displayName(props.user)
      .split(/[\s._-]+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((p) => p[0]!.toUpperCase())
      .join("");
  return (
    <span
      class="fg-avatar"
      title={props.title ?? displayName(props.user)}
      style={{ width: `${size()}px`, height: `${size()}px`, "font-size": `${Math.round(size() * 0.42)}px`, background: hue(props.user?.username ?? "?") }}
    >
      <Show when={props.user?.avatarUrl} fallback={initials()}>
        <img src={props.user!.avatarUrl!} alt="" />
      </Show>
    </span>
  );
}

export const ciLabels: Record<CiStatus, string> = {
  success: "Passed",
  failed: "Failed",
  running: "Running",
  pending: "Pending",
  canceled: "Canceled",
  skipped: "Skipped",
  manual: "Manual",
};

export function CiIcon(props: { status: CiStatus | null | undefined; size?: number }) {
  const size = () => props.size ?? 14;
  return (
    <span class={`fg-ci fg-ci-${props.status ?? "none"}`} title={props.status ? `CI: ${ciLabels[props.status]}` : undefined}>
      <Switch>
        <Match when={props.status === "success"}>
          <CircleCheck size={size()} />
        </Match>
        <Match when={props.status === "failed"}>
          <CircleX size={size()} />
        </Match>
        <Match when={props.status === "running"}>
          <LoaderCircle size={size()} class="spin" />
        </Match>
        <Match when={props.status === "pending"}>
          <CircleDashed size={size()} />
        </Match>
        <Match when={props.status === "canceled"}>
          <CircleMinus size={size()} />
        </Match>
        <Match when={props.status === "skipped"}>
          <CirclePause size={size()} />
        </Match>
        <Match when={props.status === "manual"}>
          <Play size={size()} />
        </Match>
      </Switch>
    </span>
  );
}

/** Icon for a merge request by state (open, draft, merged, closed). */
export function MrStateIcon(props: { mr: Pick<MergeRequest, "state" | "draft">; size?: number }) {
  const size = () => props.size ?? 16;
  return (
    <span class={`fg-state-icon fg-state-${props.mr.state}${props.mr.draft && props.mr.state === "open" ? " fg-state-draft" : ""}`}>
      <Switch fallback={<GitPullRequest size={size()} />}>
        <Match when={props.mr.state === "merged"}>
          <GitMerge size={size()} />
        </Match>
        <Match when={props.mr.state === "closed"}>
          <GitPullRequestClosed size={size()} />
        </Match>
        <Match when={props.mr.draft}>
          <GitPullRequestDraft size={size()} />
        </Match>
      </Switch>
    </span>
  );
}

export function StateBadge(props: { mr: Pick<MergeRequest, "state" | "draft"> }) {
  const text = () => (props.mr.state === "open" ? (props.mr.draft ? "Draft" : "Open") : props.mr.state === "merged" ? "Merged" : "Closed");
  return <span class={`fg-badge fg-badge-${props.mr.state}${props.mr.draft && props.mr.state === "open" ? " fg-badge-draft" : ""}`}>{text()}</span>;
}

export function LabelPill(props: { label: Label }) {
  return (
    <span class="fg-label" style={{ "--label-color": props.label.color ?? "var(--fg-muted)" } as JSX.CSSProperties}>
      {props.label.name}
    </span>
  );
}

export function BranchPill(props: { name: string }) {
  return (
    <span class="fg-branch" title={props.name}>
      {props.name}
    </span>
  );
}

export function IssueStateIcon(props: { state: "open" | "closed"; size?: number }) {
  return (
    <span class={`fg-issue-icon fg-issue-${props.state}`}>
      <Show when={props.state === "open"} fallback={<CircleCheck size={props.size ?? 16} />}>
        <CircleDot size={props.size ?? 16} />
      </Show>
    </span>
  );
}
