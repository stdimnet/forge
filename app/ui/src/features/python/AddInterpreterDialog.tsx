/**
 * "Add Python Interpreter", PyCharm style: a list of targets on the left (Local
 * Interpreter, Docker, Docker Compose, SSH, WSL) and the target's form on the right. A
 * local interpreter is either generated (Virtualenv, uv, Poetry or conda: the commands
 * come from Rust and run in the Run tool window) or an existing one (detected
 * environments and system Pythons, or a path). The result goes back to the caller, which
 * stores it (status bar popup: right away; Settings: in the draft until Apply).
 */
import { createResource, createSignal, For, Match, onCleanup, Show, Switch, type JSX } from "solid-js";
import { createStore } from "solid-js/store";
import { Portal } from "solid-js/web";
import { Box, Container, Monitor, Server, Terminal } from "lucide-solid";
import { pythonApi, type DetectedPython, type EnvKind, type PythonInterpreter } from "@/ipc/python";
import { remoteApi } from "@/ipc/remote";
import { interpretersApi } from "@/features/php/interpreters";
import { suspendKeymap } from "@/keymap";
import { showBalloon } from "@/keymap";
import { chrome } from "@/shell/window/chrome";
import { workspace } from "@/state/workspace";
import { commandName, emptyInterpreter, KIND_LABELS, labelOf, runCommandLine } from "./state";

type Target = "local" | "docker" | "compose" | "ssh" | "wsl";

/** What the dialog produced: the value of `python.interpreter`, and a new configured interpreter. */
export interface AddResult {
  select: string;
  add?: PythonInterpreter;
}

const TARGETS: { id: Target; label: string; icon: (p: { size: number }) => JSX.Element }[] = [
  { id: "local", label: "Local Interpreter", icon: (p) => <Monitor size={p.size} /> },
  { id: "docker", label: "Docker", icon: (p) => <Box size={p.size} /> },
  { id: "compose", label: "Docker Compose", icon: (p) => <Container size={p.size} /> },
  { id: "ssh", label: "SSH", icon: (p) => <Server size={p.size} /> },
  { id: "wsl", label: "WSL", icon: (p) => <Terminal size={p.size} /> },
];

const NEW_KINDS: { id: EnvKind; label: string }[] = [
  { id: "venv", label: "Virtualenv" },
  { id: "uv", label: "uv" },
  { id: "poetry", label: "Poetry" },
  { id: "conda", label: "Conda" },
];

function Row(props: { label: string; for?: string; children: JSX.Element }) {
  return (
    <div class="py-field">
      <label for={props.for}>{props.label}</label>
      <div class="py-field-value">{props.children}</div>
    </div>
  );
}

