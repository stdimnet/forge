/**
 * Docker Compose files: YAML plus completion and hover documentation for the Compose
 * Specification (top-level elements, service attributes, networks, volumes, configs,
 * secrets), driven by the indentation context so it works while the file is incomplete.
 * The merge tags `!reset` and `!override` are completed and documented as well.
 */
import { yaml } from "@codemirror/lang-yaml";
import type { Completion } from "@codemirror/autocomplete";
import { registerLanguage } from "@/editor/languages";
import { flag, key, map, schemaCompletion, schemaHover, seq, type SchemaNode, type SchemaTags } from "./yamlSchema";

const duration = "A duration such as `30s`, `1m30s` or `2h`.";

const healthcheck = map("Checks that the service's containers are healthy.", {
  test: key("The command that checks health: a string, or a list starting with `CMD`, `CMD-SHELL` or `NONE`.", "string | list"),
  interval: key(`Time between checks (default \`30s\`). ${duration}`),
  timeout: key(`Time before a check counts as failed (default \`30s\`). ${duration}`),
  retries: key("Consecutive failures needed to report the container unhealthy (default `3`).", "int"),
  start_period: key(`Grace period during which failures do not count (default \`0s\`). ${duration}`),
  start_interval: key(`Time between checks during the start period. ${duration}`),
  disable: flag("Disable the healthcheck inherited from the image."),
});

const buildKeys: Record<string, SchemaNode> = {
  context: key("Path to the build context: a directory with a Dockerfile, or a git URL."),
  dockerfile: key("Alternative Dockerfile, relative to the context."),
  dockerfile_inline: key("The Dockerfile content as an inline string."),
  args: key("Build arguments (`ARG`), as a map or a list of `KEY=value`.", "map | list"),
  target: key("The stage of a multi-stage Dockerfile to build."),
  cache_from: seq("Images the engine uses as cache sources."),
  cache_to: seq("Locations to export the build cache to."),
  labels: key("Labels added to the built image.", "map | list"),
  network: key("The network used for `RUN` instructions during the build."),
  platforms: seq("Platforms to build for, e.g. `linux/amd64`."),
  pull: flag("Always attempt to pull newer versions of the base images."),
  secrets: seq("Secrets the build can access."),
  ssh: seq("SSH agent sockets or keys the build can use."),
  shm_size: key("Size of `/dev/shm` for the build container."),
  tags: seq("Additional tags for the built image."),
  no_cache: flag("Disable the build cache."),
  additional_contexts: key("Named extra build contexts.", "map | list"),
  extra_hosts: key("Extra hosts added to the build container.", "map | list"),
  isolation: key("Container isolation technology."),
  privileged: flag("Give extended privileges to the build."),
  ulimits: key("Ulimits for the build container.", "map"),
};

const deploy = map("Deployment and lifecycle settings, used by Swarm and by `docker compose` where supported.", {
  mode: key("`global` or `replicated` (default).", "string", { values: ["replicated", "global", "replicated-job", "global-job"] }),
  replicas: key("Number of containers that should run (replicated mode).", "int"),
  labels: key("Labels for the service, not its containers.", "map | list"),
  endpoint_mode: key("Service discovery method for external clients.", "string", { values: ["vip", "dnsrr"] }),
  placement: map("Constraints and preferences for where tasks run.", {
    constraints: seq("Placement constraints such as `node.role == manager`."),
    preferences: seq("Placement preferences, e.g. spreading over a label."),
    max_replicas_per_node: key("Maximum replicas on one node.", "int"),
  }),
  resources: map("Physical resource limits and reservations.", {
    limits: map("Hard limits.", { cpus: key("CPU limit, e.g. `0.5`."), memory: key("Memory limit, e.g. `512M`."), pids: key("Process limit.", "int") }),
    reservations: map("Guaranteed minimum.", { cpus: key("Reserved CPUs."), memory: key("Reserved memory."), devices: seq("Reserved devices such as GPUs.") }),
  }),
  restart_policy: map("How containers are restarted when they exit.", {
    condition: key("When to restart.", "string", { values: ["none", "on-failure", "any"] }),
    delay: key(`Wait between restart attempts. ${duration}`),
    max_attempts: key("Attempts before giving up.", "int"),
    window: key(`Time to wait before deciding a restart worked. ${duration}`),
  }),
  update_config: map("How the service is updated.", {
    parallelism: key("Containers updated at a time.", "int"),
    delay: key("Wait between updates."),
    failure_action: key("What to do on failure.", "string", { values: ["continue", "rollback", "pause"] }),
    monitor: key("Time to monitor each update for failure."),
    max_failure_ratio: key("Tolerated failure rate during an update.", "number"),
    order: key("Order of operations.", "string", { values: ["stop-first", "start-first"] }),
  }),
  rollback_config: map("How the service is rolled back after a failed update.", {
    parallelism: key("Containers rolled back at a time.", "int"),
    delay: key("Wait between rollbacks."),
    failure_action: key("What to do on failure.", "string", { values: ["continue", "pause"] }),
    monitor: key("Time to monitor each rollback."),
    max_failure_ratio: key("Tolerated failure rate.", "number"),
    order: key("Order of operations.", "string", { values: ["stop-first", "start-first"] }),
  }),
});

