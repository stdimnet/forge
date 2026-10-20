---
name: add-tool-window
description: Add a new tool window (side or bottom panel with a stripe icon) to Forge, or replace one of the placeholder tool windows. Use when building a new panel like Database, Services, Docker, a test runner or any feature UI that lives in a tool window.
---

# Add a tool window

Tool windows are registered from a feature folder through `registerToolWindow`
(`app/ui/src/features/registry.ts`). Placeholders for not-yet-built windows live in
`app/ui/src/features/placeholders/index.tsx`; registering the **same id** replaces a placeholder.

## Steps

1. Create `app/ui/src/features/<name>/` with an `index.ts` and a component, e.g. `ServicesView.tsx`.
2. Register the window:

   ```ts
   import { Container } from "lucide-solid";
   import { registerToolWindow } from "@/features/registry";
   import { ServicesView } from "./ServicesView";

   registerToolWindow({
     id: "services",          // reuse a placeholder id to replace it
     title: "Services",
     icon: Container,          // lucide-solid icon
     anchor: "bottom",         // "left" | "right" | "bottom"
     order: 30,                // position inside the stripe
     shortcut: "Alt+8",        // tooltip hint only; real binding goes through the keymap
     component: ServicesView,
     actions: () => <HeaderButtons />, // optional header buttons
   });
   ```

3. Import the feature once in `app/ui/src/features/index.ts` (`import "./<name>";`).
4. Shortcut: add or alias the PHPStorm action (e.g. `ActivateServicesToolWindow`) – see the
   `add-keymap-action` skill. Never add a `window.addEventListener("keydown", ...)`.
5. Data: call the backend through a typed client in `app/ui/src/ipc/<name>.ts`; add a browser
   mock (`add-ipc-command` skill) so the window works in `pnpm dev`.
6. Styling: a `<name>.css` next to the component using only variables from
   `app/ui/src/theme/theme.css` (both themes). Reuse shared classes: `.tree-row`, `.icon-btn`,
   `.pill-btn`, `.btn-primary`, `.btn-secondary`, `.muted`, `.vcs-*`.
7. Verify with a screenshot scenario (`verify-ui-with-screenshots` skill) and compare against
   PHPStorm's New UI: same layout, wording and behaviour, Forge colors.

## Reference implementations

- Simple tree: `app/ui/src/features/project/`
- Complex panel with toolbar: `app/ui/src/features/git/CommitPanel.tsx`
- Right-side panel: `app/ui/src/features/ai-chat/`
