import { describe, expect, it, vi } from "vitest";
import type { EditorView } from "@codemirror/view";
import type { ContextAction, CreateClassData } from "@/ipc/codeintel";
import { createFromUsage } from "@/mock/createFromUsageMock";
import { actionIcon, registerContextActionHandler, runContextAction } from "./intentions";

const CONTROLLER = `<?php

namespace App\\Http\\Controllers;

use App\\Models\\Order;

final class OrderController extends Controller
{
    public function show(Order $order): void
    {
        $this->notifyCustomer($order, 'paid');
        $receipt = new Receipt($order);
    }
}
`;

describe("create from usage", () => {
  it("offers Create method with parameters and a caret in the body", () => {
    const [create] = createFromUsage("app/Http/Controllers/OrderController.php", CONTROLLER, CONTROLLER.indexOf("notifyCustomer") + 2);
    expect(create.title).toBe("Create method 'notifyCustomer'");
    expect(create.kind).toBe("quickFix");
    expect(actionIcon(create)).toBe("fix");
    const change = create.edit!.changes[0];
    expect(change.kind === "edit" && change.edits[0].newText).toContain(
      "private function notifyCustomer(Order $order, string $string): void\n    {\n        // TODO: Implement notifyCustomer() method.\n    }",
    );
    expect(create.caret?.range.start).toEqual({ line: 16, character: 8 });
  });

  it("opens the class dialog through the registered handler", async () => {
    const [create] = createFromUsage("app/Http/Controllers/OrderController.php", CONTROLLER, CONTROLLER.indexOf("Receipt") + 1);
    expect(create.title).toBe("Create class 'Receipt'");
    // An action with arguments is a fix, not a link like "Generate...".
    expect(actionIcon(create)).toBe("fix");
    const data = create.data as CreateClassData;
    expect(data).toMatchObject({ name: "Receipt", namespace: "App\\Http\\Controllers", template: "PHP Class", import: true });

    const run = vi.fn();
    registerContextActionHandler("PhpCreateClass", run);
    await runContextAction("app/Http/Controllers/OrderController.php", {} as EditorView, create as ContextAction);
    expect(run).toHaveBeenCalledWith(data, expect.objectContaining({ path: "app/Http/Controllers/OrderController.php" }));

    // Existing methods and classes get nothing.
    expect(createFromUsage("x.php", CONTROLLER, CONTROLLER.indexOf("show") + 1)).toEqual([]);
    expect(createFromUsage("x.php", CONTROLLER, CONTROLLER.indexOf("Order $order") + 1)).toEqual([]);
  });
});
