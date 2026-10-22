import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { AuditResult, ComposerProject, OutdatedMarker, PackageDetails, SearchHit } from "@/ipc/composer";
import type { SchemaCompletion, SchemaHover } from "@/ipc/jsonSchema";
import { formatDownloads, updateCommand } from "@/features/composer/model";
import { composerConfiguration, isComposerJson, requireCommand, siblingOf } from "@/features/composer/state";
import { demoFiles } from "./demoProject";
import "./composerMock";

const JSON_TEXT = demoFiles["composer.json"];

describe("Composer mock (Rust analysis through WebAssembly)", () => {
  it("reads requirements and the lock file", async () => {
    const p = await invoke<ComposerProject>("composer_project");
    expect(p.exists && p.hasLock).toBe(true);
    expect(p.requirements.map((r) => r.name)).toEqual(["php", "laravel/framework", "doctrine/dbal", "guzzlehttp/guzzle", "phpunit/phpunit"]);
    expect(p.installed.find((i) => i.name === "phpunit/phpunit")).toMatchObject({ version: "11.4.0", dev: true });
  });

  it("marks outdated packages", async () => {
    const markers = await invoke<OutdatedMarker[]>("composer_markers", { path: "composer.json", text: JSON_TEXT });
    expect(markers.map((m) => [m.name, m.status, m.suggested])).toEqual([
      ["laravel/framework", "semver-safe-update", "^12.0"],
      ["phpunit/phpunit", "update-possible", "^12.0"],
    ]);
    const laravel = markers[0];
    expect(JSON_TEXT.slice(laravel.from, laravel.to)).toBe('"^12.0"');
    expect(laravel.message).toBe("Package 'laravel/framework' is outdated: v12.1.0 is installed, v12.3.0 is available");
  });

  it("completes package names and versions and shows the hover", async () => {
    const text = JSON_TEXT.replace('"doctrine/dbal": "^4.0"', '"doctrine/dbal": "^4.0",\n        "laravel/sa"');
    const offset = text.indexOf('"laravel/sa"') + 10;
    const names = await invoke<SchemaCompletion | null>("composer_complete", { path: "composer.json", text, offset });
    expect(names?.items.map((i) => i.label)).toContain("laravel/sanctum");

    const at = JSON_TEXT.indexOf('"^4.0"') + 1;
    const versions = await invoke<SchemaCompletion | null>("composer_complete", { path: "composer.json", text: JSON_TEXT, offset: at });
    expect(versions?.items[0]).toMatchObject({ label: "^4.2", detail: "latest: 4.2.3" });

    const hover = await invoke<SchemaHover | null>("composer_hover", {
      path: "composer.json",
      text: JSON_TEXT,
      offset: JSON_TEXT.indexOf("laravel/framework") + 3,
    });
    expect(hover?.title).toBe("laravel/framework");
    expect(hover?.markdown).toContain("Installed: `v12.1.0` · Latest: `v12.3.0`");
  });

  it("searches the demo catalog and audits the lock file", async () => {
    const hits = await invoke<SearchHit[]>("composer_search", { query: "laravel" });
    expect(hits[0].name).toBe("laravel/framework");
    const details = await invoke<PackageDetails>("composer_package", { name: "laravel/sanctum" });
    expect(details.suggested).toBe("^4.0");

    const audit = await invoke<AuditResult>("composer_audit");
    expect(audit.advisories.map((a) => a.packageName)).toEqual(["guzzlehttp/psr7", "laravel/framework"]);
    expect(audit.jsonProblems.map((p) => p.package)).toEqual(["laravel/framework"]);
    expect(audit.lockProblems).toHaveLength(2);
  });
});

describe("Composer UI model", () => {
  it("builds Composer commands", () => {
    expect(updateCommand({ name: "a/b", status: "semver-safe-update", suggested: "^1.0", dev: false })).toBe("update a/b --with-dependencies");
    expect(updateCommand({ name: "a/b", status: "update-possible", suggested: "^2.0", dev: true })).toBe("require a/b:^2.0 --dev");
    expect(requireCommand("a/b", ">=1.0 <2.0", false)).toBe('require "a/b:>=1.0 <2.0"');
    expect(requireCommand("a/b", "", false)).toBe("require a/b");
    expect(composerConfiguration("install", "api/composer.json")).toMatchObject({
      type: "composer-command",
      workingDirectory: "api",
      options: { command: "install" },
    });
  });

  it("recognizes project composer.json files", () => {
    expect(isComposerJson("composer.json")).toBe(true);
    expect(isComposerJson("api/composer.json")).toBe(true);
    expect(isComposerJson("vendor/laravel/framework/composer.json")).toBe(false);
    expect(siblingOf("api/composer.json", "composer.lock")).toBe("api/composer.lock");
    expect(formatDownloads(431_208_771)).toBe("431M");
    expect(formatDownloads(2_812)).toBe("2.8K");
  });
});