const service: SchemaNode = {
  type: "map",
  extensionPrefix: "x-",
  keys: {
    image: key("The image to start the container from, e.g. `postgres:16`. Optional when `build` is set."),
    build: key("Build configuration: a path to the context, or a map with `context`, `dockerfile`, `args`, …", "string | map", { keys: buildKeys }),
    container_name: key("A custom container name instead of the generated one. Prevents scaling the service."),
    command: key("Overrides the default `CMD` of the image: a string or a list.", "string | list"),
    entrypoint: key("Overrides the default `ENTRYPOINT` of the image.", "string | list"),
    environment: key("Environment variables, as a map or a list of `KEY=value`.", "map | list"),
    env_file: key("Files with environment variables: a path or a list of paths (or maps with `path`, `required`).", "string | list"),
    ports: seq("Published ports as `HOST:CONTAINER`, e.g. `\"8080:80\"`. Quote them so YAML does not read `xx:yy` as a number."),
    expose: seq("Ports exposed to linked services, not published to the host."),
    volumes: seq("Mounts: `HOST:CONTAINER[:ro]`, a named volume, or a long-form map with `type`, `source`, `target`."),
    networks: key("Networks the service connects to: a list, or a map with `aliases`, `ipv4_address`, …", "list | map"),
    depends_on: key("Services started first: a list of names, or a map with `condition`.", "list | map", {
      any: map("A dependency.", {
        condition: key("What to wait for.", "string", { values: ["service_started", "service_healthy", "service_completed_successfully"] }),
        restart: flag("Restart this service when the dependency is updated."),
        required: flag("Whether the dependency is required (default `true`)."),
      }),
    }),
    restart: key("Restart policy: `no`, `always`, `on-failure[:max-retries]` or `unless-stopped`.", "string", {
      values: ["no", "always", "on-failure", "unless-stopped"],
    }),
    healthcheck,
    deploy,
    labels: key("Labels for the container, as a map or a list of `key=value`.", "map | list"),
    working_dir: key("Working directory inside the container."),
    user: key("User (and group) the container runs as: `uid[:gid]` or names."),
    hostname: key("A custom host name for the container."),
    domainname: key("A custom domain name for the container."),
    extra_hosts: key("Additional host-to-IP mappings for `/etc/hosts`.", "list | map"),
    dns: key("Custom DNS servers.", "string | list"),
    dns_search: key("Custom DNS search domains.", "string | list"),
    cap_add: seq("Linux capabilities to add, e.g. `SYS_ADMIN`."),
    cap_drop: seq("Linux capabilities to drop."),
    privileged: flag("Run the container with extended privileges."),
    read_only: flag("Mount the container's root file system read-only."),
    tmpfs: key("Temporary file systems to mount in memory.", "string | list"),
    stdin_open: flag("Keep stdin open (`docker run -i`)."),
    tty: flag("Allocate a pseudo-TTY (`docker run -t`)."),
    logging: map("Logging configuration.", {
      driver: key("Logging driver.", "string", { values: ["json-file", "syslog", "journald", "gelf", "fluentd", "awslogs", "splunk", "none", "local"] }),
      options: key("Driver specific options.", "map"),
    }),
    profiles: seq("Profiles the service belongs to; it only starts when one is enabled."),
    pull_policy: key("When to pull the image.", "string", { values: ["always", "never", "missing", "build", "daily", "weekly"] }),
    platform: key("The target platform, e.g. `linux/amd64`."),
    network_mode: key("Network mode, e.g. `host`, `none` or `service:<name>`."),
    pid: key("PID namespace mode, e.g. `host`."),
    ipc: key("IPC isolation mode, e.g. `shareable` or `service:<name>`."),
    init: flag("Run an init process inside the container that forwards signals and reaps processes."),
    stop_signal: key("Signal sent to stop the container (default `SIGTERM`)."),
    stop_grace_period: key(`Time to wait after the stop signal before killing the container. ${duration}`),
    sysctls: key("Kernel parameters to set in the container.", "map | list"),
    ulimits: key("Ulimit overrides.", "map"),
    security_opt: seq("Security options, e.g. `no-new-privileges:true`."),
    devices: seq("Device mappings, `HOST:CONTAINER[:permissions]`."),
    extends: map("Inherit configuration from another service, in this or another file.", {
      file: key("The Compose file to take the service from."),
      service: key("The service to extend."),
    }),
    links: seq("Link to containers of another service. Legacy; networks make it unnecessary."),
    external_links: seq("Link to containers started outside this Compose file."),
    configs: seq("Grant access to configs, by name or with `source`, `target`, `mode`."),
    secrets: seq("Grant access to secrets, by name or with `source`, `target`, `mode`."),
    runtime: key("Runtime to use, e.g. `runc` or `nvidia`."),
    shm_size: key("Size of `/dev/shm`, e.g. `256m`."),
    mem_limit: key("Memory limit, e.g. `512m`."),
    mem_reservation: key("Soft memory limit."),
    memswap_limit: key("Memory plus swap limit."),
    cpus: key("Number of CPUs the container may use.", "number"),
    cpu_shares: key("Relative CPU weight.", "int"),
    cpuset: key("CPUs the container may run on, e.g. `0-2`."),
    group_add: seq("Additional groups the container user joins."),
    userns_mode: key("User namespace mode, e.g. `host`."),
    oom_kill_disable: flag("Disable the OOM killer for the container."),
    volumes_from: seq("Mount volumes from another service or container."),
    mac_address: key("MAC address of the container."),
    annotations: key("Annotations for the container.", "map | list"),
    attach: flag("Whether `docker compose up` attaches to the service's logs (default `true`)."),
    scale: key("Number of containers to run for the service.", "int"),
    post_start: seq("Commands to run after the container starts."),
    pre_stop: seq("Commands to run before the container stops."),
    develop: map("Development settings for `docker compose watch`.", {
      watch: seq("Rules that sync, rebuild or restart on file changes (`action`, `path`, `target`)."),
    }),
    gpus: key("GPU devices to give to the container, e.g. `all`.", "string | list"),
    label_file: key("Files with labels to add.", "string | list"),
    credential_spec: key("Credential spec for managed service accounts.", "map"),
    isolation: key("Container isolation technology."),
    storage_opt: key("Storage driver options.", "map"),
    use_api_socket: flag("Give the container access to the Docker API socket."),
    models: key("AI models the service uses.", "list | map"),
    provider: map("Use a platform provider instead of a container.", { type: key("The provider binary or plugin."), options: key("Provider options.", "map") }),
  },
};

