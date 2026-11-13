/**
 * Fake Docker daemon and Kubernetes cluster for the browser-mode Services tool window. The
 * data is kept in the shape of the real APIs (Docker Engine API JSON, Kubernetes lists,
 * kubeconfig YAML) so the Rust models parse it exactly like real responses.
 */
import { demoProjectName } from "./demoProject";

const P = demoProjectName;
const WORKDIR = `/home/dev/Projects/${P}`;
const hour = 3600;

/** Unix seconds `ago` seconds before now. */
export const ago = (seconds: number) => Math.floor(Date.now() / 1000) - seconds;
const iso = (secondsAgo: number) => new Date((ago(secondsAgo)) * 1000).toISOString().replace(/\.\d+Z$/, "Z");

export interface RawContainer {
  Id: string;
  Names: string[];
  Image: string;
  ImageID: string;
  Command: string;
  Created: number;
  Ports: { IP?: string; PrivatePort: number; PublicPort?: number; Type: string }[];
  Labels: Record<string, string>;
  State: string;
  Status: string;
  NetworkSettings: { Networks: Record<string, { IPAddress: string }> };
  Mounts: { Type: string; Name?: string; Source: string; Destination: string; RW: boolean }[];
  /** Mock only: environment for inspect. */
  Env?: string[];
}

const composeLabels = (service: string, image: string) => ({
  "com.docker.compose.project": P,
  "com.docker.compose.service": service,
  "com.docker.compose.container-number": "1",
  "com.docker.compose.oneoff": "False",
  "com.docker.compose.project.working_dir": WORKDIR,
  "com.docker.compose.project.config_files": `${WORKDIR}/docker-compose.yml`,
  "com.docker.compose.image": image,
  "com.docker.compose.version": "2.29.7",
});

export function composeContainer(service: string, image: string, opts: Partial<RawContainer> & { ip: string }): RawContainer {
  const id = opts.Id ?? Array.from({ length: 64 }, (_, i) => "0123456789abcdef"[(service.charCodeAt(i % service.length) * (i + 7)) % 16]).join("");
  return {
    Id: id,
    Names: [`/${P}-${service}-1`],
    Image: image,
    ImageID: `sha256:${id.split("").reverse().join("")}`,
    Command: opts.Command ?? "",
    Created: opts.Created ?? ago(2 * hour + 300),
    Ports: opts.Ports ?? [],
    Labels: composeLabels(service, image),
    State: opts.State ?? "running",
    Status: opts.Status ?? "Up 2 hours",
    NetworkSettings: { Networks: { [`${P}_default`]: { IPAddress: opts.ip } } },
    Mounts: opts.Mounts ?? [],
    Env: opts.Env,
  };
}

export function demoContainers(): RawContainer[] {
  return [
    composeContainer("app", `${P}-app`, {
      Id: "4f2a9c1be8d3a7c55e0b9d1f6a2c4e8b7d3f1a9c0e5b2d4f6a8c1e3b5d7f9a2c",
      Command: "docker-php-entrypoint php artisan serve --host=0.0.0.0",
      ip: "172.18.0.4",
      Ports: [
        { IP: "0.0.0.0", PrivatePort: 8000, PublicPort: 8000, Type: "tcp" },
        { IP: "::", PrivatePort: 8000, PublicPort: 8000, Type: "tcp" },
        { PrivatePort: 9000, Type: "tcp" },
      ],
      Mounts: [{ Type: "bind", Source: WORKDIR, Destination: "/var/www/html", RW: true }],
      Env: ["DB_HOST=mysql", "XDEBUG_MODE=debug", "PHP_VERSION=8.3.12", "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"],
    }),
    composeContainer("mysql", "mysql:8.4", {
      Id: "9b7e2d41c0a8f3e6b5d4c2a1908f7e6d5c4b3a2918f7e6d5c4b3a2910f8e7d6c",
      Command: "docker-entrypoint.sh mysqld",
      ip: "172.18.0.2",
      Ports: [{ PrivatePort: 3306, Type: "tcp" }, { PrivatePort: 33060, Type: "tcp" }],
      Mounts: [{ Type: "volume", Name: `${P}_db-data`, Source: `/var/lib/docker/volumes/${P}_db-data/_data`, Destination: "/var/lib/mysql", RW: true }],
      Env: ["MYSQL_DATABASE=shop", "MYSQL_ROOT_PASSWORD=secret", "MYSQL_MAJOR=8.4", "GOSU_VERSION=1.17"],
    }),
    composeContainer("redis", "redis:7-alpine", {
      Id: "c31d5e8a0b7f4c2d9e6a1b3c5d7e9f0a2b4c6d8e0f1a3b5c7d9e1f3a5b7c9d0e",
      Command: "docker-entrypoint.sh redis-server",
      ip: "172.18.0.3",
      Ports: [
        { IP: "0.0.0.0", PrivatePort: 6379, PublicPort: 6379, Type: "tcp" },
        { IP: "::", PrivatePort: 6379, PublicPort: 6379, Type: "tcp" },
      ],
      Mounts: [{ Type: "volume", Name: "8d1f0c6e2b9a4c7d5e3f1a0b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d", Source: "/var/lib/docker/volumes/8d1f0c6e2b9a/_data", Destination: "/data", RW: true }],
      Env: ["REDIS_VERSION=7.4.1"],
    }),
    {
      Id: "e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d9e8f7",
      Names: ["/mailpit"],
      Image: "axllent/mailpit:latest",
      ImageID: "sha256:7a6b5c4d3e2f",
      Command: "/mailpit",
      Created: ago(3 * 24 * hour),
      Ports: [],
      Labels: { "org.opencontainers.image.title": "Mailpit" },
      State: "exited",
      Status: "Exited (0) 3 days ago",
      NetworkSettings: { Networks: { bridge: { IPAddress: "" } } },
      Mounts: [],
    },
  ];
}

