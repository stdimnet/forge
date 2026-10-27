/**
 * Small tile identifying a language or framework (Welcome screen badges, New Project list,
 * status bar). Forge draws its own glyphs: a lucide icon where one fits the project's logo,
 * otherwise a short monogram, tinted with a theme color.
 */
import type { Component } from "solid-js";
import { Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Atom, Code, Folder, Heart, Leaf, Moon, Mountain, Package, Zap } from "lucide-solid";
import "./projectTypeIcon.css";

type Glyph = { icon?: Component<{ size?: number; "stroke-width"?: number }>; text?: string };

const GLYPHS: Record<string, Glyph> = {
  php: { text: "php" },
  laravel: { text: "L" },
  symfony: { text: "sf" },
  composer: { icon: Package },
  wordpress: { text: "W" },
  drupal: { text: "Dr" },
  joomla: { text: "J" },
  shopware: { text: "Sw" },
  magento: { text: "M" },
  html: { icon: Code },
  css: { text: "#" },
  javascript: { text: "JS" },
  typescript: { text: "TS" },
  vite: { icon: Zap },
  react: { icon: Atom },
  vue: { text: "V" },
  angular: { text: "A" },
  svelte: { text: "S" },
  solid: { text: "So" },
  next: { text: "N" },
  nuxt: { icon: Mountain },
  lua: { icon: Moon },
  love: { icon: Heart },
  neovim: { text: "nv" },
  rust: { text: "Rs" },
  go: { text: "Go" },
  python: { text: "Py" },
  "spring-boot": { icon: Leaf },
  "jakarta-ee": { text: "EE" },
  django: { text: "dj" },
  flask: { text: "Fl" },
  fastapi: { icon: Zap },
  celery: { text: "Ce" },
  folder: { icon: Folder },
};

export const hasProjectTypeIcon = (type: string) => type in GLYPHS;

export function ProjectTypeIcon(props: { type: string; size?: number; title?: string }) {
  const glyph = () => GLYPHS[props.type] ?? { text: props.type.slice(0, 2) };
  const size = () => props.size ?? 20;
  // Short monograms get bigger letters than three-letter ones.
  const fontSize = () => Math.round(size() * ([0.62, 0.62, 0.5, 0.4][glyph().text?.length ?? 2] ?? 0.4));
  return (
    <span
      class="pt-icon"
      data-type={props.type}
      title={props.title}
      style={{ width: `${size()}px`, height: `${size()}px`, "font-size": `${fontSize()}px` }}
    >
      <Show when={glyph().icon} fallback={<span class="pt-icon-text">{glyph().text}</span>}>
        <Dynamic component={glyph().icon} size={Math.round(size() * 0.62)} stroke-width={2.2} />
      </Show>
    </span>
  );
}
