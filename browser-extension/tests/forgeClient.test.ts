import { describe, expect, it } from "vitest";
import { candidatePorts, describeBrowser, ForgeClient, ForgeError } from "../src/lib/forgeClient";

type Call = { url: string; init: RequestInit };

/** A fake Forge on port 63344; other ports refuse connections. */
function fakeForge(calls: Call[] = []) {
  return (async (url: string, init: RequestInit) => {
    calls.push({ url, init });
    if (!url.startsWith("http://127.0.0.1:63344/")) throw new TypeError("connection refused");
    const auth = (init.headers as Record<string, string>).Authorization;
    const json = (status: number, body: unknown) => new Response(JSON.stringify(body), { status });
    if (url.endsWith("/ping")) {
      return json(200, {
        name: "Forge",
        version: "0.1.0",
        api: 1,
        paired: auth === "Bearer t0k",
        ...(auth === "Bearer t0k" ? { projects: [{ name: "shop", root: "/home/me/shop" }] } : {}),
      });
    }
    if (url.endsWith("/pair")) {
      const { code } = JSON.parse(String(init.body));
      return code === "482913" ? json(200, { clientId: "c1", token: "t0k" }) : json(403, { error: "wrong pairing code" });
    }
    if (url.endsWith("/open")) {
      return auth === "Bearer t0k" ? json(200, { status: "opened", file: JSON.parse(String(init.body)).file }) : json(401, { error: "not paired" });
    }
    return json(404, { error: "not found" });
  }) as unknown as typeof fetch;
}

describe("ForgeClient", () => {
  it("scans the port range and remembers nothing it does not need", async () => {
    const calls: Call[] = [];
    const client = new ForgeClient(fakeForge(calls));
    const found = await client.discover(undefined, "t0k");
    expect(found?.port).toBe(63344);
    expect(found?.ping.projects?.[0].root).toBe("/home/me/shop");
    expect(calls.map((c) => c.url)).toEqual([
      "http://127.0.0.1:63342/api/forge/ping",
      "http://127.0.0.1:63343/api/forge/ping",
      "http://127.0.0.1:63344/api/forge/ping",
    ]);
    expect(calls[0].init.credentials).toBe("omit");
  });

  it("tries the remembered port first", () => {
    expect(candidatePorts(63344).slice(0, 2)).toEqual([63344, 63342]);
    expect(candidatePorts()).toHaveLength(10);
  });

  it("pairs with a code (spaces allowed) and opens files with the token", async () => {
    const calls: Call[] = [];
    const client = new ForgeClient(fakeForge(calls));
    const { token } = await client.pair(63344, "482 913", "Chrome on Linux");
    expect(token).toBe("t0k");
    const outcome = await client.open(63344, token, { file: "/home/me/shop/src/App.tsx", line: 3 });
    expect(outcome.status).toBe("opened");
    const open = calls[calls.length - 1];
    expect((open.init.headers as Record<string, string>)["Content-Type"]).toBe("application/json");
  });

  it("reports Forge's error messages", async () => {
    const client = new ForgeClient(fakeForge());
    await expect(client.pair(63344, "000000", "x")).rejects.toThrow("wrong pairing code");
    const err = await client.open(63344, "bad", { file: "/a" }).catch((e) => e);
    expect(err).toBeInstanceOf(ForgeError);
    expect(err.status).toBe(401);
    await expect(client.open(63342, "t0k", { file: "/a" })).rejects.toThrow("Forge is not running");
  });
});

describe("describeBrowser", () => {
  it("names common browsers", () => {
    expect(describeBrowser("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/141.0.0.0 Safari/537.36")).toBe("Chrome 141 on Linux");
    expect(describeBrowser("Mozilla/5.0 (Windows NT 10.0) Chrome/140.0 Safari/537.36 Edg/140.0")).toBe("Edge 140 on Windows");
    expect(describeBrowser("Mozilla/5.0 (Macintosh; Intel Mac OS X 14.5; rv:131.0) Gecko/20100101 Firefox/131.0")).toBe("Firefox 131 on macOS");
  });
});