const newId = () => `py-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;

export function AddInterpreterDialog(props: { onDone: (r: AddResult) => void; onClose: () => void; initialTarget?: Target }) {
  const release = suspendKeymap();
  onCleanup(release);
  const [target, setTarget] = createSignal<Target>(props.initialTarget ?? "local");
  const [mode, setMode] = createSignal<"new" | "existing">("new");
  const [busy, setBusy] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [detected] = createResource(() => pythonApi.detect().catch((): DetectedPython[] => []));
  const bases = () => (detected() ?? []).filter((d) => d.kind === "system" || d.kind === "pyenv");
  const root = () => workspace()?.root ?? "";
  const [local, setLocal] = createStore({
    kind: "venv" as EnvKind,
    base: "",
    pythonVersion: "",
    location: ".venv",
    installDependencies: true,
    existing: "",
  });
  const [remote, setRemote] = createStore({
    name: "",
    image: "python:3.12-slim",
    composeFiles: "",
    service: "",
    lifecycle: "run" as "run" | "exec",
    sshConfig: "",
    distribution: "",
    path: "python3",
  });
  const windows = () => chrome()?.os === "windows" || /Windows/.test(typeof navigator === "undefined" ? "" : navigator.userAgent);
  const [services] = createResource(
    () => (target() === "compose" ? remote.composeFiles : null),
    (files) => interpretersApi.composeServices(files.split(";").map((f) => f.trim()).filter(Boolean)).catch((): string[] => []),
  );
  const [ssh] = createResource(() => (target() === "ssh" ? true : null), () => remoteApi.configs().catch(() => []));
  const [distributions] = createResource(() => (target() === "wsl" ? true : null), () => interpretersApi.wslDistributions().catch((): string[] => []));

  const remoteName = (): string => {
    if (remote.name.trim()) return remote.name.trim();
    switch (target()) {
      case "docker":
        return `Docker ${remote.image}`;
      case "compose":
        return `Docker Compose ${remote.service || "service"}`;
      case "ssh": {
        const c = (ssh() ?? []).find((x) => x.id === remote.sshConfig);
        return `SSH ${c?.label ?? "host"}`;
      }
      default:
        return `WSL ${remote.distribution || "default"}`;
    }
  };

  const valid = () => {
    if (busy()) return false;
    switch (target()) {
      case "local":
        return mode() === "new" ? local.kind === "poetry" || !!local.location.trim() : !!local.existing.trim();
      case "docker":
        return !!remote.image.trim();
      case "compose":
        return !!remote.service.trim();
      case "ssh":
        return !!remote.sshConfig;
      default:
        return true;
    }
  };

  async function generate(): Promise<void> {
    setError(null);
    const plan = await pythonApi.environmentPlan({
      kind: local.kind,
      location: local.location,
      baseInterpreter: local.base,
      pythonVersion: local.pythonVersion,
      installDependencies: local.installDependencies,
    });
    for (const step of plan.steps) {
      setBusy(commandName(step));
      const code = await runCommandLine(commandName(step), step);
      if (code !== 0) throw new Error(`'${commandName(step)}' failed${code === null ? "" : ` with exit code ${code}`}. See the Run tool window.`);
    }
    let python = plan.python;
    if (!python) {
      // Poetry chose the folder: the project's environments now include it.
      const found = await pythonApi.detect().catch((): DetectedPython[] => []);
      python = found.find((d) => d.project && d.kind === "poetry")?.path ?? "";
    }
    if (!python) throw new Error("The new environment was not found");
    props.onDone({ select: python });
  }

  async function ok(): Promise<void> {
    if (!valid()) return;
    try {
      if (target() === "local") {
        if (mode() === "new") {
          setBusy("Creating environment…");
          await generate();
        } else {
          props.onDone({ select: local.existing.trim() });
        }
        return;
      }
      const name = remoteName();
      const add: PythonInterpreter = {
        ...emptyInterpreter(),
        id: newId(),
        name,
        kind: target() as EnvKind,
        path: remote.path.trim() || "python3",
        image: target() === "docker" ? remote.image.trim() : "",
        composeFiles: target() === "compose" ? remote.composeFiles.split(";").map((f) => f.trim()).filter(Boolean) : [],
        service: target() === "compose" ? remote.service : "",
        lifecycle: remote.lifecycle,
        sshConfig: target() === "ssh" ? remote.sshConfig : "",
        distribution: target() === "wsl" ? remote.distribution : "",
        project: true,
      };
      // Reload: the version, where Docker / SSH can answer.
      setBusy("Checking the interpreter…");
      const info = await pythonApi.info(add).catch((e) => {
        showBalloon(`Python interpreter '${name}'`, `Saved without checking it: ${String(e)}`, 5000);
        return null;
      });
      if (info) add.version = info.version;
      props.onDone({ select: name, add });
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(null);
    }
  }

  const detectedLabel = (d: DetectedPython) => `${labelOf(d)}  ${d.path}`;

  return (
    <Portal>
      <div class="popup-backdrop modal" onMouseDown={(e) => e.target === e.currentTarget && !busy() && props.onClose()}>
        <div
          class="dialog py-dialog"
          role="dialog"
          data-testid="python-add-interpreter"
          style={{ width: "min(760px, calc(100vw - 32px))" }}
          onKeyDown={(e) => {
            if (e.key === "Escape" && !busy()) props.onClose();
            else if (e.key === "Enter" && !(e.target instanceof HTMLSelectElement)) void ok();
            else return;
            e.preventDefault();
            e.stopPropagation();
          }}
        >
          <div class="dialog-title">Add Python Interpreter</div>
          <div class="py-dialog-body">
            <div class="py-dialog-targets">
              <For each={TARGETS}>
                {(t) => (
                  <button
                    class="tree-row py-dialog-target"
                    classList={{ selected: target() === t.id, disabled: t.id === "wsl" && !windows() }}
                    disabled={t.id === "wsl" && !windows()}
                    title={t.id === "wsl" && !windows() ? "WSL is available on Windows" : undefined}
                    onClick={() => setTarget(t.id)}
                  >
                    {t.icon({ size: 14 })}
                    <span>{t.label}</span>
                  </button>
                )}
              </For>
            </div>
            <div class="py-dialog-form">
              <Switch>
                <Match when={target() === "local"}>
                  <div class="py-radios">
                    <label>
                      <input type="radio" name="py-env-mode" checked={mode() === "new"} onChange={() => setMode("new")} />
                      Generate new
                    </label>
                    <label>
                      <input type="radio" name="py-env-mode" checked={mode() === "existing"} onChange={() => setMode("existing")} />
                      Select existing
                    </label>
                  </div>
                  <Show
                    when={mode() === "new"}
                    fallback={
                      <Row label="Interpreter:" for="py-existing">
                        <select id="py-existing" value={local.existing} onChange={(e) => setLocal("existing", e.currentTarget.value)}>
                          <option value="">Choose an interpreter…</option>
                          <For each={detected() ?? []}>
                            {(d) => (
                              <option value={d.path}>
                                {detectedLabel(d)} [{KIND_LABELS[d.kind]}]
                              </option>
                            )}
                          </For>
                        </select>
                        <input
                          type="text"
                          class="py-path-input"
                          placeholder="or the path of a python executable"
                          value={local.existing}
                          onInput={(e) => setLocal("existing", e.currentTarget.value)}
                        />
                      </Row>
                    }
                  >
                    <Row label="Type:" for="py-new-kind">
                      <select id="py-new-kind" value={local.kind} onChange={(e) => setLocal("kind", e.currentTarget.value as EnvKind)}>
                        <For each={NEW_KINDS}>{(k) => <option value={k.id}>{k.label}</option>}</For>
                      </select>
                    </Row>
                    <Show when={local.kind !== "conda"}>
                      <Row label="Base python:" for="py-base">
                        <select id="py-base" value={local.base} onChange={(e) => setLocal("base", e.currentTarget.value)}>
                          <option value="">{local.kind === "uv" ? "uv's default Python" : "python3"}</option>
                          <For each={bases()}>{(d) => <option value={d.path}>{detectedLabel(d)}</option>}</For>
                        </select>
                      </Row>
                    </Show>
                    <Show when={local.kind === "conda" || (local.kind === "uv" && !local.base)}>
                      <Row label="Python version:" for="py-version">
                        <input id="py-version" type="text" placeholder="3.12" value={local.pythonVersion} onInput={(e) => setLocal("pythonVersion", e.currentTarget.value)} />
                      </Row>
                    </Show>
                    <Show when={local.kind !== "poetry"} fallback={<div class="py-note muted">Poetry creates the environment in its own folder (or .venv with virtualenvs.in-project).</div>}>
                      <Row label="Location:" for="py-location">
                        <input id="py-location" type="text" value={local.location} onInput={(e) => setLocal("location", e.currentTarget.value)} />
                      </Row>
                      <div class="py-note muted">{local.location.startsWith("/") || /^[A-Za-z]:/.test(local.location) ? local.location : `${root()}/${local.location}`}</div>
                    </Show>
                    <label class="py-check">
                      <input type="checkbox" checked={local.installDependencies} onChange={(e) => setLocal("installDependencies", e.currentTarget.checked)} />
                      Install the project's dependencies (requirements.txt, pyproject.toml)
                    </label>
                  </Show>
                </Match>
                <Match when={target() === "docker"}>
                  <Row label="Image name:" for="py-image">
                    <input id="py-image" type="text" placeholder="python:3.12-slim" value={remote.image} onInput={(e) => setRemote("image", e.currentTarget.value)} />
                  </Row>
                  <div class="py-note muted">The project is mounted at /opt/project.</div>
                </Match>
                <Match when={target() === "compose"}>
                  <Row label="Configuration files:" for="py-compose-files">
                    <input id="py-compose-files" type="text" placeholder="./compose.yaml" value={remote.composeFiles} onChange={(e) => setRemote("composeFiles", e.currentTarget.value)} />
                  </Row>
                  <Row label="Service:" for="py-service">
                    <select id="py-service" value={remote.service} onChange={(e) => setRemote("service", e.currentTarget.value)}>
                      <option value="">{(services() ?? []).length ? "Choose a service…" : "No services found"}</option>
                      <For each={services() ?? []}>{(s) => <option value={s}>{s}</option>}</For>
                    </select>
                  </Row>
                  <Row label="Lifecycle:">
                    <div class="py-radios column">
                      <label>
                        <input type="radio" name="py-lifecycle" checked={remote.lifecycle === "run"} onChange={() => setRemote("lifecycle", "run")} />
                        Always start a new container ('docker compose run')
                      </label>
                      <label>
                        <input type="radio" name="py-lifecycle" checked={remote.lifecycle === "exec"} onChange={() => setRemote("lifecycle", "exec")} />
                        Connect to existing container ('docker compose exec')
                      </label>
                    </div>
                  </Row>
                </Match>
                <Match when={target() === "ssh"}>
                  <Row label="SSH configuration:" for="py-ssh">
                    <select id="py-ssh" value={remote.sshConfig} onChange={(e) => setRemote("sshConfig", e.currentTarget.value)}>
                      <option value="">{(ssh() ?? []).length ? "Choose a configuration…" : "No SSH configurations (Settings | Tools | SSH Configurations)"}</option>
                      <For each={ssh() ?? []}>{(c) => <option value={c.id}>{c.name || c.label}</option>}</For>
                    </select>
                  </Row>
                </Match>
                <Match when={target() === "wsl"}>
                  <Row label="Linux distribution:" for="py-wsl">
                    <select id="py-wsl" value={remote.distribution} onChange={(e) => setRemote("distribution", e.currentTarget.value)}>
                      <option value="">Default</option>
                      <For each={distributions() ?? []}>{(d) => <option value={d}>{d}</option>}</For>
                    </select>
                  </Row>
                </Match>
              </Switch>
              <Show when={target() !== "local"}>
                <Row label="Python interpreter path:" for="py-remote-path">
                  <input id="py-remote-path" type="text" placeholder="python3" value={remote.path} onInput={(e) => setRemote("path", e.currentTarget.value)} />
                </Row>
                <Row label="Name:" for="py-remote-name">
                  <input id="py-remote-name" type="text" placeholder={remoteName()} value={remote.name} onInput={(e) => setRemote("name", e.currentTarget.value)} />
                </Row>
              </Show>
              <Show when={error()}>
                <div class="dialog-error">{error()}</div>
              </Show>
            </div>
          </div>
          <div class="dialog-buttons">
            <Show when={busy()}>
              <span class="muted py-busy">{busy()}</span>
            </Show>
            <button class="btn-secondary" disabled={!!busy()} onClick={props.onClose}>
              Cancel
            </button>
            <button class="btn-primary" disabled={!valid()} onClick={() => void ok()} data-testid="python-add-interpreter-ok">
              OK
            </button>
          </div>
        </div>
      </div>
    </Portal>
  );
}
