import { createResource, createSignal, For, Show } from "solid-js";
import { FolderOpen, FolderPlus, GitBranch, LayoutGrid, Puzzle, Search, Settings, SlidersHorizontal } from "lucide-solid";
import { workspaceApi, type RecentProject } from "@/ipc/workspace";
import { openWorkspace, pickAndOpenWorkspace } from "@/state/workspace";
import { runCommand } from "@/state/commands";
import { frameworkLabel } from "@/state/project";
import { ProjectTypeIcon } from "@/shell/ProjectTypeIcon";
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

function ProjectRow(props: { project: RecentProject }) {
  return (
    <button class="welcome-project" onClick={() => openWorkspace(props.project.root)} title={props.project.root}>
      <span class="welcome-badge" style={{ background: badgeColor(props.project.name) }}>
        {initials(props.project.name)}
      </span>
      <span class="welcome-project-text">
        <span class="welcome-project-name">{props.project.name}</span>
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

/** PHPStorm-style Welcome screen, shown when no project is open. */
export function WelcomeScreen() {
  const [recent] = createResource(() => workspaceApi.recent().catch(() => [] as RecentProject[]));
  const [query, setQuery] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const filtered = () =>
    (recent() ?? []).filter((p) => `${p.name} ${p.root}`.toLowerCase().includes(query().toLowerCase()));

  const open = () => pickAndOpenWorkspace().catch((e) => setError(String(e)));
  const newProject = () => void runCommand("NewProject");

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
        <button class="welcome-nav-item active">
          <LayoutGrid size={15} /> Projects
        </button>
        <button class="welcome-nav-item" onClick={() => runCommand("settings.open")}>
          <SlidersHorizontal size={15} /> Customize
        </button>
        <button class="welcome-nav-item" onClick={() => runCommand("settings.open")}>
          <Puzzle size={15} /> Plugins
        </button>
        <div class="welcome-nav-foot">
          <button class="icon-btn" title="Settings" onClick={() => runCommand("settings.open")}>
            <Settings size={16} />
          </button>
        </div>
      </aside>
      <main class="welcome-main">
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
                <button class="welcome-big" disabled title="Coming soon">
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
            <button class="btn-secondary" disabled title="Coming soon">Get from VCS</button>
          </div>
          <div class="welcome-list">
            <For each={filtered()}>{(p) => <ProjectRow project={p} />}</For>
          </div>
        </Show>
        <Show when={error()}>
          <div class="welcome-error">{error()}</div>
        </Show>
      </main>
    </div>
  );
}
