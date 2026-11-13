import { describe, expect, it } from "vitest";
import "./aiMock";
import { aiApi } from "@/ipc/ai";

describe("browser-mode AI backend", () => {
  it("completes with the shared contract", async () => {
    const text = await aiApi.complete({
      system: "Write a git commit message",
      messages: [{ role: "user", content: "diff" }],
      maxTokens: 200,
    });
    expect(text).toMatch(/^Fix /);
  });

  it("streams deltas through the async iterator", async () => {
    let text = "";
    for await (const d of aiApi.stream({ messages: [{ role: "user", content: "Explain User" }] })) text += d;
    expect(text).toContain("app/Models/User.php");
  });

  it("cancels when the consumer stops iterating", async () => {
    const parts: string[] = [];
    for await (const d of aiApi.stream({ messages: [{ role: "user", content: "Hi" }] })) {
      parts.push(d);
      if (parts.length === 2) break;
    }
    expect(parts).toHaveLength(2);
  });

  it("reports errors from unreachable local servers", async () => {
    const handle = await aiApi.streamWithCallbacks({ provider: "lmstudio", messages: [{ role: "user", content: "Hi" }] });
    await expect(handle.result).rejects.toThrow(/is LM Studio running\?/);
  });

  it("manages providers", async () => {
    const saved = await aiApi.saveProvider(
      { id: "", kind: "ollama", name: "Ollama", baseUrl: "http://gpu-box:11434/v1", enabled: true, headers: {} },
      undefined,
    );
    expect(saved.id).toBe("ollama-2");
    await aiApi.setDefault(saved.id, "llama3.1:8b");
    let state = await aiApi.providers();
    expect(state.defaultProvider).toBe("ollama-2");
    expect(state.providers.find((p) => p.id === "anthropic")?.keySource).toBe("keychain");
    const test = await aiApi.testProvider(saved.id);
    expect(test.ok).toBe(true);
    await aiApi.deleteProvider(saved.id);
    state = await aiApi.providers();
    expect(state.defaultProvider).toBe("anthropic");
  });
});
