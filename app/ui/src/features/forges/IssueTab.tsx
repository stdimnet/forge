/** Issue details as an editor tab: description, comments and a comment box. */
import { createEffect, createResource, For, Match, on, Show, Switch } from "solid-js";
import { ExternalLink, RefreshCw } from "lucide-solid";
import { forgesApi } from "@/ipc/forges";
import { run } from "./actions";
import { errorText, revision } from "./store";
import { CommentEditor, Markdown, NoteView } from "./Thread";
import { Avatar, IssueStateIcon, LabelPill, ago, displayName } from "./ui";

export function IssueTab(props: { number: number }) {
  const [issue, { refetch }] = createResource(() => props.number, forgesApi.issue);
  createEffect(on(revision, () => void refetch(), { defer: true }));
  return (
    <div class="fg-mr fg-issue" data-issue-tab={props.number}>
      <Switch>
        <Match when={issue.error}>
          <div class="fg-empty fg-error-text">{errorText(issue.error)}</div>
        </Match>
        <Match when={!issue()}>
          <div class="fg-empty muted">Loading issue #{props.number}…</div>
        </Match>
        <Match when={issue()}>
          {(i) => (
            <>
              <header class="fg-mr-header">
                <div class="fg-mr-title-row">
                  <IssueStateIcon state={i().state} size={18} />
                  <h1 class="fg-mr-title">
                    {i().title} <span class="muted">#{i().number}</span>
                  </h1>
                  <span class={`fg-badge fg-badge-${i().state === "open" ? "open" : "closed"}`}>{i().state === "open" ? "Open" : "Closed"}</span>
                </div>
                <div class="fg-mr-sub">
                  <Avatar user={i().author} size={18} />
                  <span class="fg-note-author">{displayName(i().author)}</span>
                  <span class="muted">opened this issue {ago(i().createdAt)}</span>
                  <For each={i().labels}>{(l) => <LabelPill label={l} />}</For>
                  <span class="spacer" />
                  <button class="icon-btn" title="Open in Browser" onClick={() => window.open(i().webUrl, "_blank")}>
                    <ExternalLink size={15} />
                  </button>
                  <button class="icon-btn" title="Refresh" onClick={() => void refetch()}>
                    <RefreshCw size={14} />
                  </button>
                </div>
              </header>
              <div class="fg-overview fg-overview-single">
                <div class="fg-overview-main">
                  <div class="fg-card fg-description">
                    <Markdown text={i().description} class="fg-description-body" />
                  </div>
                  <div class="fg-timeline">
                    <For each={i().notes}>
                      {(n) => (
                        <div class="fg-card">
                          <NoteView note={n} />
                        </div>
                      )}
                    </For>
                  </div>
                  <Show when={i().assignees.length}>
                    <div class="muted fg-small">Assigned to {i().assignees.map(displayName).join(", ")}</div>
                  </Show>
                  <div class="fg-card fg-new-comment">
                    <CommentEditor
                      placeholder="Leave a comment…"
                      submitLabel="Comment"
                      onSubmit={async (body) => {
                        const ok = await run("Comment failed", () => forgesApi.commentIssue(props.number, body));
                        if (ok) void refetch();
                        return ok;
                      }}
                    />
                  </div>
                </div>
              </div>
            </>
          )}
        </Match>
      </Switch>
    </div>
  );
}
