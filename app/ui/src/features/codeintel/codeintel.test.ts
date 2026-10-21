import { describe, expect, it } from "vitest";
import { CircleArrowDown, CircleArrowUp } from "lucide-solid";
import { commands } from "@/state/commands";
import { findToolWindow } from "@/features/registry";
import type { LineMarkerInfo, UsageTree } from "@/ipc/codeintel";
import { libraryCrumbs, noteLibrarySymbol } from "./library";
import { markerChooserTitle, toLineMarker } from "./markers";
import { toDeclarationTargets } from "./targets";
import { countLabel, usageItems } from "./usages";
import "./index";

const target = (name: string) => ({ path: "app/Services/Payments/StripeGateway.php", line: 13, column: 20, name, container: "App\\Services\\Payments" });

describe("code navigation wiring", () => {
  it("implements PHPStorm's navigation actions, so the main menu enables them", () => {
    for (const id of ["GotoImplementation", "GotoTypeDeclaration", "GotoSuperMethod", "FindUsages", "ShowUsages"]) {
      expect(commands().has(id), id).toBe(true);
    }
    // Alt+3 (ActivateFindToolWindow) finds the Find tool window.
    expect(findToolWindow("find")?.shortcut).toBe("Alt+3");
  });

  it("turns gutter markers into icons with PHPStorm's chooser titles", () => {
    const down: LineMarkerInfo = {
      line: 9,
      kind: "implemented",
      title: "Is implemented in 2 classes",
      targets: [target("FakeGateway::charge()"), target("StripeGateway::charge()")],
    };
    const m = toLineMarker(down);
    expect(m.icon).toBe(CircleArrowDown);
    expect(m.className).toContain("marker-impl");
    expect(markerChooserTitle(down)).toBe("Choose Implementation of charge (2 found)");

    const up: LineMarkerInfo = { line: 13, kind: "overrides", title: "Overrides method in Model", targets: [target("Model::save()")] };
    expect(toLineMarker(up).icon).toBe(CircleArrowUp);
    expect(markerChooserTitle(up)).toBe("Choose Super Method of save (1 found)");
    expect(markerChooserTitle({ ...down, kind: "subclassed", targets: [target("OrderController")] })).toBe("Choose Subclass (1 found)");
  });

  it("names library files like PHPStorm's PHP Runtime", () => {
    const path = "library://php-stubs/Core/Core_c.php";
    expect(libraryCrumbs(path)).toEqual(["PHP Runtime", "Core", "Core_c.php"]);
    noteLibrarySymbol(path, "Throwable");
    expect(libraryCrumbs(path)).toEqual(["PHP Runtime", "Core", "Core_c.php", "Throwable"]);
    noteLibrarySymbol(path, "Exception::getMessage()");
    expect(libraryCrumbs(path).at(-1)).toBe("Exception::getMessage");
    expect(libraryCrumbs("library://file//opt/intelephense/lib/stub/Core/Core_c.php")).toEqual(["External Libraries", "Core", "Core_c.php"]);
  });

  it("keeps symbol names for the Choose Declaration list", () => {
    expect(toDeclarationTargets([target("StripeGateway")])[0]).toMatchObject({ name: "StripeGateway", container: "App\\Services\\Payments", line: 13 });
    expect(toDeclarationTargets([{ ...target(""), container: null }])[0].name).toBeUndefined();
  });

  it("lists usages in tree order", () => {
    const row = (line: number) => ({ line, column: 4, endLine: line, endColumn: 9, text: "new Order()", ranges: [[4, 9]] as [number, number][], kind: "newInstance" as const });
    const tree: UsageTree = {
      total: 3,
      fileCount: 2,
      groups: [
        { kind: "import", label: "Usage in import", count: 1, files: [{ path: "a.php", usages: [row(2)] }] },
        { kind: "newInstance", label: "New instance creation", count: 2, files: [{ path: "a.php", usages: [row(8)] }, { path: "b.php", usages: [row(1)] }] },
      ],
    };
    expect(usageItems(tree).map((u) => `${u.path}:${u.row.line}`)).toEqual(["a.php:2", "a.php:8", "b.php:1"]);
    expect(countLabel(1)).toBe("1 usage");
    expect(countLabel(3, "file")).toBe("3 files");
  });
});
