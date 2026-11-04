/**
 * Code hosts (GitLab, Forgejo/Gitea, GitHub, Azure DevOps): the Pull/Merge Requests tool window, request
 * and issue editor tabs, Create Merge Request dialog, account settings pages and the CI
 * status of the current branch in the status bar.
 */
import { createEffect, createRoot, Show } from "solid-js";
import { isDialogWindow } from "@/shell/window/mode";
import { render } from "solid-js/web";
import { GitPullRequest } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerStatusItem } from "@/features/statusItems";
import { registerCommand } from "@/state/commands";
import { showToolWindow } from "@/state/layout";
import { MergeRequestsPanel, PanelActions } from "./MergeRequestsPanel";
import { AccountsPage } from "./AccountsPage";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { ForgeDialogs } from "./Dialogs";
import { openCreateDialog } from "./actions";
import { branchPipeline, context, refreshAll, requestName, startForgesStore } from "./store";
import { CiIcon, ciLabels } from "./ui";
import "./forges.css";
import { t } from "@/i18n";

const toolWindow = (title: string) =>
  registerToolWindow({
    id: "pull-requests",
    title,
    icon: GitPullRequest,
    anchor: "left",
    order: 20,
    component: MergeRequestsPanel,
    actions: PanelActions,
  });
toolWindow("Pull Requests");

const pages = [
  { id: "vcs.gitlab", title: "GitLab", kind: "gitlab", keywords: ["merge request", "token", "self-managed"] },
  { id: "vcs.forgejo", title: "Forgejo / Gitea", kind: "forgejo", keywords: ["codeberg", "pull request", "token"] },
  { id: "vcs.github", title: "GitHub", kind: "github", keywords: ["pull request", "token", "enterprise"] },
  { id: "vcs.azure", title: "Azure DevOps", kind: "azure", keywords: ["azure repos", "pull request", "work items", "pat", "token", "tfs", "visual studio team services"] },
] as const;
// Version Control | Code Hosts: refresh interval and the status bar pipeline; the accounts
// of each host are pages below it.
registerSettingsPage({
  id: "vcs.hosts",
  title: t("settings.page.vcs.hosts"),
  group: "Version Control",
  order: 10,
  keywords: ["pipeline", "ci", "merge request", "pull request", "refresh", "gitlab", "github", "forgejo", "azure devops"],
  component: () => <SchemaPage pageId="vcs.hosts" />,
});
pages.forEach((p, i) =>
  registerSettingsPage({
    id: p.id,
    title: p.title,
    group: "Version Control",
    parent: "vcs.hosts",
    order: i,
    keywords: [...p.keywords, "account", "log in"],
    component: () => <AccountsPage kind={p.kind} />,
  }),
);

registerCommand({ id: "forges.createMergeRequest", title: "Create Merge Request…", category: "Git", run: openCreateDialog });
registerCommand({
  id: "forges.showMergeRequests",
  title: "View Pull/Merge Requests",
  category: "Git",
  run: () => showToolWindow("pull-requests"),
});
registerCommand({ id: "forges.refresh", title: "Refresh Pull/Merge Requests", category: "Git", run: () => void refreshAll() });

function PipelineStatus() {
  return (
    <Show when={branchPipeline()}>
      {(p) => (
        <button
          class="status-item fg-status-ci"
          data-status="pipeline"
          title={`Pipeline for ${p().ref ?? "this branch"}: ${ciLabels[p().status]}${p().jobs.length ? `\n${p().jobs.map((j) => `${ciLabels[j.status]}  ${j.name}`).join("\n")}` : ""}`}
          onClick={() => (p().webUrl ? window.open(p().webUrl!, "_blank") : showToolWindow("pull-requests"))}
        >
          <CiIcon status={p().status} size={12} />
          CI: {ciLabels[p().status]}
        </button>
      )}
    </Show>
  );
}
registerStatusItem({ id: "forges.pipeline", align: "right", order: 95, component: PipelineStatus });

if (typeof window !== "undefined") {
  const host = document.createElement("div");
  host.id = "forge-hosts-overlays";
  document.body.append(host);
  render(ForgeDialogs, host);
  // Code host polling belongs to the IDE window, not to dialog windows.
  if (!isDialogWindow()) startForgesStore();
  createRoot(() => {
    // "Merge Requests" on GitLab, "Pull Requests" elsewhere, like PHPStorm.
    createEffect(() => {
      if (context()) toolWindow(`${requestName()}s`);
    });
  });
}
