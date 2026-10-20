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

  it("answers gutter markers for compose files and Dockerfiles", async () => {
    const marks = await servicesApi.gutter("docker-compose.yml", "services:\n  app:\n    image: x\n");
    expect(marks.map((m) => [m.line, m.kind])).toEqual([
      [0, "composeAll"],
      [1, "composeService"],
    ]);
    expect((await servicesApi.gutter("Dockerfile", "FROM php\n"))[0].kind).toBe("dockerfile");
  });
});
