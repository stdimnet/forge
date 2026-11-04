import { createResource, createSignal, For, Match, Show, Switch } from "solid-js";
import {
  Box,
  FolderOpen,
  FolderPlus,
  GitBranch,
  LayoutGrid,
  Puzzle,
  Search,
  Server,
  Settings,
  SlidersHorizontal,
  SquareTerminal,
  Users,
} from "lucide-solid";
import { workspaceApi, type RecentProject } from "@/ipc/workspace";
import { openWorkspace, pickAndOpenWorkspace } from "@/state/workspace";
import { runCommand } from "@/state/commands";
import { frameworkLabel, isPluginActive } from "@/state/project";
import { ProjectTypeIcon } from "@/shell/ProjectTypeIcon";
import { RemoteDevelopmentPage, SshBadge } from "@/features/remote/RemoteDevelopmentPage";
import { action, separator, showContextMenu } from "@/features/contextMenu/state";
import { openRecentRemote } from "@/features/remote/store";
import { DevContainersPage } from "@/features/devenv/DevContainersPage";
import { WslPage } from "@/features/devenv/WslPage";
import { CodeWithMeWelcomePage } from "@/features/code-with-me/WelcomePage";
import "./welcome.css";

/** Badge color derived from the project name so each project keeps its color. */
function badgeColor(name: string): string {
  const hues = [22, 200, 145, 280, 340, 45, 175];
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return `hsl(${hues[h % hues.length]} 62% 52%)`;
}

