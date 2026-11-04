/**
 * Settings | Build, Execution, Deployment | Docker (connection incl. SSH hosts and Docker
 * contexts, Test Connection), | Docker | Docker Registry (accounts; passwords in the OS
 * keychain) and | Kubernetes (with the kubectl of each remote project host).
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { CircleCheck, CircleX, LoaderCircle, Plus, Trash2 } from "lucide-solid";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import { servicesApi } from "@/ipc/services";
import { workspace } from "@/state/workspace";
import { t } from "@/i18n";

function TestConnection() {
  const [state, setState] = createSignal<{ ok: boolean; text: string } | "testing" | null>(null);
  const test = async () => {
    setState("testing");
    try {
      const text = await servicesApi.testConnection({
        connection: draftValue<string>("docker.connection") ?? "auto",
        socketPath: draftValue<string>("docker.socketPath") ?? "",
        tcpUrl: draftValue<string>("docker.tcp.url") ?? "",
        tls: draftValue<boolean>("docker.tcp.tls") ?? false,
        certPath: draftValue<string>("docker.tcp.certPath") ?? "",
        sshUrl: draftValue<string>("docker.ssh.url") ?? "",
        sshSocket: draftValue<string>("docker.ssh.socketPath") ?? "",
        context: draftValue<string>("docker.context") ?? "",
      });
      setState({ ok: true, text });
    } catch (err) {
      setState({ ok: false, text: String(err).replace(/^Error: /, "") });
    }
  };
  return (
    <div class="svc-test">
      <button class="btn-secondary" onClick={() => void test()} data-testid="docker-test-connection">
        Test Connection
      </button>
      <Show when={state()}>
        {(s) => (
          <Show when={s() !== "testing"} fallback={<LoaderCircle size={14} class="svc-spin" />}>
            <span class="svc-test-result" classList={{ ok: (s() as { ok: boolean }).ok, failed: !(s() as { ok: boolean }).ok }}>
              {(s() as { ok: boolean }).ok ? <CircleCheck size={14} /> : <CircleX size={14} />}
              {(s() as { text: string }).text}
            </span>
          </Show>
        )}
      </Show>
    </div>
  );
}

/** Docker CLI contexts (`docker context ls`) as connections: Use picks one. */
function DockerContexts() {
  const [contexts] = createResource(() => servicesApi.dockerContexts().catch(() => []));
  const inUse = (name: string) => draftValue<string>("docker.connection") === "context" && draftValue<string>("docker.context") === name;
  return (
    <Show when={contexts()?.length}>
      <section class="set-section svc-contexts" data-testid="docker-contexts">
        <div class="set-section-title">
          <span>Docker Contexts</span>
        </div>
        <div class="set-section-body indented">
          <p class="muted">Contexts of the Docker CLI (docker context ls). Use connects through one of them.</p>
          <table class="svc-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Docker endpoint</th>
                <th>Description</th>
                <th />
              </tr>
            </thead>
            <tbody>
              <For each={contexts()}>
                {(c) => (
                  <tr>
                    <td>
                      {c.name}
                      <Show when={c.current}>
                        <span class="muted"> (current)</span>
                      </Show>
                    </td>
                    <td>{c.host}</td>
                    <td class="muted">{c.description ?? ""}</td>
                    <td class="svc-cell-actions">
                      <Show when={!inUse(c.name)} fallback={<span class="svc-in-use">In use</span>}>
                        <button
                          class="btn-secondary svc-small-btn"
                          onClick={() => {
                            setDraft("docker.connection", "context");
                            setDraft("docker.context", c.name);
                          }}
                        >
                          Use
                        </button>
                      </Show>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </section>
    </Show>
  );
}

function DockerPage() {
  return (
    <>
      <SchemaPage pageId="build.docker" sectionHeader={(s) => (s === "Connection" ? <TestConnection /> : null)} />
      <DockerContexts />
    </>
  );
}

const normalize = (server: string) => {
  const s = server.trim().replace(/^https?:\/\//, "").replace(/\/+$/, "");
  return ["", "docker.io", "index.docker.io", "index.docker.io/v1", "registry-1.docker.io"].includes(s) ? "docker.io" : s;
};

function RegistryPage() {
  const accounts = () =>
    (draftValue<string[]>("docker.registries") ?? [])
      .map((line) => line.split("\t"))
      .filter((p) => p.length >= 2)
      .map(([server, user]) => ({ server, user }));
  const [stored, { refetch }] = createResource(
    () => accounts().map((a) => a.server).join("\n"),
    (key) => (key ? servicesApi.registryStatus(key.split("\n")).catch(() => []) : Promise.resolve([] as boolean[])),
  );
  const [server, setServer] = createSignal("docker.io");
  const [user, setUser] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const add = async () => {
    const s = normalize(server());
    if (!user().trim()) return;
    try {
      if (password()) await servicesApi.registryLogin(s, password());
      const rest = accounts().filter((a) => a.server !== s);
      setDraft("docker.registries", [...rest, { server: s, user: user().trim() }].map((a) => `${a.server}\t${a.user}`));
      setUser("");
      setPassword("");
      setError(null);
      void refetch();
    } catch (err) {
      setError(String(err).replace(/^Error: /, ""));
    }
  };
  const remove = async (s: string) => {
    await servicesApi.registryLogout(s).catch(() => undefined);
    setDraft(
      "docker.registries",
      accounts()
        .filter((a) => a.server !== s)
        .map((a) => `${a.server}\t${a.user}`),
    );
  };
  return (
    <div class="set-page svc-registries">
      <p class="muted">Accounts used to pull images. Passwords and access tokens are stored in the OS keychain, never in settings files.</p>
      <table class="svc-table">
        <thead>
          <tr>
            <th>Registry</th>
            <th>User</th>
            <th>Password</th>
            <th />
          </tr>
        </thead>
        <tbody>
          <For each={accounts()} fallback={<tr><td colSpan={4} class="muted">No registry accounts. Public images are pulled anonymously.</td></tr>}>
            {(a, i) => (
              <tr>
                <td>{a.server}</td>
                <td>{a.user}</td>
                <td>{stored()?.[i()] ? "Stored in keychain" : <span class="muted">Not set</span>}</td>
                <td>
                  <button class="icon-btn" title="Remove" onClick={() => void remove(a.server)}>
                    <Trash2 size={13} />
                  </button>
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <div class="svc-registry-form">
        <input placeholder="Registry (docker.io, ghcr.io, …)" value={server()} onInput={(e) => setServer(e.currentTarget.value)} spellcheck={false} />
        <input placeholder="User name" value={user()} onInput={(e) => setUser(e.currentTarget.value)} spellcheck={false} />
        <input placeholder="Password or access token" type="password" value={password()} onInput={(e) => setPassword(e.currentTarget.value)} />
        <button class="btn-secondary" onClick={() => void add()} disabled={!user().trim()}>
          <Plus size={13} /> Add
        </button>
      </div>
      <Show when={error()}>
        <div class="svc-test-result failed">{error()}</div>
      </Show>
    </div>
  );
}

/** `kubernetes.hostKubectlPaths`: the kubectl of each remote project host (`host<TAB>path`). */
function HostKubectl() {
  const entries = () =>
    (draftValue<string[]>("kubernetes.hostKubectlPaths") ?? [])
      .map((line) => line.split("\t"))
      .filter((p) => p.length >= 2)
      .map(([host, path]) => ({ host, path }));
  const current = () => workspace()?.remote?.label;
  const [host, setHost] = createSignal(current() ?? "");
  const [path, setPath] = createSignal("");
  const save = (list: { host: string; path: string }[]) =>
    setDraft(
      "kubernetes.hostKubectlPaths",
      list.map((e) => `${e.host}\t${e.path}`),
    );
  const add = () => {
    const h = host().trim();
    if (!h || !path().trim()) return;
    save([...entries().filter((e) => e.host !== h), { host: h, path: path().trim() }]);
    setPath("");
  };
  return (
    <section class="set-section svc-host-kubectl" data-testid="kubectl-hosts">
      <div class="set-section-title">
        <span>kubectl from the Project Host</span>
      </div>
      <div class="svc-hosts-body">
        <p class="muted">
          In remote projects kubectl runs on the project's host with the host's kubeconfig. Hosts without an entry use kubectl from their PATH.
        </p>
        <table class="svc-table">
          <thead>
            <tr>
              <th>Host</th>
              <th>kubectl path on the host</th>
              <th />
            </tr>
          </thead>
          <tbody>
            <For
              each={entries()}
              fallback={
                <tr>
                  <td colSpan={3} class="muted">
                    No hosts. kubectl from the host's PATH is used.
                  </td>
                </tr>
              }
            >
              {(e) => (
                <tr>
                  <td>
                    {e.host}
                    <Show when={e.host === current()}>
                      <span class="muted"> (this project)</span>
                    </Show>
                  </td>
                  <td class="mono">{e.path}</td>
                  <td>
                    <button class="icon-btn" title="Remove" onClick={() => save(entries().filter((x) => x.host !== e.host))}>
                      <Trash2 size={13} />
                    </button>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
        <div class="svc-registry-form">
          <input placeholder="user@host" value={host()} onInput={(e) => setHost(e.currentTarget.value)} spellcheck={false} />
          <input placeholder="/usr/local/bin/kubectl" value={path()} onInput={(e) => setPath(e.currentTarget.value)} spellcheck={false} />
          <button class="btn-secondary" onClick={add} disabled={!host().trim() || !path().trim()}>
            <Plus size={13} /> Add
          </button>
        </div>
      </div>
    </section>
  );
}

registerSettingsPage({
  id: "build.docker",
  title: t("settings.page.build.docker"),
  group: "Build, Execution, Deployment",
  order: 10,
  keywords: ["docker", "socket", "DOCKER_HOST", "tcp", "tls", "compose", "named pipe", "podman", "ssh", "remote", "context"],
  component: DockerPage,
});

registerSettingsPage({
  id: "build.docker.registries",
  title: t("settings.page.build.docker.registries"),
  group: "Build, Execution, Deployment",
  parent: "build.docker",
  order: 0,
  keywords: ["registry", "docker hub", "ghcr", "login", "credentials"],
  component: RegistryPage,
});

registerSettingsPage({
  id: "build.kubernetes",
  title: t("settings.page.build.kubernetes"),
  group: "Build, Execution, Deployment",
  order: 20,
  keywords: ["kubeconfig", "kubectl", "context", "namespace", "k8s", "remote", "host", "ssh"],
  component: () => (
    <>
      <SchemaPage pageId="build.kubernetes" />
      <HostKubectl />
    </>
  ),
});
