import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { ExtractComponentInfo, RefactorAnswer, RefactorThisAnswer } from "@/ipc/refactor";
import { componentPath, validateComponentName } from "@/features/codeintel/extractComponent";
import { positionOf } from "./codeintelMock";
import "./workspaceMock";
import "./refactorHierarchyMock";
import "./extractComponentMock";

const VUE = "<template>\n  <section>\n    <StatCard :value=\"store.revenue\" />\n    <p>{{ label }}</p>\n  </section>\n</template>\n\n<script setup lang=\"ts\">\nimport StatCard from './StatCard.vue'\n</script>\n";
const TSX = 'export function Cart({ items }: { items: Item[] }) {\n  return (\n    <ul>\n      {items.map((i) => (\n        <li key={i.id}>{i.name}</li>\n      ))}\n    </ul>\n  );\n}\n';

const range = (text: string, from: string, to: string) => ({
  start: positionOf(text, text.indexOf(from)),
  end: positionOf(text, text.indexOf(to) + to.length),
});

describe("Extract Component (browser mode runs the Rust analysis)", () => {
  it("finds the props of Vue template elements and extracts them", async () => {
    const r = range(VUE, "<StatCard", "</p>");
    const info = await invoke<ExtractComponentInfo>("refactor_extract_component_prepare", { path: "src/views/Stats.vue", ...r, content: VUE });
    expect(info.framework).toBe("vue");
    expect(info.props).toEqual(["store", "label"]);
    expect(info.imports).toEqual(["StatCard"]);
    const answer = await invoke<RefactorAnswer>("refactor_extract_component", {
      path: "src/views/Stats.vue",
      ...r,
      content: VUE,
      name: "Revenue",
      directory: "src/components",
    });
    const created = answer.edit.changes.find((c) => c.kind === "create");
    expect(created?.kind === "create" && created.path).toBe("src/components/Revenue.vue");
    const parent = answer.edit.changes.find((c) => c.kind === "edit" && c.path === "src/views/Stats.vue");
    const texts = parent?.kind === "edit" ? parent.edits.map((e) => e.newText) : [];
    expect(texts).toContain('<Revenue :store="store" :label="label" />');
    expect(texts).toContain("\nimport Revenue from '../components/Revenue.vue'");
  });

  it("offers Extract Component and server refactorings in Refactor This", async () => {
    const r = range(TSX, "<ul>", "</ul>");
    const answer = await invoke<RefactorThisAnswer>("code_refactor_this", {
      path: "src/Cart.tsx",
      line: r.start.line,
      character: r.start.character,
      endLine: r.end.line,
      endCharacter: r.end.character,
      content: TSX,
    });
    expect(answer.actions).toEqual(["RenameElement", "WebForge.ExtractComponent", "Move", "CopyElement"]);
    const expr = range(TSX, "items.map", "i.id}>");
    const other = await invoke<RefactorThisAnswer>("code_refactor_this", {
      path: "src/Cart.tsx",
      line: expr.start.line,
      character: expr.start.character,
      endLine: expr.end.line,
      endCharacter: expr.end.character,
      content: TSX,
    });
    expect(other.actions).not.toContain("WebForge.ExtractComponent");
    expect(other.lsp.map((a) => a.title)).toEqual(["Extract Method (function in module scope)", "Introduce Constant (in enclosing scope)", "Move to New File"]);
  });

  it("validates the dialog", () => {
    expect(validateComponentName("CartList")).toBeNull();
    expect(validateComponentName("cartList")).toMatch(/PascalCase/);
    expect(componentPath("src/components/", "CartList", "tsx")).toBe("src/components/CartList.tsx");
  });
});