const network = map("A named network.", {
  driver: key("Network driver, e.g. `bridge` or `overlay`."),
  driver_opts: key("Driver specific options.", "map"),
  attachable: flag("Allow standalone containers to attach to an overlay network."),
  enable_ipv6: flag("Enable IPv6 on the network."),
  external: key("Use an existing network instead of creating it.", "bool | map"),
  internal: flag("Create an externally isolated network."),
  ipam: map("IP address management.", {
    driver: key("IPAM driver."),
    config: seq("Subnets, e.g. `subnet: 172.28.0.0/16`."),
    options: key("Driver specific options.", "map"),
  }),
  labels: key("Labels for the network.", "map | list"),
  name: key("A custom name for the network."),
});

const volume = map("A named volume.", {
  driver: key("Volume driver."),
  driver_opts: key("Driver specific options.", "map"),
  external: key("Use an existing volume instead of creating it.", "bool | map"),
  labels: key("Labels for the volume.", "map | list"),
  name: key("A custom name for the volume."),
});

const fileResource = (what: string) =>
  map(`A named ${what}.`, {
    file: key(`Create the ${what} from the content of a file.`),
    environment: key(`Create the ${what} from an environment variable.`),
    content: key(`Create the ${what} from inline content.`),
    external: key(`Use an existing ${what}.`, "bool | map"),
    name: key(`A custom name for the ${what}.`),
    labels: key(`Labels for the ${what}.`, "map | list"),
  });

