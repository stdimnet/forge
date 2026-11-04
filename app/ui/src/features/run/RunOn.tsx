/**
 * "Run on" of a run configuration (IntelliJ's run targets): the local machine, a Docker
 * image, a Docker Compose service, an SSH host, a WSL distribution or a Dev Container.
 * Runs, tests, coverage and debug runs of the configuration execute there; the command line
 * and the path mappings are built in Rust (`forge-runtargets`).
 */
import { For, Show } from "solid-js";
import type { RunConfiguration, RunTarget, RunTargetKind } from "@/ipc/run";
import { t as tr } from "@/i18n";

export const TARGET_KINDS: { value: RunTargetKind; label: string }[] = [
  { value: "local", label: tr("run.runOn.localMachine") },
  { value: "dockerImage", label: tr("run.runOn.dockerImage") },
  { value: "composeService", label: tr("run.runOn.dockerComposeService") },
  { value: "ssh", label: "SSH" },
  { value: "wsl", label: "WSL" },
  { value: "devContainer", label: tr("run.runOn.devContainer") },
];

interface TargetField {
  key: keyof RunTarget;
  label: string;
  placeholder?: string;
  kind?: "number" | "list" | "lifecycle";
}

const PROJECT_PATH: TargetField = { key: "projectPath", label: tr("run.runOn.projectPathOnTarget"), placeholder: "/opt/project" };

const FIELDS: Record<RunTargetKind, TargetField[]> = {
  local: [],
  dockerImage: [
    { key: "image", label: tr("run.runOn.image"), placeholder: "node:22" },
    { key: "runOptions", label: tr("run.runOn.dockerRunOptions"), placeholder: "--network host" },
    PROJECT_PATH,
  ],
  composeService: [
    { key: "composeFiles", label: tr("run.runOn.composeFiles"), placeholder: "compose.yaml", kind: "list" },
    { key: "service", label: tr("run.runOn.service"), placeholder: "app" },
    { key: "lifecycle", label: tr("run.runOn.lifecycle"), kind: "lifecycle" },
  ],
  ssh: [
    { key: "host", label: tr("run.runOn.host"), placeholder: "ci.example.com" },
    { key: "port", label: tr("run.runOn.port"), placeholder: "22", kind: "number" },
    { key: "user", label: tr("run.runOn.userName"), placeholder: "deploy" },
    { key: "keyFile", label: tr("run.runOn.privateKeyFile"), placeholder: "~/.ssh/id_ed25519" },
    { ...PROJECT_PATH, placeholder: "/srv/shop" },
  ],
  wsl: [
    { key: "distribution", label: tr("run.runOn.distribution"), placeholder: "Ubuntu" },
    { key: "user", label: tr("run.runOn.userName"), placeholder: tr("run.runOn.defaultUser") },
  ],
  devContainer: [
    { key: "container", label: tr("run.runOn.container"), placeholder: "shop-dev" },
    { key: "user", label: tr("run.runOn.userName"), placeholder: "vscode" },
    { ...PROJECT_PATH, placeholder: "/workspaces/shop" },
  ],
};

/** `local=remote; …` of the target's own path mappings. */
const mappingsText = (t: RunTarget) => (t.mappings ?? []).map((m) => `${m.local}=${m.remote}`).join("; ");

const parseMappings = (text: string) =>
  text
    .split(";")
    .map((p) => p.trim())
    .filter((p) => p.includes("="))
    .map((p) => {
      const i = p.indexOf("=");
      return { local: p.slice(0, i).trim(), remote: p.slice(i + 1).trim() };
    });

function fieldValue(t: RunTarget, f: TargetField): string {
  const v = t[f.key];
  if (Array.isArray(v)) return f.kind === "list" ? (v as string[]).join(", ") : "";
  return v === undefined || v === null || v === 0 ? "" : String(v);
}

/** The "Run on" row and the fields of the chosen target. */
export function RunOnRow(props: {
  cfg: RunConfiguration;
  update: (fn: (cfg: RunConfiguration) => void) => void;
  /** PHP types: a remote CLI interpreter decides where PHP runs. */
  php?: boolean;
}) {
  const target = (): RunTarget => props.cfg.runOn ?? { kind: "local" };
  const set = (patch: Partial<RunTarget>) =>
    props.update((c) => {
      const next: RunTarget = { ...(c.runOn ?? { kind: "local" }), ...patch };
      if (next.kind === "local") delete c.runOn;
      else c.runOn = next;
    });
  const setField = (f: TargetField, value: string) => {
    if (f.kind === "number") set({ [f.key]: Number(value) || undefined } as Partial<RunTarget>);
    else if (f.kind === "list")
      set({
        [f.key]: value
          .split(",")
          .map((x) => x.trim())
          .filter(Boolean),
      } as Partial<RunTarget>);
    else set({ [f.key]: value } as Partial<RunTarget>);
  };
  return (
    <>
      <label class="rc-row">
        <span class="rc-label">{tr("run.runOn.runOn")}</span>
        <select
          class="rc-select"
          data-testid="rc-run-on"
          value={target().kind}
          onChange={(e) => set({ kind: e.currentTarget.value as RunTargetKind })}
        >
          <For each={TARGET_KINDS}>{(k) => <option value={k.value}>{k.label}</option>}</For>
        </select>
      </label>
      <Show when={target().kind !== "local"}>
        <div class="rc-target" data-run-target={target().kind}>
          <For each={FIELDS[target().kind]}>
            {(f) =>
              f.kind === "lifecycle" ? (
                <div class="rc-row">
                  <span class="rc-label">{f.label}:</span>
                  <div class="rc-radios" role="radiogroup">
                    <label class="rc-radio">
                      <input type="radio" name="runOnLifecycle" checked={(target().lifecycle ?? "run") === "run"} onChange={() => set({ lifecycle: "run" })} />
                      <span>{tr("run.runOn.alwaysStartANewContainer")}</span>
                    </label>
                    <label class="rc-radio">
                      <input type="radio" name="runOnLifecycle" checked={target().lifecycle === "exec"} onChange={() => set({ lifecycle: "exec" })} />
                      <span>{tr("run.runOn.connectToExistingContainer")}</span>
                    </label>
                  </div>
                </div>
              ) : (
                <label class="rc-row">
                  <span class="rc-label">{f.label}:</span>
                  <input
                    class="rc-input"
                    name={`runOn.${f.key}`}
                    value={fieldValue(target(), f)}
                    placeholder={f.placeholder}
                    onChange={(e) => setField(f, e.currentTarget.value)}
                  />
                </label>
              )
            }
          </For>
          <label class="rc-row">
            <span class="rc-label">{tr("run.runOn.pathMappings")}</span>
            <input
              class="rc-input"
              name="runOn.mappings"
              value={mappingsText(target())}
              placeholder={tr("run.runOn.automaticOrProjectFolderPath")}
              onChange={(e) => set({ mappings: parseMappings(e.currentTarget.value) })}
            />
          </label>
          <div class="rc-target-note muted">
            Runs, tests and debug runs execute there; paths in arguments, output, test results, coverage and breakpoints are mapped.
            <Show when={props.php}> {tr("run.runOn.aRemotePhpCliInterpreter")}</Show>
          </div>
        </div>
      </Show>
    </>
  );
}
