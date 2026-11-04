import { describe, expect, it } from "vitest";
import type { DescriptorProblem, MinecraftCompletion, MinecraftMarker, MinecraftTarget } from "@/ipc/minecraft";
import type { GeneratorChoice, Plan, ProjectInfo } from "@/ipc/project";

// The Minecraft demo is chosen by the page URL, before the mocks load.
history.replaceState(null, "", "/?project=minecraft");
const { invoke } = await import("@/ipc");
await import("./settingsMock");
const { mockFiles } = await import("./workspaceMock");
await import("./projectMock");
await import("./minecraftMock");

const YML = "src/main/resources/plugin.yml";
const PKG = "src/main/java/com/example/betterchat";

describe("Minecraft mock (javaforge-minecraft through WebAssembly)", () => {
  it("detects the demo as a Paper plugin", async () => {
    const info = await invoke<ProjectInfo>("project_detect");
    expect(info.primary).toMatchObject({ id: "minecraft", name: "Paper Plugin", version: "1.21.8" });
  });

  it("plans the generator's files with hidden options at their defaults", async () => {
    const args = { generator: "minecraft-plugin", location: "~/Projects", name: "better-chat" };
    const plan = await invoke<Plan>("project_plan", { ...args, options: { language: "kotlin", buildSystem: "maven" } });
    expect(plan.error).toBeUndefined();
    expect(plan.files).toContain("pom.xml");
    expect(plan.files).toContain(`${PKG}/BetterChat.java`);
    const bad = await invoke<Plan>("project_plan", { ...args, options: { mainClass: "Main" } });
    expect(bad.error).toContain("Main class");
  });

  it("refreshes versions without the network", async () => {
    const choices = await invoke<GeneratorChoice[]>("minecraft_versions_refresh");
    expect(choices[0]).toEqual({ value: "26.3", label: "26.3 (Java 25)" });
    expect(choices.some((c) => c.value === "26.1")).toBe(true);
    expect(choices.some((c) => c.value === "1.16.5")).toBe(true);
  });

  it("checks and navigates the main class of plugin.yml", async () => {
    const content = mockFiles[YML];
    expect(await invoke<DescriptorProblem[]>("minecraft_inspect", { path: YML, content })).toEqual([]);
    const broken = content.replace("main: com.example.betterchat.BetterChat", "main: com.example.betterchat.Missing");
    const problems = await invoke<DescriptorProblem[]>("minecraft_inspect", { path: YML, content: broken });
    expect(problems.map((p) => p.message)).toEqual(["Cannot resolve class 'com.example.betterchat.Missing'"]);
    const pos = content.indexOf("com.example.betterchat.BetterChat") + 5;
    const targets = await invoke<MinecraftTarget[]>("minecraft_goto", { path: YML, content, pos });
    expect(targets).toEqual([{ path: `${PKG}/BetterChat.java`, line: 8, name: "BetterChat" }]);
  });

  it("marks event handlers and the main class", async () => {
    const path = `${PKG}/listener/ChatListener.java`;
    const marks = await invoke<MinecraftMarker[]>("minecraft_gutter", { path, content: mockFiles[path] });
    expect(marks.map((m) => m.name)).toEqual(["AsyncChatEvent", "BlockBreakEvent"]);
    const main = `${PKG}/BetterChat.java`;
    const mainMarks = await invoke<MinecraftMarker[]>("minecraft_gutter", { path: main, content: mockFiles[main] });
    expect(mainMarks[0]).toMatchObject({ kind: "plugin", name: "BetterChat" });
  });

  it("plans a mod and refreshes the mod versions", async () => {
    const args = { generator: "minecraft-mod", location: "~/Projects", name: "better-chests" };
    const plan = await invoke<Plan>("project_plan", { ...args, options: { loader: "neoforge", language: "kotlin" } });
    expect(plan.error).toBeUndefined();
    expect(plan.files).toContain("src/main/templates/META-INF/neoforge.mods.toml");
    expect(plan.files).toContain("src/main/resources/assets/better_chests/textures/item/example_item.png");
    const bad = await invoke<Plan>("project_plan", { ...args, options: { modId: "Better" } });
    expect(bad.error).toContain("Mod ID");
    const choices = await invoke<GeneratorChoice[]>("minecraft_versions_refresh", { generator: "minecraft-mod" });
    expect(choices[0]).toEqual({ value: "26.3", label: "26.3 (Java 25)" });
    expect(choices.some((c) => c.value === "1.21.5")).toBe(true);
    expect(choices.some((c) => c.value === "1.16.5")).toBe(false);
  });

  it("checks fabric.mod.json entrypoints and completes neoforge.mods.toml", async () => {
    const path = "src/main/resources/fabric.mod.json";
    const content = '{ "entrypoints": { "main": ["com.example.demo.Demo"] } }';
    const problems = await invoke<DescriptorProblem[]>("minecraft_inspect", { path, content });
    expect(problems.map((p) => p.code)).toEqual(["minecraft.modClass.unresolved"]);
    const toml = 'modLoader = "javafml"\n[[mods]]\nmodId = "demo"\ndis';
    const completion = await invoke<MinecraftCompletion | null>("minecraft_complete", {
      path: "src/main/templates/META-INF/neoforge.mods.toml",
      content: toml,
      pos: toml.length,
    });
    expect(completion?.items.map((i) => i.label)).toContain("displayName");
    expect(completion?.from).toBe(toml.length - 3);
  });
});
