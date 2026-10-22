import { describe, expect, it } from "vitest";
import { hintText, isPhpFile, isWebScript, optimizesImports, splitFqn, webImportModule } from "./model";
import { completedClass } from "./completion";
import { describeTargets } from "./OptimizeImportsDialog";
import { mockImportClassAction, mockUnresolved } from "@/mock/importsMock";

describe("imports model", () => {
  it("knows the files it handles", () => {
    expect(isPhpFile("app/User.php")).toBe(true);
    expect(isPhpFile("resources/views/a.blade.php")).toBe(false);
    expect(isWebScript("src/App.vue")).toBe(true);
    expect(isWebScript("types/env.d.ts")).toBe(false);
    expect(optimizesImports("README.md")).toBe(false);
  });

  it("formats the import popup", () => {
    expect(hintText(["App\\Models\\User"], "Alt+Enter")).toBe("? App\\Models\\User Alt+Enter");
    expect(hintText(["A\\Order", "B\\Order"], "Alt+Enter")).toBe("? Multiple choices… Alt+Enter");
    expect(splitFqn("\\App\\Models\\User")).toEqual({ name: "User", namespace: "App\\Models" });
    expect(splitFqn("Exception")).toEqual({ name: "Exception", namespace: "" });
    expect(webImportModule('Add import from "./utils"')).toBe("./utils");
    expect(webImportModule('Update import from "vue"')).toBe("vue");
    expect(webImportModule("Remove unused declaration")).toBeNull();
  });

  it("finds the class a PHP completion inserts", () => {
    const item = { kind: "class", label: "User", detail: "App\\Models\\User", additionalTextEdits: [] };
    expect(completedClass(item, "User")).toBe("App\\Models\\User");
    const fromEdit = { ...item, detail: "class", additionalTextEdits: [{ range: { start: { line: 2, character: 0 }, end: { line: 2, character: 0 } }, newText: "use Illuminate\\Support\\Str;\n" }] };
    expect(completedClass(fromEdit, "Str")).toBe("Illuminate\\Support\\Str");
    expect(completedClass({ ...item, detail: null }, "\\Carbon\\Carbon")).toBe("Carbon\\Carbon");
    expect(completedClass({ ...item, kind: "method" }, "User")).toBeNull();
    expect(completedClass({ ...item, detail: "Some text" }, "User")).toBeNull();
  });

  it("describes what the dialog optimizes", () => {
    expect(describeTargets([{ path: "", dir: true }], "shop")).toBe("Optimize imports in project 'shop'");
    expect(describeTargets([{ path: "app/Models", dir: true }], "shop")).toBe("Optimize imports in directory 'app/Models'");
    expect(describeTargets([{ path: "app/Models/User.php", dir: false }], "shop")).toBe("Optimize imports in file 'User.php'");
    expect(describeTargets([{ path: "a.php", dir: false }, { path: "b.ts", dir: false }], "shop")).toBe("Optimize imports in 2 files");
  });
});

describe("imports mock", () => {
  const src = "<?php\n\nnamespace App\\Http;\n\nuse App\\Models\\User;\n\nclass C\n{\n    public function f(User $u): Collection\n    {\n        return Str::of('x');\n    }\n}\n";

  it("finds unresolved names with candidates", () => {
    const list = mockUnresolved(src);
    const names = list.map((u) => [u.name, u.candidates]);
    expect(names).toContainEqual(["Collection", ["Illuminate\\Database\\Eloquent\\Collection", "Illuminate\\Support\\Collection"]]);
    expect(names).toContainEqual(["Str", ["Illuminate\\Support\\Str"]]);
    expect(names.map((n) => n[0])).not.toContain("User");
  });

  it("offers Import class with a chooser when ambiguous", () => {
    const collection = src.indexOf("Collection");
    expect(mockImportClassAction("app/Http/C.php", src, collection)?.action).toBe("PhpImportClass");
    const str = mockImportClassAction("app/Http/C.php", src, src.indexOf("Str::"));
    expect(str?.edit?.changes[0]).toMatchObject({ kind: "edit", path: "app/Http/C.php" });
  });
});
