/**
 * Settings | Editor | Color Scheme: the page and the scheme's CSS variables on the document
 * (`store.ts`). Logic, data and .icls import/export live in `crates/forge-colorscheme`.
 */
import { registerSettingsPage } from "@/features/settingsPages";
import { ColorSchemePage } from "./ColorSchemePage";
import { installColorScheme } from "./store";
import "./colorScheme.css";

registerSettingsPage({
  id: "editor.color-scheme",
  title: "Color Scheme",
  group: "Editor",
  order: 15,
  keywords: ["colors", "theme", "syntax", "highlighting", "icls", "import scheme", "export scheme", "bold", "italic", "effect"],
  component: ColorSchemePage,
});

installColorScheme();
