import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { NewFile, TemplateCatalog } from "@/ipc/templates";
import { mockFiles } from "./workspaceMock";
import "./templatesMock";

describe("file templates mock (forge-file-templates through WebAssembly)", () => {
  it("lists the built-ins and a draft's changes", async () => {
    const saved = await invoke<TemplateCatalog>("templates_list", {});
    expect(saved.templates.find((t) => t.name === "PHP Class")).toMatchObject({ kind: "file", phpClass: true, modified: false });
    expect(saved.variables.map((v) => v.name)).toContain("NAMESPACE");

    const draft = await invoke<TemplateCatalog>("templates_list", {
      custom: [{ kind: "file", name: "PHP Class", text: "<?php\nfinal class ${NAME} {}\n" }, { kind: "file", name: "Job", extension: "php", text: "#if ($X)" }],
    });
    expect(draft.templates.find((t) => t.name === "PHP Class")).toMatchObject({ modified: true, phpClass: false });
    expect(draft.templates.find((t) => t.name === "Job")).toMatchObject({ builtin: false, group: "user", error: "Line 1: Missing #end for #if" });
    expect(await invoke("templates_validate", { text: "#end" })).toBe("Line 1: #end without #if");
  });

  it("maps folders and namespaces with the demo composer.json", async () => {
    expect(await invoke("templates_php_namespace", { dir: "app/Http/Controllers" })).toBe("App\\Http\\Controllers");
    expect(await invoke("templates_php_namespace", { dir: "resources/views" })).toBeNull();
    expect(await invoke("templates_php_directory", { namespace: "App\\Models\\Billing" })).toBe("app/Models/Billing");
  });

  it("creates the file in memory and refuses an existing one", async () => {
    const request = { template: "PHP Class", dir: "app/Models", name: "Invoice", namespace: "App\\Models", extends: "\\Illuminate\\Database\\Eloquent\\Model" };
    const file = await invoke<NewFile>("templates_create", { request });
    expect(file.path).toBe("app/Models/Invoice.php");
    expect(mockFiles["app/Models/Invoice.php"]).toBe(
      "<?php\n\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\n\nclass Invoice extends Model\n{\n\n}\n",
    );
    await expect(invoke("templates_create", { request })).rejects.toThrow(/already exists/);
  });
});
