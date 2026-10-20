import { describe, expect, it } from "vitest";
import { DEFAULT_KEYMAPS, PHPSTORM_ACTIONS, type KeymapId } from "./phpstormKeymap";
import { canonicalShortcut, tryParseShortcut } from "./shortcut";
import { allConflicts, buildBindings } from "./engine";

const contextOf = (id: string) => PHPSTORM_ACTIONS.find((a) => a.id === id)?.context ?? "any";
const sc = (km: KeymapId, id: string) => DEFAULT_KEYMAPS[km][id].map((s) => canonicalShortcut(s));

describe("PHPStorm default keymaps", () => {
  it("covers the full action list", () => {
    expect(PHPSTORM_ACTIONS.length).toBeGreaterThanOrEqual(150);
    expect(new Set(PHPSTORM_ACTIONS.map((a) => a.id)).size).toBe(PHPSTORM_ACTIONS.length);
  });

  it.each(["$default", "macOS"] as KeymapId[])("every %s shortcut parses", (km) => {
    for (const [id, list] of Object.entries(DEFAULT_KEYMAPS[km])) {
      for (const s of list) expect(tryParseShortcut(s), `${id}: ${s}`).not.toBeNull();
    }
  });

  it("matches PHPStorm Windows/Linux defaults", () => {
    const expectations: Record<string, string> = {
      SearchEverywhere: "Double Shift",
      GotoAction: "Ctrl+Shift+A",
      GotoClass: "Ctrl+N",
      GotoFile: "Ctrl+Shift+N",
      GotoSymbol: "Ctrl+Alt+Shift+N",
      RecentFiles: "Ctrl+E",
      RecentLocations: "Ctrl+Shift+E",
      Generate: "Alt+Insert",
      ShowIntentionActions: "Alt+Enter",
      SelectNextOccurrence: "Alt+J",
      EditorSelectWord: "Ctrl+W",
      EditorUnSelectWord: "Ctrl+Shift+W",
      RenameElement: "Shift+F6",
      "Refactorings.QuickListPopupAction": "Ctrl+Alt+Shift+T",
      CheckinProject: "Ctrl+K",
      "Vcs.Push": "Ctrl+Shift+K",
      "Vcs.UpdateProject": "Ctrl+T",
      ActivateProjectToolWindow: "Alt+1",
      ActivateTerminalToolWindow: "Alt+F12",
      HideAllWindows: "Ctrl+Shift+F12",
      ShowSettings: "Ctrl+Alt+S",
      NewScratchFile: "Ctrl+Alt+Shift+Insert",
      CloseContent: "Ctrl+F4",
      NextTab: "Alt+Right",
      PreviousTab: "Alt+Left",
      FileStructurePopup: "Ctrl+F12",
      QuickJavaDoc: "Ctrl+Q",
      ParameterInfo: "Ctrl+P",
      EditorCompleteStatement: "Ctrl+Shift+Enter",
      SurroundWith: "Ctrl+Alt+T",
      InsertLiveTemplate: "Ctrl+J",
      EditorDuplicate: "Ctrl+D",
      EditorDeleteLine: "Ctrl+Y",
      ReformatCode: "Ctrl+Alt+L",
      OptimizeImports: "Ctrl+Alt+O",
      ToggleLineBreakpoint: "Ctrl+F8",
      Run: "Shift+F10",
      Debug: "Shift+F9",
    };
    for (const [id, s] of Object.entries(expectations)) expect(sc("$default", id), id).toContain(canonicalShortcut(s));
  });

  it("matches PHPStorm macOS defaults", () => {
    const expectations: Record<string, string> = {
      GotoAction: "Cmd+Shift+A",
      GotoClass: "Cmd+O",
      GotoFile: "Cmd+Shift+O",
      ShowSettings: "Cmd+,",
      EditorSelectWord: "Alt+Up",
      SelectNextOccurrence: "Ctrl+G",
      Generate: "Cmd+N",
      ActivateProjectToolWindow: "Cmd+1",
      CloseContent: "Cmd+W",
      Run: "Ctrl+R",
      "Refactorings.QuickListPopupAction": "Ctrl+T",
    };
    for (const [id, s] of Object.entries(expectations)) expect(sc("macOS", id), id).toContain(canonicalShortcut(s));
  });

  it.each(["$default", "macOS"] as KeymapId[])("%s has no unintended same-context conflicts", (km) => {
    const nonNative = Object.fromEntries(
      Object.entries(DEFAULT_KEYMAPS[km]).filter(([id]) => !PHPSTORM_ACTIONS.find((a) => a.id === id)?.native),
    );
    const conflicts = [...allConflicts(buildBindings(nonNative, contextOf, km === "macOS" ? "mac" : "other")).entries()];
    // PHPStorm itself binds these pairs on purpose (different UI contexts in the IDE).
    const known = new Set([
      "Ctrl+Alt+Z", // Rollback / Rollback Lines
      "Meta+Alt+Z",
      "Alt+Meta+Z",
      "Meta+N", // New… / Generate (editor)
      "Alt+Insert",
      "Ctrl+Shift+Tab",
    ]);
    const unexpected = conflicts.filter(([id]) => !known.has(id));
    expect(unexpected).toEqual([]);
  });
});
