import { describe, expect, it } from "vitest";
import "@/mock/settingsMock";
import "@/mock/codeStyleMock";
import { codeStyleApi } from "@/ipc/codeStyle";
import { settingsApi } from "@/ipc/settings";
import {
  applyDraft,
  applyPreset,
  codeStyle,
  copyAs,
  copyToProject,
  current,
  deleteCurrent,
  discardDraft,
  freeName,
  hasChanges,
  loadSchemes,
  nameProblem,
  pageModified,
  resetPage,
  setValue,
  valueOf,
} from "./store";

describe("code style schemes (browser mode)", () => {
  it("edits, resets and applies schemes", async () => {
    await loadSchemes();
    expect(current()?.id).toBe("default");
    expect(valueOf("php", "indent")).toBe(4);
    expect(hasChanges()).toBe(false);

    setValue("php", "indent", 2);
    expect(pageModified("editor.code-style.php")).toBe(true);
    expect(pageModified("editor.code-style.json")).toBe(false);
    expect(pageModified("editor.code-style")).toBe(false);
    resetPage("editor.code-style.php");
    expect(hasChanges()).toBe(false);

    // Set from… WordPress, then copy to the project and apply.
    applyPreset("php", "wordpress");
    expect(valueOf("php", "useTabs")).toBe(true);
    expect(valueOf("php", "codeConversion.arraySyntax")).toBe("long");
    copyToProject();
    expect(current()?.level).toBe("project");
    expect(pageModified("editor.code-style")).toBe(true);
    await applyDraft();
    expect(hasChanges()).toBe(false);
    const all = await settingsApi.getAll();
    // Set from… changed the Default scheme; the Project scheme only stores what differs from
    // it, so the project keeps just the selection.
    expect(all.user["php.codeStyle.useTabs"]).toBe(true);
    expect(all.project["php.codeStyle.useTabs"]).toBeUndefined();
    expect(all.project["editor.codeStyle.scheme"]).toBe("project");
    expect((await codeStyleApi.schemes()).selected).toBe("project");

    // A new IDE scheme, then delete it again before applying.
    expect(freeName("Project")).toBe("Project");
    copyAs("Team");
    expect(current()?.name).toBe("Team");
    expect(nameProblem("team")).toMatch(/already exists/);
    deleteCurrent();
    expect(current()?.id).toBe("default");
    discardDraft();
    expect(codeStyle.draft?.selected).toBe("project");

    // Clean up the shared mock layers.
    applyPreset("php", "psr12");
    await applyDraft();
  });

  it("previews and round-trips exports", async () => {
    const values = { "php.useTabs": true, "php.tabSize": 4, "php.indent": 4 };
    const preview = await codeStyleApi.preview("php", values);
    expect(preview).toContain("\n\tpublic function index(");
    const xml = await codeStyleApi.export("Tabs", values, "xml");
    const back = await codeStyleApi.import({ text: xml });
    expect(back.name).toBe("Tabs");
    expect(back.values["php.useTabs"]).toBe(true);
  });
});