export const composeSchema: SchemaNode = {
  type: "map",
  extensionPrefix: "x-",
  keys: {
    name: key("The project name; used as prefix for networks, volumes and containers."),
    version: key("**Obsolete.** Compose ignores it; remove it."),
    services: map("The containers of the application.", {}, { any: service }),
    networks: map("Networks the services can connect to.", {}, { any: network }),
    volumes: map("Named volumes that can be mounted by services.", {}, { any: volume }),
    configs: map("Configs that can be granted to services.", {}, { any: fileResource("config") }),
    secrets: map("Secrets that can be granted to services.", {}, { any: fileResource("secret") }),
    include: seq("Other Compose files to include: a path, or a map with `path`, `project_directory`, `env_file`."),
    models: map("AI models used by the services.", {}),
  },
};

/** YAML tags of the Compose merge rules (https://docs.docker.com/reference/compose-file/merge/). */
export const COMPOSE_TAGS: SchemaTags = {
  "!reset":
    "**Reset value.** Removes a value set by the overridden Compose file when files are merged. " +
    "A valid value for the attribute must be given, but it is ignored: the attribute is set to its " +
    "type's default value or `null`.\n\n```yaml\nservices:\n  app:\n    build: !reset null\n    ports: !reset []\n```",
  "!override":
    "**Replace value.** Fully replaces a value from the overridden Compose file instead of merging it, " +
    "e.g. to publish only the ports listed here.\n\n```yaml\nservices:\n  app:\n    ports: !override\n      - \"8443:443\"\n```",
};

/** Names declared directly under a top-level section (`services:`, `networks:`, ...). */
export function namesUnder(text: string, section: string): string[] {
  const start = text.search(new RegExp(`^${section}:\\s*(?:#.*)?$`, "m"));
  if (start < 0) return [];
  const names: string[] = [];
  let indent = -1;
  for (const line of text.slice(start).split("\n").slice(1)) {
    if (!line.trim() || /^\s*#/.test(line)) continue;
    const lead = line.length - line.trimStart().length;
    if (lead === 0) break;
    if (indent < 0) indent = lead;
    const m = lead === indent ? /^\s+([\w.-]+):/.exec(line) : null;
    if (m) names.push(m[1]);
  }
  return names;
}

const asCompletions = (names: string[], detail: string): Completion[] =>
  names.map((label) => ({ label, type: "class", detail }));

const completion = schemaCompletion({
  root: composeSchema,
  tags: COMPOSE_TAGS,
  dynamic: (at, state) => {
    const text = state.doc.toString();
    const last = at.path[at.path.length - 1];
    const inItem = last === "-";
    const owner = inItem ? at.path[at.path.length - 2] : last;
    if (at.keyPosition && at.path[0] === "services") {
      if (owner === "depends_on" || owner === "links") return asCompletions(namesUnder(text, "services"), "service");
      if (owner === "networks" && at.path.length >= 3) return asCompletions(namesUnder(text, "networks"), "network");
      if (owner === "secrets") return asCompletions(namesUnder(text, "secrets"), "secret");
      if (owner === "configs") return asCompletions(namesUnder(text, "configs"), "config");
    }
    if (!at.keyPosition && at.currentKey === "service" && owner === "extends") {
      return asCompletions(namesUnder(text, "services"), "service");
    }
    return null;
  },
});

export function composeSupport() {
  const lang = yaml();
  return [lang, lang.language.data.of({ autocomplete: completion }), schemaHover(composeSchema, COMPOSE_TAGS)];
}

registerLanguage({
  id: "docker-compose",
  load: composeSupport,
});
