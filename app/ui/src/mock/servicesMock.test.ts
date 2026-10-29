import { beforeAll, describe, expect, it } from "vitest";
import { listen } from "@/ipc";
import { servicesApi, type ServiceNode, type ServicesEvent } from "@/ipc/services";
import { forgeWasm } from "@/wasm";
import "./settingsMock";
import "./workspaceMock";
import "./servicesMock";

const find = (nodes: ServiceNode[], id: string): ServiceNode | undefined => {
  for (const n of nodes) {
    if (n.id === id) return n;
    const c = n.children && find(n.children, id);
    if (c) return c;
  }
  return undefined;
};

describe("services mock (Rust tree through WebAssembly)", () => {
  beforeAll(async () => {
    await forgeWasm();
  });

  it("builds the Docker, Compose and Kubernetes tree", async () => {
    const tree = await servicesApi.tree();
    expect(tree.map((n) => n.label)).toEqual(["Docker", "Docker Compose", "Kubernetes"]);
    expect(find(tree, "docker/containers")?.detail).toBe("4");
    const project = find(tree, "compose/acme-shop")!;
    expect(project.children?.map((s) => s.label)).toEqual(["app", "mysql", "redis"]);
    expect(project.status).toBe("running");
    expect(find(tree, "kube/kind-acme")?.lazy).toBe(true);
  });

  it("loads Kubernetes workloads on expand", async () => {
    const tree = await servicesApi.expand("kube/kind-acme");
    const pod = find(tree, "kube/kind-acme/shop/pods/scheduler-6f8d9c7b5-tq9vd");
    expect(pod?.status).toBe("error");
    expect(pod?.detail).toBe("0/1 · CrashLoopBackOff");
    const failing = await servicesApi.expand("kube/prod-eu");
    expect(find(failing, "kube/prod-eu")?.status).toBe("error");
  });

  it("streams logs and runs compose actions", async () => {
    const events: ServicesEvent[] = [];
    await listen<ServicesEvent>("services://event", (e) => events.push(e));
    const tree = await servicesApi.tree();
    const app = find(tree, "docker/containers")!.children!.find((c) => c.label === "acme-shop-app-1")!;
    const details = await servicesApi.details(app.id);
    expect(details?.tabs).toEqual(["dashboard", "log", "console", "inspection"]);
    expect(details?.actions).toContain("exec");
    const key = await servicesApi.logsStart(app.id);
    const lines = await servicesApi.console(key);
    expect(lines.some((l) => l.segments.map((s) => s.text).join("").includes("Server running on"))).toBe(true);
    await servicesApi.logsStop(app.id);

    await servicesApi.action("compose/acme-shop/redis", "down");
    expect(find(await servicesApi.tree(), "compose/acme-shop/redis")?.status).toBe("stopped");
    await servicesApi.action("compose/acme-shop/redis", "up");
    expect(find(await servicesApi.tree(), "compose/acme-shop/redis")?.status).toBe("running");
    const out = await servicesApi.console("run:compose/acme-shop");
    expect(out.map((l) => l.segments.map((s) => s.text).join("")).join("\n")).toContain("Container acme-shop-redis-1  Started");
    expect(events.some((e) => e.type === "tree")).toBe(true);

    const exec = await servicesApi.terminal(app.id, false);
    expect(exec.args.slice(0, 2)).toEqual(["exec", "-it"]);
    expect(JSON.parse(await servicesApi.inspect(app.id)).Name).toBe("/acme-shop-app-1");
  });

  it("picks pod containers, aggregates deployment logs and forwards ports", async () => {
    await servicesApi.expand("kube/kind-acme");
    const pod = "kube/kind-acme/shop/pods/web-7d4b9c6f8-x2kqf";
    const d = (await servicesApi.details(pod))!;
    expect(d.tabs).toEqual(["dashboard", "log", "console", "inspection"]);
    expect(d.containers?.map((c) => [c.name, !!c.default, !!c.init])).toEqual([
      ["nginx", false, false],
      ["php", true, false],
      ["cache-warmup", false, true],
    ]);
    expect(d.ports?.map((p) => p.port)).toEqual([80, 9000]);
    const text = async (key: string) => (await servicesApi.console(key)).map((l) => l.segments.map((s) => s.text).join("")).join("\n");
    expect(await text(await servicesApi.logsStart(pod))).toContain("fpm is running");
    expect(await text(await servicesApi.logsStart(pod, "nginx"))).toContain("GET /healthz");
    const attach = await servicesApi.terminal(pod, true, "nginx");
    expect(attach.args).toContain("attach");
    expect(attach.args.slice(-2)).toEqual(["-c", "nginx"]);

    const logs = await text(await servicesApi.logsStart("kube/kind-acme/shop/deployments/web"));
    expect(logs).toContain("[pod/web-7d4b9c6f8-x2kqf/nginx] 10.244.0.1");
    expect(logs).toContain("[pod/web-7d4b9c6f8-p8m4n/php] [20-Oct-2026 09:00:01] NOTICE: fpm is running");

    const [f] = await servicesApi.portForwardStart(pod, "8080:80");
    expect([f.localPort, f.remotePort]).toEqual([8080, 80]);
    expect((await servicesApi.details(pod))?.portForwards?.map((x) => x.id)).toEqual([f.id]);
    expect(find(await servicesApi.tree(), pod)?.detail).toBe("2/2 · Running · ⇄ 8080→80");
    await expect(servicesApi.portForwardStart(pod, "8080:80")).rejects.toThrow("already in use");
    await expect(servicesApi.portForwardStart(pod, "x")).rejects.toThrow("Invalid");
    await servicesApi.portForwardStop(f.id);
    expect(await servicesApi.portForwards()).toEqual([]);
  });

  it("lists Docker contexts", async () => {
    const contexts = await servicesApi.dockerContexts();
    expect(contexts.map((c) => c.name)).toEqual(["buildbox", "staging"]);
    const docker = await servicesApi.details("docker");
    expect(docker?.sections.find((s) => s.title === "Docker Contexts")?.table?.rows[0][1]).toBe("ssh://ci@buildbox.acme.dev");
  });

  it("answers gutter markers for compose files and Dockerfiles", async () => {
    const marks = await servicesApi.gutter("docker-compose.yml", "services:\n  app:\n    image: x\n");
    expect(marks.map((m) => [m.line, m.kind])).toEqual([
      [0, "composeAll"],
      [1, "composeService"],
    ]);
    expect((await servicesApi.gutter("Dockerfile", "FROM php\n"))[0].kind).toBe("dockerfile");
  });

  it("selects Kubernetes contexts and applies manifests", async () => {
    const sel = await servicesApi.kubeSelection();
    expect(sel?.contexts).toEqual(["kind-acme", "prod-eu"]);
    expect(sel?.source).toBe("~/.kube/config");
    expect(sel?.host).toBeUndefined();
    const picked = await servicesApi.kubeSelect("kind-acme", "default");
    expect([picked?.context, picked?.namespace]).toEqual(["kind-acme", "default"]);
    const marks = await servicesApi.gutter("k8s/deployment.yaml", "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\n");
    expect(marks[0].kind).toBe("kubeManifest");
    await servicesApi.kubeApply("k8s/deployment.yaml", "apply");
    const lines = (await servicesApi.console("run:kube")).map((l) => l.segments.map((s) => s.text).join(""));
    expect(lines.some((l) => l.includes("deployment.apps/web configured"))).toBe(true);
  });
});
