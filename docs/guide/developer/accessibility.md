---
title: Writing accessible UI
description: Rules for accessible Forge UI - ARIA roles and names, trees and lists, dialogs and focus, keyboard operation through the keymap, focus rings, theme contrast, live announcements, and the checks (contrast test, axe audit, keyboard-only flow).
order: 106
tags: [accessibility, a11y, aria, keyboard, focus, contrast, axe, screen reader]
---

# Writing accessible UI

Every panel, dialog and popup must work with a screen reader and with the keyboard alone, and
must stay readable in all themes. The shared pieces live in
`app/ui/src/features/accessibility/`; this page lists the rules and the checks. The user side
is described in [Accessibility](../user/accessibility.md).

## Rules

1. **Use the element that means what it does.** A `<button>` for actions, `<input>` / `<select>`
   for values, `<a>` for links. A clickable `<div>` or `<span>` is not reachable with Tab.
2. **Every control has a name.** Icon buttons get `title` and `aria-label` (translated, see
   [Localization](localization.md)); inputs get a `<label for>` (or are inside their label) and
   `aria-describedby` for their description, as `SchemaPage` does. Decorative icons and
   initials badges get `aria-hidden="true"`.
3. **Trees and lists use their roles.** A focusable container with `role="tree"` /
   `role="listbox"` and rows with `role="treeitem"` / `role="option"`, `aria-selected`,
   `aria-level` and `aria-expanded` (trees). Either the container keeps focus and points at the
   active row with `aria-activedescendant` (Project, Structure, Commit, Search Everywhere), or
   the rows use a roving `tabIndex` (0 on the selected row, -1 on the others: Settings tree,
   editor tabs). Arrow keys, `Home`, `End` and `Enter` come from
   `features/project/treeNavigation.ts` (`navigate`), so every tree behaves the same.
4. **Dialogs are `role="dialog"` with a name.** Inside a backdrop (`*-backdrop`) they are
   modal. `features/accessibility/dialogs.ts` finds every dialog in the window: it names it
   after its title, sets `aria-modal`, connects `<label>` elements to the control after them,
   keeps Tab inside the topmost modal dialog and returns focus to the opener when it closes.
   Focus the first field when the dialog opens, and close it with `Escape`.
5. **Shortcuts only through the keymap** ([Keymap actions](keymap-actions.md)). Keys inside a
   widget (arrows in a tree, `Enter` in a list, `Escape` in a dialog) belong to the widget's
   own `onKeyDown`; anything that is a command gets an action id. Never trap Tab: a widget may
   take Tab only when `Escape` leaves it (Search Everywhere switches tabs with Tab).
6. **Focus is visible.** Do not remove outlines from focusable controls; the global
   `:focus-visible` ring in `accessibility.css` draws one (`--focus-ring`,
   `--focus-ring-width`). A container that keeps focus with `aria-activedescendant` shows the
   ring on its selected row instead.
7. **Colors come from the theme tokens.** Text uses `--fg`, `--fg-strong`, `--fg-muted` or
   `--fg-faint` on the `--bg-*` tokens; status colors use `--danger`, `--warning`, `--success`,
   `--info` and `--vcs-*`. Never write a color literal: the High Contrast themes replace the
   tokens. Do not show a state by color alone: add an icon, a text, a border or an underline
   (the High Contrast themes add outlines and underlines in `accessibility.css`).
8. **Announce what is not visible to a screen reader.** `announce(text)` from
   `@/features/accessibility` speaks through a live region that exists from startup;
   `announceForScreenReader(text)` only with **Support screen readers** on. Balloons are
   announced already (`.balloons` is a live region; errors are `role="alert"`).
9. **Name the areas.** Landmarks (`header`, `nav`, `main`, `footer`, `section`) need a unique
   `aria-label`. A new tool window gets its name from `registerToolWindow`'s title.

## The building blocks

| Module | What it does |
| --- | --- |
| `features/accessibility/announce.ts` | live regions, `announce`, `announceForScreenReader`, `screenReaderMode` |
| `features/accessibility/dialogs.ts` | dialog names, `aria-modal`, label association, focus trap and return, `tabbables()` |
| `features/accessibility/areas.ts` | **Focus Next / Previous Area**, **Focus Editor**, `focusToolWindowContent` (used by `Alt+<n>`) |
| `features/accessibility/presentation.ts` | the Presentation Assistant, fed by `onCommandInvoked` in `state/commands.ts` |
| `features/accessibility/ide.ts` | the editor's accessible name, **Announce Caret Position**, problem announcements |
| `features/accessibility/accessibility.css` | `.sr-only`, focus rings, High Contrast extras, contrast scrollbars |
| `theme/contrast.ts` | WCAG contrast of the theme tokens |

`runCommand(id, actionId)` notifies `onCommandInvoked` listeners before running a command;
`runAction` passes the PHPStorm action id, so the Presentation Assistant shows the action's
title and shortcuts (and the other PHPStorm keymap's shortcut from the keymap snapshot's
`alternate` field, computed in `forge-keymap`).

## Themes and contrast

Theme tokens live in `app/ui/src/theme/theme.css`. `src/theme/contrast.test.ts` (part of
`pnpm test`) computes WCAG contrast ratios for the token pairs in `theme/contrast.ts`
(`themePairs`): text tokens on every background, syntax colors on the editor, secondary text,
status and VCS colors, also on the selected row, and the accent button. The normal themes must
reach AA (4.5:1 for text), the `high-contrast*` themes AAA (7:1) plus visible borders. Only the
Solarized palette has documented exceptions. A new token that text is drawn with belongs in
`themePairs`.

A new theme needs: a block in `theme.css`, its id in `THEMES` (`state/theme.ts`), the option in
`appearance.theme` (`app/src-tauri/plugins/core.toml`), and, for an editor scheme of its own, a
bundled scheme in `forge-colorscheme` (`Bundled`).

## Checks

| Check | Command | Needs |
| --- | --- | --- |
| Theme contrast | `pnpm test src/theme` | nothing (part of `pnpm test`) |
| Dialog focus, live regions, focus areas, Presentation Assistant, Settings dialog with the keyboard | `pnpm test src/features/accessibility` | nothing (part of `pnpm test`) |
| axe-core audit of the main screens and dialogs in all themes you pass | `pnpm a11y:audit --themes=dark,light,high-contrast,high-contrast-light` | `pnpm dev` running |
| Keyboard-only flow: open a file, rename, focus areas, Switcher, commit, Settings | `pnpm a11y:keyboard` | `pnpm dev` running |

`pnpm a11y:audit` runs [axe-core](https://github.com/dequelabs/axe-core) (MPL-2.0, a
development dependency only, never bundled) in Chromium and exits with 1 on a serious or
critical violation (`--report` only prints, `--json=file` writes the results). Add a screen to
`SCREENS` in `scripts/a11y-audit.mjs` when you add a major dialog or tool window. The keyboard
flow fails on any mouse event; extend it when a feature adds a keyboard path users depend on.

When the audit was introduced, axe reported 98 critical and 232 serious violations (counted
per element, summed over the audited screens) in the dark theme and 98 critical and 260 serious
in the light theme: unnamed editor, selects and checkboxes, nested interactive editor tabs, a
tree without tree items, text below 4.5:1 and scroll areas without keyboard access. All
serious and critical findings were fixed.