export function demoImages() {
  const img = (id: string, tags: string[], size: number, createdAgo: number, containers = 0) => ({
    Id: `sha256:${id}`,
    RepoTags: tags,
    RepoDigests: tags[0] === "<none>:<none>" || tags[0].startsWith(P) ? [] : [`${tags[0].split(":")[0]}@sha256:${id.split("").reverse().join("")}`],
    Created: ago(createdAgo),
    Size: size,
    Containers: containers,
  });
  return [
    img("c2a7e4f1b9d8c3a6e5f4d3c2b1a0918f7e6d5c4b3a29180f7e6d5c4b3a291807", [`${P}-app:latest`], 187_400_000, 2 * hour + 600, 1),
    img("0a9b8c7d6e5f4a3b2c1d0e9f8a7b6c5d4e3f2a1b0c9d8e7f6a5b4c3d2e1f0a9b", ["mysql:8.4"], 603_500_000, 21 * 24 * hour, 1),
    img("1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c", ["redis:7-alpine"], 41_200_000, 35 * 24 * hour, 1),
    img("7a6b5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d9e8f7a6b", ["axllent/mailpit:latest"], 31_800_000, 60 * 24 * hour, 1),
    img("3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d", ["php:8.3-fpm-alpine"], 84_700_000, 28 * 24 * hour),
    img("5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f", ["composer:2"], 199_300_000, 40 * 24 * hour),
    img("9f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4a39281706f5e4d3c2b1a0", ["<none>:<none>"], 187_100_000, 3 * 24 * hour),
  ];
}

export function demoVolumes() {
  return {
    Volumes: [
      { Name: `${P}_db-data`, Driver: "local", Mountpoint: `/var/lib/docker/volumes/${P}_db-data/_data`, CreatedAt: iso(9 * 24 * hour), Labels: { "com.docker.compose.project": P, "com.docker.compose.volume": "db-data" }, Scope: "local" },
      { Name: "8d1f0c6e2b9a4c7d5e3f1a0b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d", Driver: "local", Mountpoint: "/var/lib/docker/volumes/8d1f0c6e2b9a/_data", CreatedAt: iso(2 * hour), Labels: {}, Scope: "local" },
    ],
  };
}

export function demoNetworks() {
  const net = (id: string, name: string, driver: string, subnet?: string, gateway?: string, labels: Record<string, string> = {}) => ({
    Id: id,
    Name: name,
    Driver: driver,
    Scope: "local",
    Internal: false,
    IPAM: { Config: subnet ? [{ Subnet: subnet, Gateway: gateway }] : [] },
    Labels: labels,
  });
  return [
    net("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2", `${P}_default`, "bridge", "172.18.0.0/16", "172.18.0.1", { "com.docker.compose.project": P, "com.docker.compose.network": "default" }),
    net("b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3", "bridge", "bridge", "172.17.0.0/16", "172.17.0.1"),
    net("c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4", "host", "host"),
    net("d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5", "none", "null"),
  ];
}

