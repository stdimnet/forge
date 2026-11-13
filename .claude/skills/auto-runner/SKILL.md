---
name: auto-runner
description: Work through docs/ROADMAP.md milestone by milestone without being prompted for each step - pick the next open item, build it on its own branch, open a pull request, merge it and delete the branch. Use when a scheduled "Auto-Runner tick" arrives or when asked to continue the roadmap autonomously.
---

# Auto-Runner

The project owner asked for an autonomous runner that builds the roadmap phase by phase. This
skill is that runner's contract. It overrides the AGENTS.md rule "do not push or open pull
requests unless asked" **only** for the steps below; every other rule in AGENTS.md still applies.

## Each tick

1. **Catch up first.** Read `ReadNotifications`, the open pull requests labelled `auto-runner`
   and `git fetch origin`. Finish or repair work in flight before starting anything new:
   a red or conflicted runner PR is fixed before new work starts.
2. **Pick the next item.** The first unchecked `- [ ]` item of the lowest milestone in
   `docs/ROADMAP.md` that no open `auto-runner` PR covers. Split items that are too big for one
   reviewable PR (rule of thumb: one feature, one crate or one tool window per PR).
3. **Ask when it is the owner's decision.** Product choices that the roadmap, PHPStorm's
   behaviour and AGENTS.md do not settle (licences, paid services, dropping a feature, a new
   dependency with a non-permissive licence, anything that costs money) go to the owner as a
   short question. Skip that item and continue with the next one until the answer arrives.
4. **Build it.**
   - Branch from the latest `origin/dev/gallant-cerf-630lu4`: `feat/…`, `fix/…`, `docs/…`,
     `chore/…`, `refactor/…` (AGENTS.md naming).
   - Large items go to worktree agents (at most 3 at once, each with its own
     `CARGO_TARGET_DIR`); small ones are done directly.
   - Follow the matching skills (`add-tool-window`, `add-ipc-command`, …), write tests, docs
     (`docs/guide/`, `pnpm docs:llms`) and screenshots, and look at the screenshots.
5. **Gates.** Run the `quality-gates` skill completely. Never open a PR with red gates.
6. **Pull request.**
   - Push the branch and open a PR against `dev/gallant-cerf-630lu4` with the label
     `auto-runner`. The title is the commit-style summary; the body says what changed, how it
     was verified and which roadmap item it closes.
   - Tick the roadmap item (`- [x]`) in the same PR.
7. **Merge and clean up.**
   - Merge the PR yourself with a merge commit once the gates passed and there is no conflict
     (on a conflict: merge the base into the branch, rerun the gates, push).
   - Then delete the branch locally and remove its worktree. On GitHub the repository setting
     "Automatically delete head branches" deletes it on merge; this environment's git proxy
     refuses branch deletion (HTTP 403). If the branch is still there after the merge, add it
     to the owner's cleanup list (see "Branches left on GitHub" below) instead of retrying.
8. **Milestone done.** When every item of a milestone is ticked, build the AppImage, smoke-test
   it under Xvfb, note the result in the milestone heading ("(done)") and tell the owner in one
   short message. Do not send installers unless asked.

## Never

- Delete, push to, rebase or merge a branch the runner did not create. A branch is the
  runner's only if its PR carries the `auto-runner` label or it is listed under "Phase 1
  branches" below. Branches of other people (e.g. `feat/code-with-me`, `dev/robert`) and
  issue branches are never touched.
- Push to `main` or `dev/main` directly, force-push, or rewrite published history.
- Trigger workflows, create or publish releases, or bump the version.
- Add co-author or other trailers, or mention AI tools in commits.
- Commit secrets or paid tools (e.g. Intelephense).

## Branches left on GitHub

When a merged runner branch cannot be deleted, tell the owner once per milestone with a
ready-to-run command that deletes only fully merged runner branches, for example:

```sh
git fetch --prune origin
for b in feat/a fix/b; do
  git merge-base --is-ancestor "origin/$b" origin/dev/gallant-cerf-630lu4 && git push origin --delete "$b"
done
```

## Phase 1 branches

Created by the owner's main session before the runner existed. They may be deleted once phase
1 is merged, and only if they are fully merged into `dev/gallant-cerf-630lu4`:

chore/bundle-remote-agents, chore/forge-wasm-bridge, chore/pride-versioning,
chore/rename-repo-to-forge, chore/phase-1-finish, ci/release-workflow, ci/self-hosted-runner,
docs/agents-md, docs/contributor-skills, docs/user-and-developer-guide, feat/browser-companion,
feat/database-tools, feat/dialog-windows, feat/docker-kubernetes, feat/editor-polish,
feat/gemini-cli-plugin, feat/gitlab-forgejo, feat/integration-polish,
feat/intelephense-autoinstall, feat/language-servers, feat/laravel-plugin, feat/luaforge,
feat/main-menu, feat/php-navigation, feat/project-tree-interactions, feat/project-wizard,
feat/remote-finish, feat/remote-parity, feat/remote-sessions, feat/rename-and-intentions,
feat/scratch-files, feat/services-polish, feat/settings-complete, feat/splash-and-updates,
feat/symfony-doctrine, feat/syntax-highlighting, feat/template-data-languages,
feat/terminal-run-configs, feat/webforge, feat/workbench-complete, feat/workspace-skeleton,
feat/xdebug-debugger, fix/compressed-remote-agents, fix/find-action-alias-duplicates,
fix/git-mock-unhandled-rejection, fix/main-menu-new-file, fix/php-generate-through-keymap,
fix/php-stubs-content-digest, fix/remote-file-operations, fix/remote-reconnect-test-race,
fix/watch-updates-without-receivers, refactor/rust-keymap, refactor/rust-parsers,
refactor/rust-search-languages
