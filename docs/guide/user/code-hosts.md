---
title: Code hosts
description: Review pull and merge requests with inline threads, browse issues and CI pipelines for GitHub, GitLab, Forgejo/Gitea and Azure DevOps, including self-hosted servers.
order: 130
tags: [github, gitlab, forgejo, gitea, azure devops, pull requests, merge requests, work items, code review, ci]
---

# Code hosts

The plugin **GitLab, Forgejo, GitHub and Azure DevOps** (`forge.code-hosts`) brings code review into Forge:

| Host | API | Number format | Self-hosted |
| --- | --- | --- | --- |
| GitLab | REST v4 | `!12` | yes (self-managed, also with a relative URL root or SSH port) |
| Forgejo / Gitea | `/api/v1` | `#12` | yes (e.g. Codeberg) |
| GitHub | REST + GraphQL for review threads | `#12` | GitHub Enterprise Server |
| Azure DevOps | REST 7.1 | `!12` | Azure DevOps Server (the collection URL) |

The repository is detected from `git remote -v` (https, ssh and scp-style URLs, nested GitLab
groups); in a [remote project](remote-development.md) git runs on the host, while the API calls
go from your machine with your tokens. Wording follows the host: "merge request" on GitLab, "pull request" elsewhere.

## Logging in

Open Settings | Version Control | **GitLab**, **Forgejo / Gitea**, **GitHub** or **Azure DevOps** and click
**Log In with Token…**. Enter the **Server** (for self-hosted instances) and an access token:

| Host | Token permissions |
| --- | --- |
| GitLab | scopes `api`, `read_user` |
| Forgejo / Gitea | read/write for repository and issue, read for user |
| GitHub | scopes `repo`, `read:org`, `workflow` (or a fine-grained token with pull request, issue and actions access) |
| Azure DevOps | a personal access token with Code (Read & Write), Work Items (Read & Write), Build (Read), Project and Team (Read) |

![Accounts](../../screenshots/forges-accounts.png)

The token is verified first and then stored in the operating system keychain (service
`forge-forges`), never in a settings file. Accounts themselves (server, user) are kept in
`forges.toml` in Forge's config folder. Without an account, the environment variables
`GITLAB_TOKEN`, `FORGEJO_TOKEN` / `GITEA_TOKEN`, `GITHUB_TOKEN` / `GH_TOKEN` and
`AZURE_DEVOPS_EXT_PAT` are used for the public servers ("Token from environment").

Errors are explained in plain words: a rejected token, missing permissions, rate limits (with
the reset time) or an unreachable server.

## Pull Requests tool window

The **Pull Requests** tool window (left stripe, **View Pull/Merge Requests** in Find Action)
has three tabs.

![Merge request list](../../screenshots/forges-mr-list.png)

### Pull/Merge Requests

- Search, plus filters for **State** (Open, Merged, Closed, All), **Author**, **Label**,
  **Assignee** and **Reviewer**, and **Quick Filters** such as "Assigned to you" and
  "Review requests".
- Each row shows the title, number, author, age, CI status, comment count and avatar.
- Click a request to open it as an editor tab.
- **+** (or **Create Merge Request…** in Find Action) creates one.

### A pull/merge request tab

![Overview](../../screenshots/forges-mr-overview.png)

- **Overview**: description (Markdown), comments and review threads with **Reply**,
  resolve/unresolve (where the host supports it; Forgejo cannot resolve threads through its
  API), reviewers and approvals, assignees, labels, the pipeline with its jobs, and the merge
  status ("No conflicts with main").
- **Files**: the changed files with their diffs; comment on a line to start a thread.
  **Show Diff (side by side)** opens the diff viewer.
- Actions: **Approve**, **Merge…** (merge commit, **Squash and merge**, **Rebase and
  merge**, "Mark as ready before merging" for drafts, the commit message), **Close**,
  **Check Out** (fetches the source branch and checks it out locally), **Open in Browser** and
  **Refresh**.

![Files](../../screenshots/forges-mr-files.png)

### Create a pull/merge request

**Create Merge Request…** asks for the **Head branch** and **Base branch**, **Title**,
**Description** (Markdown), **Reviewers**, **Labels** and whether it is a draft.

![Create merge request](../../screenshots/forges-create-mr.png)

### Issues

Search and filter issues, open one as a tab, read and add comments.

![Issues](../../screenshots/forges-issues.png)

### Pipelines

CI runs of the repository (GitLab pipelines, GitHub Actions workflow runs, Forgejo/Gitea
Actions runs, Azure Pipelines builds) with their status and jobs.

## Azure DevOps

![Settings | Version Control | Azure DevOps](../../screenshots/azure-devops-accounts.png)

- The account's **Server** is the organization, `https://dev.azure.com/<organization>`
  (Azure DevOps Server: the collection URL, such as `https://tfs.example.com/tfs/DefaultCollection`);
  the token is a personal access token of that organization, sent with HTTP Basic
  authentication.
- Remotes are recognized in all their forms: `https://<org>@dev.azure.com/<org>/<project>/_git/<repo>`,
  `git@ssh.dev.azure.com:v3/<org>/<project>/<repo>`, `https://<org>.visualstudio.com/…`
  and `…/_git/<repo>` on a server.
- **Pull requests**: the list (active, completed, abandoned), details with reviewers and
  their votes (approved, waiting for author, rejected), required reviewers, merge status and
  the latest build, the changed files (diffed by Forge from the merge base, since the API has
  no patches), threads with replies and **Resolve** (status *Fixed* / *Active*), **Approve**
  (vote 10), **Complete** (merge commit, squash or rebase, delete the source branch) and
  **Abandon**. **Create Pull Request** sets reviewers, labels, draft and "delete source branch".
- **Issues** are the project's **work items** (found with a WIQL query: open or closed states
  of the Agile, Scrum, CMMI and Basic processes, assignee, author, tag and title search), with
  their type and tags as labels, descriptions and comments; **Comment** adds a work item
  comment.
- **Pipelines** are the builds of the repository (Azure Pipelines), with the status bar's CI
  status of the current branch.

## CI status in the status bar

The status bar shows the CI status of the current branch ("CI: Running", "Passed", "Failed").
Settings | Version Control | **Code Hosts** (the account pages are below it):

| Setting | Default | Meaning |
| --- | --- | --- |
| `vcs.hosts.statusBarPipeline` | `true` | show the CI status of the current branch |
| `vcs.hosts.refreshSeconds` | `120` | refresh requests and pipelines every N seconds; `0` turns automatic refresh off |
