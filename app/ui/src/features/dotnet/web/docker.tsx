/**
 * Docker for .NET projects, like Rider's "Add Docker Support": a multi-stage Dockerfile
 * next to the project and a `.dockerignore` in the solution folder (generated in Rust,
 * `dotnet_dockerfile`), and the ".NET Docker" run configuration: `docker build` from the
 * solution folder in the Run tool window, then `docker run` with the published ports. The
 * container shows up in the Services tool window like any other.
 */
import { createResource, createSignal, For, onCleanup, Show } from "solid-js";
import { render } from "solid-js/web";
import { Container } from "lucide-solid";
import { dotnetWebApi, type GeneratedDocker } from "@/ipc/dotnetWeb";
import { runApi, type RunConfiguration } from "@/ipc/run";
import { workspaceApi } from "@/ipc/workspace";
import { registerRunConfigurationType } from "@/features/run/types";
import { useTemporary } from "@/features/run/configStore";
import { openFileAt } from "@/editor/views";
import { suspendKeymap } from "@/keymap";
import { isDialogWindow } from "@/shell/window/mode";
import { dotnetWorkspace, errorText, PLUGIN, runDotnetCommand, runnableProjects, showDotnetBalloon } from "../state";

const opt = (cfg: RunConfiguration, key: string) => cfg.options?.[key] ?? "";

registerRunConfigurationType({
  id: "dotnet-docker",
  name: ".NET Docker",
  description: "Build a .NET project's Dockerfile from the solution folder (docker build) and run the image as a container with published ports and environment.",
  icon: Container,
  color: "var(--info)",
  pluginId: PLUGIN,
  argumentsLabel: "Container arguments",
  noRunTarget: true,
  fields: [
    { key: "dockerfile", label: "Dockerfile", kind: "path", required: true, placeholder: "src/Bookstore.Api/Dockerfile" },
    { key: "context", label: "Build context", placeholder: ". (the solution folder)" },
    { key: "image", label: "Image tag", placeholder: "bookstore.api (from the project folder)" },
    { key: "container", label: "Container name", placeholder: "bookstore-api" },
    { key: "ports", label: "Ports (host:container)", placeholder: "8080:8080" },
    { key: "env", label: "Environment", placeholder: "ASPNETCORE_ENVIRONMENT=Development" },
    { key: "buildArgs", label: "docker build options", placeholder: "--build-arg BUILD_CONFIGURATION=Debug" },
    { key: "runArgs", label: "docker run options", placeholder: "--network bookstore" },
  ],
  suggestName: (cfg) => {
    const dir = opt(cfg, "dockerfile").split("/").slice(-2, -1)[0];
    return dir ? `${dir} (Docker)` : undefined;
  },
  start: async (cfg, request) => {
    // `docker build` first, in its own tab; the configuration's process is `docker run`.
    const build = await dotnetWebApi.dockerBuild(cfg);
    const code = await runDotnetCommand(build, `docker build ${opt(cfg, "image") || opt(cfg, "dockerfile")}`);
    if (code !== 0) throw new Error("docker build failed; see its tab in the Run tool window");
    return runApi.start(request);
  },
});

// ---- Add Docker Support -------------------------------------------------------------------------------

const [open, setOpen] = createSignal<string | null>(null);

/** Open "Add Docker Support" for a project (default: the first web project). */
export function openAddDockerSupport(project?: string): void {
  const runnable = runnableProjects();
  const p = project ?? runnable.find((x) => x.web)?.path ?? runnable[0]?.path;
  if (!p) {
    showDotnetBalloon("No application project", "Docker support needs a web or console application project.", 6000);
    return;
  }
  setOpen(p);
}

function Dialog(props: { project: string }) {
  const release = suspendKeymap();
  onCleanup(release);
  const [project, setProject] = createSignal(props.project);
  const [port, setPort] = createSignal("8080");
  const [overwrite, setOverwrite] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [generated] = createResource(
    () => [project(), Number(port()) || 8080] as const,
    ([p, n]) => dotnetWebApi.dockerfile(p, n).catch((e) => ({ error: errorText(e) }) as unknown as GeneratedDocker & { error: string }),
  );
  const error = () => (generated() as { error?: string } | undefined)?.error;
  const close = () => setOpen(null);
  const create = async () => {
    const g = generated();
    if (!g || error()) return;
    setBusy(true);
    try {
      if (!g.dockerfileExists || overwrite()) await workspaceApi.writeFile(g.dockerfilePath, g.dockerfile);
      if (!g.dockerignoreExists) await workspaceApi.writeFile(g.dockerignorePath, g.dockerignore);
      const name = dotnetWorkspace()?.projects.find((x) => x.path === project())?.name ?? g.image;
      useTemporary({
        name: `${name} (Docker)`,
        type: "dotnet-docker",
        options: { dockerfile: g.dockerfilePath, context: g.context, image: g.image, ports: `${g.port}:${g.port}`, env: "ASPNETCORE_ENVIRONMENT=Development" },
      });
      close();
      await openFileAt(g.dockerfilePath, 1);
      showDotnetBalloon("Docker support added", `${g.dockerfilePath} and the '${name} (Docker)' run configuration`, 6000);
    } catch (e) {
      showDotnetBalloon("Cannot write the Dockerfile", errorText(e), 8000);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        class="dialog dn-docker-dialog"
        style={{ width: "min(760px, calc(100vw - 32px))" }}
        role="dialog"
        data-testid="dotnet-docker-dialog"
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            close();
          }
        }}
      >
        <div class="dialog-title">Add Docker Support</div>
        <div class="dialog-body">
          <div class="dn-ef-form">
            <label class="form-label">Project</label>
            <select value={project()} onChange={(e) => setProject(e.currentTarget.value)}>
              <For each={runnableProjects()}>{(p) => <option value={p.path}>{p.name}</option>}</For>
            </select>
            <label class="form-label">Container port</label>
            <input value={port()} inputMode="numeric" onInput={(e) => setPort(e.currentTarget.value.replace(/\D/g, ""))} />
          </div>
          <Show when={error()}>
            <div class="dn-message dn-error">{error()}</div>
          </Show>
          <Show when={!error() && generated()}>
            {(g) => (
              <>
                <div class="muted dn-docker-path">
                  {g().dockerfilePath}
                  <Show when={g().dockerfileExists}> · exists</Show> · build context: the solution folder · image <b>{g().image}</b>
                </div>
                <pre class="dn-docker-preview" data-testid="dockerfile-preview">
                  {g().dockerfile}
                </pre>
                <Show when={g().dockerfileExists}>
                  <label class="dn-check">
                    <input type="checkbox" checked={overwrite()} onChange={(e) => setOverwrite(e.currentTarget.checked)} />
                    Replace the existing Dockerfile
                  </label>
                </Show>
              </>
            )}
          </Show>
        </div>
        <div class="dialog-buttons">
          <button class="btn-secondary" onClick={close}>
            Cancel
          </button>
          <button class="btn-primary" disabled={busy() || !generated() || !!error() || (!!generated()?.dockerfileExists && !overwrite())} onClick={() => void create()}>
            Add
          </button>
        </div>
      </div>
    </div>
  );
}

if (typeof document !== "undefined" && !isDialogWindow()) {
  const host = document.createElement("div");
  host.id = "dotnet-docker-overlay";
  document.body.appendChild(host);
  render(() => <Show when={open()}>{(p) => <Dialog project={p()} />}</Show>, host);
}
