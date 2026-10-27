import { describe, expect, it } from "vitest";
import { moduleOf, megabytes } from "./store";
import { isKotlinConfiguration } from "./runTypes";
import { isKotlinSource } from "./lint";

describe("KotlinForge UI helpers", () => {
  it("finds the build module of a source file", () => {
    expect(moduleOf("app/src/main/kotlin/com/acme/Main.kt")).toBe("app");
    expect(moduleOf("src/main/kotlin/com/acme/Main.kt")).toBe("");
    expect(moduleOf("core/util/src/test/kotlin/A.kt")).toBe("core/util");
    expect(megabytes(368_488_700)).toBe("368 MB");
  });

  it("tells Kotlin configurations and sources apart", () => {
    expect(isKotlinConfiguration({ name: "MainKt", type: "java-application", options: { kotlin: "true" } })).toBe(true);
    expect(isKotlinConfiguration({ name: "App", type: "java-application", options: {} })).toBe(false);
    expect(isKotlinConfiguration({ name: "CartSpec", type: "kotest" })).toBe(true);
    expect(isKotlinConfiguration({ name: "T", type: "junit", options: { language: "kotlin" } })).toBe(true);
    expect(isKotlinConfiguration({ name: "T", type: "junit", options: {} })).toBe(false);
    expect(isKotlinSource("src/main/kotlin/A.kt")).toBe(true);
    expect(isKotlinSource("scripts/release.main.kts")).toBe(true);
    expect(isKotlinSource("build.gradle.kts")).toBe(false);
  });
});
