import { beforeAll, describe, expect, it, vi } from "vitest";
import { registerMock } from "@/ipc";
import { closeAllTabs, editor, openFile, updateContent } from "@/state/editor";
import { onFsChanged } from "@/state/fsWatch";
import { adoptWorkspace } from "@/state/workspace";
import { installSystemBehaviour } from "./index";

const written: Record<string, string> = {};

beforeAll(() => {
  registerMock("fs_read_text", ({ path }) => ({ text: `<?php // ${path}\n`, eol: "\n", encoding: "UTF-8", bom: false, malformed: false }));
  registerMock("fs_write_text", ({ path, text }) => {
    written[String(path)] = String(text);
  });
  registerMock("text_prepare_save", ({ request }) => ({ text: (request as { text: string }).text, changed: false }));
  installSystemBehaviour();
});

describe("system settings", () => {
  it("saves modified files when Forge loses the focus", async () => {
    closeAllTabs();
    await openFile("a.php");
    updateContent("a.php", "<?php echo 1;\n");
    window.dispatchEvent(new Event("blur"));
    await vi.waitFor(() => expect(written["a.php"]).toBe("<?php echo 1;\n"));
    expect(editor.tabs[0].saved).toBe("<?php echo 1;\n");
  });

  it("re-checks open files and git when Forge gets the focus back", async () => {
    adoptWorkspace({ name: "p", root: "/p" });
    await openFile("b.php");
    const seen: string[][] = [];
    const off = onFsChanged((c) => seen.push([...c.changed, String(c.git)]));
    window.dispatchEvent(new Event("focus"));
    off();
    expect(seen).toEqual([["b.php", "true"]]);
  });
});