const initials = (name: string) =>
  name
    .split(/[-_ .]/)
    .filter(Boolean)
    .map((p) => p[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();

function ProjectRow(props: { project: RecentProject; onError: (message: string) => void }) {
  const open = () =>
    (props.project.remote ? openRecentRemote(props.project) : openWorkspace(props.project.root)).catch((e) =>
      props.onError(String((e as Error)?.message ?? e)),
    );
  return (
    <button
      class="welcome-project"
      classList={{ remote: !!props.project.remote }}
      onClick={() => void open()}
      title={props.project.root}
      data-root={props.project.root}
    >
      <span class="welcome-badge" style={{ background: badgeColor(props.project.name) }}>
        {initials(props.project.name)}
      </span>
      <span class="welcome-project-text">
        <span class="welcome-project-name">
          {props.project.name}
          <Show when={props.project.remote}>
            <SshBadge kind={props.project.remote?.kind} />
          </Show>
        </span>
        <span class="welcome-project-path">{props.project.root}</span>
      </span>
      <Show when={props.project.primary}>
        {(p) => (
          <span class="welcome-project-type" title={`Detected: ${frameworkLabel(p())}`}>
            <ProjectTypeIcon type={p().id} size={18} />
            {frameworkLabel(p())}
          </span>
        )}
      </Show>
    </button>
  );
}

type WelcomePage = "projects" | "remote" | "devcontainers" | "wsl" | "codewithme";

/** `?welcome=remote|devcontainers|wsl|codewithme` opens a Remote Development page (screenshots). */
function initialPage(): WelcomePage {
  const p = new URLSearchParams(location.search).get("welcome");
  return p === "remote" || p === "devcontainers" || p === "wsl" || p === "codewithme" ? p : "projects";
}

const isRemotePage = (p: WelcomePage) => p !== "projects";

/** PHPStorm-style Welcome screen, shown when no project is open. */
export function WelcomeScreen() {
  const [recent] = createResource(() => workspaceApi.recent().catch(() => [] as RecentProject[]));
  const [query, setQuery] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [page, setPage] = createSignal<WelcomePage>(initialPage());
  const row = (p: RecentProject) => <ProjectRow project={p} onError={setError} />;
  const filtered = () =>
    (recent() ?? []).filter((p) => `${p.name} ${p.root}`.toLowerCase().includes(query().toLowerCase()));

  const open = () => pickAndOpenWorkspace().catch((e) => setError(String(e)));
  const newProject = () => void runCommand("NewProject");
  const getFromVcs = () => void runCommand("Vcs.Checkout");

  return (
    <div class="welcome">
      <aside class="welcome-nav">
        <div class="welcome-brand">
          <img src="/forge.svg" alt="" />
          <div>
            <div class="welcome-brand-name">Forge</div>
            <div class="welcome-brand-version">0.1.0 · PHPForge</div>
          </div>
        </div>
        <button class="welcome-nav-item" classList={{ active: page() === "projects" }} onClick={() => setPage("projects")}>
          <LayoutGrid size={15} /> Projects
        </button>
        <button
          class="welcome-nav-item"
          classList={{ open: isRemotePage(page()) }}
          onClick={() => setPage("remote")}
          data-testid="welcome-nav-remote"
        >
          <Server size={15} /> Remote Development
        </button>
        <Show when={isRemotePage(page())}>
          <button
            class="welcome-nav-item welcome-nav-sub"
            classList={{ active: page() === "remote" }}
            onClick={() => setPage("remote")}
          >
            <Server size={14} /> SSH
          </button>
          <button
            class="welcome-nav-item welcome-nav-sub"
            classList={{ active: page() === "devcontainers" }}
            onClick={() => setPage("devcontainers")}
            data-testid="welcome-nav-devcontainers"
          >
            <Box size={14} /> Dev Containers
          </button>
          <button
            class="welcome-nav-item welcome-nav-sub"
            classList={{ active: page() === "wsl" }}
            onClick={() => setPage("wsl")}
            data-testid="welcome-nav-wsl"
          >
            <SquareTerminal size={14} /> WSL
          </button>
          <Show when={isPluginActive("forge.codewithme")}>
            <button
              class="welcome-nav-item welcome-nav-sub"
              classList={{ active: page() === "codewithme" }}
              onClick={() => setPage("codewithme")}
              data-testid="welcome-nav-codewithme"
            >
              <Users size={14} /> Code With Me
            </button>
          </Show>
        </Show>
        <button class="welcome-nav-item" onClick={() => runCommand("settings.open")}>
          <SlidersHorizontal size={15} /> Customize
        </button>
        <button class="welcome-nav-item" onClick={() => runCommand("settings.open")}>
          <Puzzle size={15} /> Plugins
        </button>
        <div class="welcome-nav-foot">
          <button
            class="icon-btn"
            title="Manage IDE Settings"
            data-testid="welcome-manage"
            onClick={(e) => {
              const r = e.currentTarget.getBoundingClientRect();
              showContextMenu(
                [
                  action("ShowSettings", "Settings..."),
                  action("TemplateProjectProperties", "Settings for New Projects..."),
                  separator,
                  action("ImportSettings", "Import Settings..."),
                  action("ExportSettings", "Export Settings..."),
                  action("RestoreDefaultSettings", "Restore Default Settings..."),
                  action("SettingsSync.Open", "Settings Sync..."),
                  separator,
                  action("InvalidateCaches", "Invalidate Caches..."),
                ],
                r.left,
                r.top - 8,
              );
            }}
          >
            <Settings size={16} />
          </button>
        </div>
      </aside>
      <main class="welcome-main">
        <Switch>
          <Match when={page() === "remote"}>
            <RemoteDevelopmentPage recent={recent() ?? []} row={row} />
          </Match>
          <Match when={page() === "devcontainers"}>
            <DevContainersPage onError={setError} />
          </Match>
          <Match when={page() === "codewithme"}>
            <CodeWithMeWelcomePage />
          </Match>
          <Match when={page() === "wsl"}>
            <WslPage recent={recent() ?? []} row={row} onError={setError} />
          </Match>
          <Match when={page() === "projects"}>
            <Show
              when={(recent() ?? []).length > 0}
              fallback={
                <div class="welcome-empty">
                  <h1>Welcome to Forge</h1>
                  <p class="muted">Create a new project to start from scratch.</p>
                  <p class="muted">Open existing project from disk or version control.</p>
                  <div class="welcome-big-actions">
                    <button class="welcome-big" onClick={newProject}>
                      <FolderPlus size={22} />
                      <span>New Project</span>
                    </button>
                    <button class="welcome-big" onClick={open}>
                      <FolderOpen size={22} />
                      <span>Open</span>
                    </button>
                    <button class="welcome-big" onClick={getFromVcs} data-testid="welcome-vcs">
                      <GitBranch size={22} />
                      <span>Get from VCS</span>
                    </button>
                  </div>
                </div>
              }
            >
              <div class="welcome-toolbar">
                <label class="welcome-search">
                  <Search size={14} class="muted" />
                  <input
                    placeholder="Search projects"
                    value={query()}
                    onInput={(e) => setQuery(e.currentTarget.value)}
                  />
                </label>
                <button class="btn-secondary" onClick={newProject}>
                  New Project
                </button>
                <button class="btn-primary" onClick={open}>Open</button>
                <button class="btn-secondary" onClick={getFromVcs} data-testid="welcome-vcs">
                  Get from VCS
                </button>
              </div>
              <div class="welcome-list">
                <For each={filtered()}>{(p) => row(p)}</For>
              </div>
            </Show>
          </Match>
        </Switch>
        <Show when={error()}>
          <div class="welcome-error">{error()}</div>
        </Show>
      </main>
    </div>
  );
}