export const demoVersion = {
  Version: "27.3.1",
  ApiVersion: "1.47",
  Os: "linux",
  Arch: "amd64",
  Platform: { Name: "Docker Engine - Community" },
};

// ---- logs ------------------------------------------------------------------------------

/** Times continue the history below (last line at 09:12:44). */
const ts = (n: number) => {
  const s = 12 * 60 + 44 + 5 + n * 6;
  return `2026-10-20 09:${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
};

/** Log history of a container (by compose service or name) with ANSI colors. */
export function initialLogs(service: string): string[] {
  switch (service) {
    case "app":
      return [
        "\x1b[32m\x1b[1m   INFO \x1b[0m Server running on [http://0.0.0.0:8000].\n",
        "\n",
        "\x1b[33m  Press Ctrl+C to stop the server\x1b[0m\n",
        "\n",
        "  2026-10-20 09:12:04 / ......................................................... ~ 0.31ms\n",
        "  2026-10-20 09:12:05 /build/assets/app-4ed993c7.css ............................ ~ 0.08ms\n",
        "  2026-10-20 09:12:05 /build/assets/app-a1b2c3d4.js ............................. ~ 0.09ms\n",
        "  2026-10-20 09:12:11 /orders ................................................... ~ 12.4ms\n",
        "  2026-10-20 09:12:16 /orders/1042 .............................................. ~ 8.71ms\n",
        "\x1b[33m[2026-10-20 09:12:16] local.WARNING: Order 1042 has no shipping address {\"user\":7}\x1b[0m\n",
        "  2026-10-20 09:12:20 /api/cart ................................................. ~ 4.02ms\n",
        "  2026-10-20 09:12:31 /checkout ................................................. ~ 21.9ms\n",
        "\x1b[31m[2026-10-20 09:12:31] local.ERROR: SQLSTATE[23000]: Integrity constraint violation: 1062 Duplicate entry 'ACME-1042' for key 'orders.orders_number_unique' (Connection: mysql)\x1b[0m\n",
        "\x1b[31m#0 /var/www/html/vendor/laravel/framework/src/Illuminate/Database/Connection.php(779): Illuminate\\Database\\Connection->runQueryCallback()\x1b[0m\n",
        "\x1b[31m#1 /var/www/html/app/Http/Controllers/OrderController.php(48): Illuminate\\Database\\Eloquent\\Builder->create()\x1b[0m\n",
        "  2026-10-20 09:12:44 /orders ................................................... ~ 9.86ms\n",
      ];
    case "mysql":
      return [
        "2026-10-20 07:10:12+00:00 [Note] [Entrypoint]: Entrypoint script for MySQL Server 8.4.3-1.el9 started.\n",
        "2026-10-20 07:10:12+00:00 [Note] [Entrypoint]: Switching to dedicated user 'mysql'\n",
        "2026-10-20T07:10:13.004521Z 0 [System] [MY-015015] [Server] MySQL Server - start.\n",
        "2026-10-20T07:10:13.211304Z 0 [System] [MY-010116] [Server] /usr/sbin/mysqld (mysqld 8.4.3) starting as process 1\n",
        "2026-10-20T07:10:13.230981Z 1 [System] [MY-013576] [InnoDB] InnoDB initialization has started.\n",
        "2026-10-20T07:10:13.902147Z 1 [System] [MY-013577] [InnoDB] InnoDB initialization has ended.\n",
        "2026-10-20T07:10:14.188472Z 0 [Warning] [MY-010068] [Server] CA certificate ca.pem is self signed.\n",
        "2026-10-20T07:10:14.188911Z 0 [System] [MY-013602] [Server] Channel mysql_main configured to support TLS.\n",
        "2026-10-20T07:10:14.214602Z 0 [System] [MY-011323] [Server] X Plugin ready for connections. Bind-address: '::' port: 33060, socket: /var/run/mysqld/mysqlx.sock\n",
        "2026-10-20T07:10:14.214764Z 0 [System] [MY-010931] [Server] /usr/sbin/mysqld: ready for connections. Version: '8.4.3'  socket: '/var/run/mysqld/mysqld.sock'  port: 3306  MySQL Community Server - GPL.\n",
      ];
    case "redis":
      return [
        "1:C 20 Oct 2026 07:10:12.511 * oO0OoO0OoO0Oo Redis is starting oO0OoO0OoO0Oo\n",
        "1:C 20 Oct 2026 07:10:12.511 * Redis version=7.4.1, bits=64, commit=00000000, modified=0, pid=1, just started\n",
        "1:C 20 Oct 2026 07:10:12.511 # Warning: no config file specified, using the default config.\n",
        "1:M 20 Oct 2026 07:10:12.512 * monotonic clock: POSIX clock_gettime\n",
        "1:M 20 Oct 2026 07:10:12.512 * Running mode=standalone, port=6379.\n",
        "1:M 20 Oct 2026 07:10:12.513 * Server initialized\n",
        "1:M 20 Oct 2026 07:10:12.513 * Ready to accept connections tcp\n",
        "1:M 20 Oct 2026 08:10:13.022 * 1 changes in 3600 seconds. Saving...\n",
        "1:M 20 Oct 2026 08:10:13.023 * Background saving started by pid 21\n",
        "21:C 20 Oct 2026 08:10:13.031 * DB saved on disk\n",
        "1:M 20 Oct 2026 08:10:13.124 * Background saving terminated with success\n",
      ];
    case "mailpit":
      return [
        'time="2026/10/17 10:02:11" level=info msg="[smtpd] starting on [::]:1025 (no encryption)"\n',
        'time="2026/10/17 10:02:11" level=info msg="[http] starting on [::]:8025"\n',
        'time="2026/10/17 18:40:03" level=info msg="[http] shutting down"\n',
      ];
    default:
      return ["Container started\n"];
  }
}

const PATHS = ["/", "/orders", "/orders/1043", "/api/cart", "/products?page=2", "/account", "/api/orders/1043/status"];

/** A new log line every few seconds while a Log tab follows a container. */
export function nextLogLine(service: string, n: number): string | null {
  if (service === "app") {
    const path = PATHS[n % PATHS.length];
    const ms = (3 + ((n * 7.3) % 19)).toFixed(2);
    return `  ${ts(n)} ${path} ${".".repeat(Math.max(3, 64 - path.length))} ~ ${ms}ms\n`;
  }
  if (service === "redis" && n % 4 === 3) return `1:M 20 Oct 2026 09:${String(13 + n).padStart(2, "0")}:13.022 * DB saved on disk\n`;
  return null;
}

// ---- Kubernetes ------------------------------------------------------------------------

export const demoKubeconfig = `apiVersion: v1
kind: Config
current-context: kind-acme
clusters:
- name: kind-acme
  cluster:
    server: https://127.0.0.1:43215
    certificate-authority-data: LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCg==
- name: prod-eu
  cluster:
    server: https://k8s.acme.example:6443
contexts:
- name: kind-acme
  context:
    cluster: kind-acme
    user: kind-acme
    namespace: shop
- name: prod-eu
  context:
    cluster: prod-eu
    user: deploy
    namespace: shop
users:
- name: kind-acme
  user:
    client-certificate-data: Y2VydA==
    client-key-data: a2V5
- name: deploy
  user:
    exec:
      apiVersion: client.authentication.k8s.io/v1
      command: kubelogin
      args: [get-token, --environment, AzurePublicCloud]
`;

/** `kubectl config view -o json` on the remote demo host: a k3s next to the shop and the
 * production cluster (secrets redacted by kubectl). */
export const hostKubeconfigView = JSON.stringify({
  kind: "Config",
  apiVersion: "v1",
  preferences: {},
  "current-context": "k3s-shop",
  clusters: [
    { name: "k3s", cluster: { server: "https://127.0.0.1:6443", "certificate-authority-data": "DATA+OMITTED" } },
    { name: "prod-eu", cluster: { server: "https://k8s.acme.example:6443" } },
  ],
  contexts: [
    { name: "k3s-shop", context: { cluster: "k3s", user: "k3s", namespace: "shop" } },
    { name: "prod-eu", context: { cluster: "prod-eu", user: "deploy", namespace: "shop" } },
  ],
  users: [
    { name: "k3s", user: { "client-certificate-data": "DATA+OMITTED", "client-key-data": "DATA+OMITTED" } },
    { name: "deploy", user: { token: "REDACTED" } },
  ],
});

export const demoNamespaces = { items: ["default", "kube-node-lease", "kube-public", "kube-system", "shop"].map((name) => ({ metadata: { name } })) };

/** Ports the demo containers declare (Forward Ports suggestions). */
const CONTAINER_PORTS: Record<string, { containerPort: number; name: string }[]> = {
  php: [{ containerPort: 9000, name: "php-fpm" }],
  nginx: [{ containerPort: 80, name: "http" }],
  mysql: [{ containerPort: 3306, name: "mysql" }],
};

function pod(
  name: string,
  opts: {
    app: string;
    containers: string[];
    initContainers?: string[];
    defaultContainer?: string;
    phase: string;
    ready: boolean[];
    restarts?: number[];
    waiting?: string;
    terminated?: string;
    createdAgo: number;
    ip: string;
    owner?: string;
  },
) {
  return {
    metadata: {
      name,
      namespace: "shop",
      creationTimestamp: iso(opts.createdAgo),
      labels: { app: opts.app, "pod-template-hash": name.split("-").slice(-2, -1)[0] ?? "" },
      annotations: opts.defaultContainer ? { "kubectl.kubernetes.io/default-container": opts.defaultContainer } : {},
      ownerReferences: opts.owner ? [{ kind: opts.owner.split("/")[0], name: opts.owner.split("/")[1] }] : [],
    },
    spec: {
      nodeName: "kind-control-plane",
      initContainers: (opts.initContainers ?? []).map((c) => ({ name: c, image: "acme/shop:1.4.2" })),
      containers: opts.containers.map((c) => ({
        name: c,
        image: c === "php" ? "acme/shop:1.4.2" : `${c}:latest`,
        ports: (CONTAINER_PORTS[c] ?? []).map((p) => ({ ...p, protocol: "TCP" })),
      })),
    },
    status: {
      phase: opts.phase,
      podIP: opts.ip,
      containerStatuses: opts.containers.map((c, i) => ({
        name: c,
        ready: opts.ready[i],
        restartCount: opts.restarts?.[i] ?? 0,
        state: opts.waiting && !opts.ready[i] ? { waiting: { reason: opts.waiting } } : opts.terminated ? { terminated: { reason: opts.terminated } } : { running: { startedAt: iso(opts.createdAgo - 5) } },
      })),
    },
  };
}

export function demoWorkloads() {
  const pods = {
    items: [
      pod("web-7d4b9c6f8-x2kqf", { app: "web", containers: ["nginx", "php"], initContainers: ["cache-warmup"], defaultContainer: "php", phase: "Running", ready: [true, true], createdAgo: 26 * hour, ip: "10.244.0.12", owner: "ReplicaSet/web-7d4b9c6f8" }),
      pod("web-7d4b9c6f8-p8m4n", { app: "web", containers: ["nginx", "php"], initContainers: ["cache-warmup"], defaultContainer: "php", phase: "Running", ready: [true, true], createdAgo: 26 * hour, ip: "10.244.0.13", owner: "ReplicaSet/web-7d4b9c6f8" }),
      pod("worker-5c9d7b8f4-qz2lw", { app: "worker", containers: ["php"], phase: "Running", ready: [true], restarts: [3], createdAgo: 3 * 24 * hour, ip: "10.244.0.9", owner: "ReplicaSet/worker-5c9d7b8f4" }),
      pod("scheduler-6f8d9c7b5-tq9vd", { app: "scheduler", containers: ["php"], phase: "Running", ready: [false], restarts: [7], waiting: "CrashLoopBackOff", createdAgo: 40 * 60, ip: "10.244.0.21", owner: "ReplicaSet/scheduler-6f8d9c7b5" }),
      pod("mysql-0", { app: "mysql", containers: ["mysql"], phase: "Running", ready: [true], createdAgo: 9 * 24 * hour, ip: "10.244.0.5", owner: "StatefulSet/mysql" }),
      pod("migrate-29384117-8xk2d", { app: "migrate", containers: ["php"], phase: "Succeeded", ready: [false], terminated: "Completed", createdAgo: 26 * hour, ip: "10.244.0.11", owner: "Job/migrate-29384117" }),
    ],
  };
  const deployment = (name: string, replicas: number, ready: number, image: string, createdAgo: number) => ({
    metadata: { name, namespace: "shop", creationTimestamp: iso(createdAgo) },
    spec: { replicas, selector: { matchLabels: { app: name } }, template: { spec: { containers: [{ name: "php", image }] } } },
    status: { readyReplicas: ready, updatedReplicas: replicas, availableReplicas: ready },
  });
  const deployments = {
    items: [
      deployment("web", 2, 2, "acme/shop:1.4.2", 30 * 24 * hour),
      deployment("worker", 1, 1, "acme/shop:1.4.2", 30 * 24 * hour),
      deployment("scheduler", 1, 0, "acme/shop:1.4.2", 30 * 24 * hour),
    ],
  };
  const service = (name: string, type: string, clusterIP: string, ports: { port: number; nodePort?: number }[]) => ({
    metadata: { name, namespace: "shop", creationTimestamp: iso(30 * 24 * hour) },
    spec: { type, clusterIP, ports: ports.map((p) => ({ ...p, protocol: "TCP" })) },
    status: {},
  });
  const services = {
    items: [
      service("web", "NodePort", "10.96.41.7", [{ port: 80, nodePort: 30080 }]),
      service("mysql", "ClusterIP", "10.96.12.4", [{ port: 3306 }]),
      service("redis", "ClusterIP", "10.96.88.15", [{ port: 6379 }]),
    ],
  };
  return { pods, deployments, services };
}

/** Log lines of a pod's container (the default container when none is given). */
export function podLogs(name: string, container?: string): string[] {
  if (container === "nginx")
    return [
      '10.244.0.1 - - [20/Oct/2026:09:12:04 +0000] "GET /healthz HTTP/1.1" 200 2 "-" "kube-probe/1.31"\n',
      '172.18.0.1 - - [20/Oct/2026:09:12:16 +0000] "GET /orders/1042 HTTP/1.1" 200 18342 "-" "Mozilla/5.0"\n',
      '172.18.0.1 - - [20/Oct/2026:09:12:17 +0000] "GET /build/assets/app-4f9c2b.css HTTP/1.1" 200 48211 "-" "Mozilla/5.0"\n',
    ];
  if (container === "cache-warmup")
    return ["\x1b[32m   INFO \x1b[0m Caching configuration.\n", "\x1b[32m   INFO \x1b[0m Caching routes.\n", "\x1b[32m   INFO \x1b[0m Caching views.\n"];
  if (name.startsWith("web"))
    return [
      "[20-Oct-2026 09:00:01] NOTICE: fpm is running, pid 1\n",
      "[20-Oct-2026 09:00:01] NOTICE: ready to handle connections\n",
      '127.0.0.1 -  20/Oct/2026:09:12:16 +0000 "GET /index.php" 200\n',
    ];
  if (name.startsWith("scheduler"))
    return [
      "\x1b[32m   INFO \x1b[0m Running scheduled tasks.\n",
      "\x1b[31m   ERROR \x1b[0m Connection refused [tcp://redis:6379].\n",
      "\n",
      "In PhpRedisConnector.php line 159:\n",
      "  Connection refused\n",
    ];
  if (name.startsWith("worker"))
    return [
      "\x1b[32m   INFO \x1b[0m Processing jobs from the [default] queue.\n",
      "  2026-10-20 09:10:01 App\\Jobs\\SendOrderConfirmation ........... RUNNING\n",
      "  2026-10-20 09:10:01 App\\Jobs\\SendOrderConfirmation ........... 41.20ms DONE\n",
      "  2026-10-20 09:11:17 App\\Jobs\\SyncInventory ................... RUNNING\n",
      "  2026-10-20 09:11:18 App\\Jobs\\SyncInventory ................... 812.4ms DONE\n",
    ];
  return [
    '10.244.0.1 - - [20/Oct/2026:09:12:04 +0000] "GET / HTTP/1.1" 200 5123 "-" "kube-probe/1.31"\n',
    '10.244.0.1 - - [20/Oct/2026:09:12:14 +0000] "GET /healthz HTTP/1.1" 200 2 "-" "kube-probe/1.31"\n',
    '172.18.0.1 - - [20/Oct/2026:09:12:16 +0000] "GET /orders/1042 HTTP/1.1" 200 18342 "-" "Mozilla/5.0"\n',
  ];
}
