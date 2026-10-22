import { describe, expect, it } from "vitest";
import type { TemplateInfo } from "@/ipc/templates";
import { newMenuGroups, newMenuLabel, offeredTemplates, phpClassTemplates, templateLabel, validateClassName, validateNamespace, variableLabel } from "./model";
import { freeName, withChange } from "./FileTemplatesPage";

const t = (name: string, extra: Partial<TemplateInfo> = {}): TemplateInfo => ({
  kind: "file",
  name,
  extension: "php",
  text: "",
  builtin: true,
  modified: false,
  group: "php",
  phpClass: false,
  variables: [],
  references: [],
  ...extra,
});

const ALL = [
  t("PHP File", { when: "php" }),
  t("PHP Class", { when: "php", phpClass: true }),
  t("PHP Interface", { when: "php", phpClass: true }),
  t("PHP Trait", { when: "php", phpClass: true }),
  t("PHP Enum", { when: "php", phpClass: true }),
  t("Laravel Model", { when: "laravel", group: "framework", phpClass: true }),
  t("Symfony Controller", { when: "symfony", group: "framework", phpClass: true }),
  t("HTML File", { extension: "html", group: "web" }),
  t("Lua File", { extension: "lua", when: "lua", group: "lua" }),
  t("Action", { builtin: false, group: "user", phpClass: true }),
  t("PHP File Header", { kind: "include" }),
];

describe("file templates model", () => {
  const laravel = (type: string) => ["php", "laravel"].includes(type);

  it("offers the templates of the project's languages and frameworks, grouped", () => {
    const groups = offeredTemplates(ALL, laravel).map((g) => g.map((x) => x.name));
    expect(groups).toEqual([["PHP File", "PHP Class"], ["Laravel Model"], ["HTML File"], ["Action"]]);
    expect(offeredTemplates(ALL, () => false).flat().map((x) => x.name)).toEqual(["HTML File", "Action"]);
  });

  it("groups the New menu into PHP, Web and Lua submenus with the class kinds first", () => {
    const groups = newMenuGroups(ALL, laravel).map((g) => ({
      label: g.label,
      sections: g.sections.map((s) => s.map((x) => newMenuLabel(x, g.label))),
    }));
    expect(groups).toEqual([
      { label: "PHP", sections: [["Class", "Interface", "Trait", "Enum"], ["File"], ["Laravel Model"]] },
      { label: "Web", sections: [["HTML File"]] },
      { label: "", sections: [["Action"]] },
    ]);
    expect(newMenuGroups(ALL, () => false).map((g) => g.label)).toEqual(["Web", ""]);
  });

  it("lists the class kinds and project templates in the New PHP Class dialog", () => {
    const list = phpClassTemplates(ALL, ALL[5], laravel).map(templateLabel);
    expect(list).toEqual(["Class", "Interface", "Trait", "Enum", "Laravel Model", "Action"]);
  });

  it("validates names and labels variables", () => {
    expect(validateClassName("Invoice")).toBeNull();
    expect(validateClassName("Über_1")).toBeNull();
    expect(validateClassName("1Invoice")).toMatch(/not a valid class name/);
    expect(validateClassName("")).toBeNull();
    expect(validateNamespace("\\App\\Models\\")).toBeNull();
    expect(validateNamespace("App\\9")).toMatch(/not a valid namespace/);
    expect(variableLabel("BACKED_TYPE")).toBe("Backed type");
  });
});

describe("File and Code Templates page", () => {
  const cls = t("PHP Class", { text: "A", defaultText: "A", defaultExtension: "php" });

  it("stores changed built-ins and drops them when they are back at their default", () => {
    const changed = withChange([], cls, { text: "B" });
    expect(changed).toEqual([{ kind: "file", name: "PHP Class", extension: "php", text: "B" }]);
    expect(withChange(changed, { ...cls, text: "B" }, { text: "A" })).toEqual([]);
  });

  it("keeps the position of edited user templates and renames them", () => {
    const user = t("Job", { builtin: false, text: "x" });
    const entries = [
      { kind: "file" as const, name: "Job", extension: "php", text: "x" },
      { kind: "code" as const, name: "Other", text: "y" },
    ];
    expect(withChange(entries, user, { name: "Task" })[0]).toEqual({ kind: "file", name: "Task", extension: "php", text: "x" });
    expect(withChange(entries, user, { text: "z" }).map((e) => e.text)).toEqual(["z", "y"]);
  });

  it("picks free names", () => {
    expect(freeName("Unnamed", [])).toBe("Unnamed");
    expect(freeName("Unnamed", ["Unnamed", "Unnamed (2)"])).toBe("Unnamed (3)");
  });
});
