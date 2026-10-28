import { describe, expect, it } from "vitest";
import "./aiMock";
import "./aiEditorMock";
import { aiEditorApi } from "@/ipc/aiEditor";
import { setSetting } from "./settingsMock";
import { mockFiles } from "./workspaceMock";

const PHP = "<?php\n\nclass Cart\n{\n    public function total(array $items): int\n    {\n        return \n    }\n}\n";
const caret = PHP.indexOf("return ") + "return ".length;

describe("browser-mode AI in the editor", () => {
  it("asks nothing while inline completion is off", async () => {
    setSetting("ai.completion.enabled", false, "user");
    const a = await aiEditorApi.inlineComplete({ path: "app/Cart.php", text: PHP, offset: caret }, "t1");
    expect(a).toEqual({ text: null, reason: "disabled" });
  });

  it("cleans the fake model's answer with the Rust post-processing", async () => {
    setSetting("ai.completion.enabled", true, "user");
    // The canned answer echoes the line inside a fence; only the new text is left.
    const a = await aiEditorApi.inlineComplete({ path: "app/Cart.php", text: PHP, offset: caret }, "t2");
    expect(a.text).toBe("$this->items->sum(fn (Item $item) => $item->price * $item->quantity);");
    const alt = await aiEditorApi.inlineComplete({ path: "app/Cart.php", text: PHP, offset: caret }, "t2", 0.1);
    expect(alt.text).toMatch(/^array_sum/);
  });

  it("respects .aiignore and the per-language switch", async () => {
    mockFiles[".aiignore"] = "secrets/\n";
    const ignored = await aiEditorApi.inlineComplete({ path: "secrets/Keys.php", text: PHP, offset: caret }, "t3");
    expect(ignored.reason).toBe("secrets/Keys.php is excluded from AI by .aiignore");
    await expect(aiEditorApi.action({ kind: "docs", path: "secrets/Keys.php", text: PHP, from: caret, to: caret })).rejects.toThrow(/aiignore/);
    delete mockFiles[".aiignore"];
    setSetting("ai.completion.disabledLanguages", ["php"], "user");
    const status = await aiEditorApi.status("app/Cart.php");
    expect(status.language).toBe("php");
    expect(status.file).toEqual({ status: "languageDisabled", message: "Inline completion is turned off for php" });
    expect(status.fileActions).toEqual({ status: "allowed" });
    expect(status.completion?.name).toBe("Ollama");
    expect(status.completion?.fim).toBe(true);
    setSetting("ai.completion.disabledLanguages", null, "user");
  });

  it("proposes documentation and tests as diffs", async () => {
    const docs = await aiEditorApi.action({ kind: "docs", path: "app/Cart.php", text: PHP, from: caret, to: caret });
    expect(docs.edit?.text).toContain("     * @param array $items The items.\n     * @return int\n     */");
    expect(docs.diff.added).toBeGreaterThan(3);
    const tests = await aiEditorApi.action({ kind: "tests", path: "app/Cart.php", text: PHP, from: caret, to: caret });
    expect(tests.path).toBe("tests/Unit/CartTest.php");
    expect(tests.newFile).toBe(true);
    expect(tests.proposed).toContain("class CartTest extends TestCase");
  });

  it("fixes the problem at the caret", async () => {
    const fixed = await aiEditorApi.action({
      kind: "fix",
      path: "app/Cart.php",
      text: PHP.replace("return \n", "return 0\n"),
      from: caret,
      to: caret,
      problems: [{ message: "Syntax error: unexpected '}'", line: 6 }],
    });
    expect(fixed.edit?.text).toContain("return 0;");
    expect(fixed.diff).toMatchObject({ added: 1, removed: 1 });
  });

  it("streams an explanation", async () => {
    let text = "";
    const done = new Promise<void>((resolve) => {
      void aiEditorApi.explain({ kind: "explain", path: "app/Cart.php", text: PHP, from: caret, to: caret }, (ev) => {
        if (ev.delta) text += ev.delta;
        if (ev.done) resolve();
      });
    });
    await done;
    expect(text).toContain("`total`");
  });
});
