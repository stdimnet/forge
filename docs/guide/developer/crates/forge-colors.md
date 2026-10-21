---
title: forge-colors
description: "Color values in code: finding CSS colors (hex, rgb, hsl, hwb, named, Tailwind) in CSS, markup and script files, parsing and formatting them in their notation (pure, WebAssembly-friendly)."
order: 34
tags: [crate, rust]
---

# forge-colors

`crates/forge-colors` — Color values in code: finding CSS colors (hex, rgb, hsl, hwb, named, Tailwind) in CSS, markup and script files, parsing and formatting them in their notation (pure, WebAssembly-friendly).

## Purpose

The logic behind the editor's color previews and the color picker. The UI
(`app/ui/src/features/colors`) draws the swatches and the picker; offsets are UTF-16.

- `color`: `Rgba` (channels 0–255 with fractions, alpha 0–1), `parse` for every CSS notation
  (hex with 3/4/6/8 digits, `rgb()`/`rgba()`, `hsl()`/`hsla()`, `hwb()`, legacy commas and
  modern spaces with `/ alpha`, percentages, `deg`/`rad`/`grad`/`turn`, `none`, names) with
  its `Notation`, and `format`, which writes a color in the notation of the original text
  (short hex while possible, the same function name and separators, `rgb` → `rgba` in legacy
  syntax when an alpha appears, percentages kept, names kept or replaced by hex).
- `named`: the 148 CSS color names plus `transparent`, and Tailwind CSS's default palette
  (22 colors × 11 shades, `black`, `white`).
- `find`: colors of a document by language: declarations in CSS/SCSS/Less (selectors and
  comments skipped, `var()` and `url()` left alone) and Sass/Stylus lines, `<style>` blocks and
  `style` attributes of markup, strings of scripts (whole-string color names, hex and color
  functions), and Tailwind color classes (`bg-blue-500`, `text-white/50`) in class attributes
  and strings (not editable).

## Main public items

```rust
pub fn find(req: &FindRequest) -> Vec<ColorMatch> { … }
pub fn find_colors(text: &str, language: &str) -> Vec<ColorMatch> { … }
pub fn parse(text: &str) -> Option<(Rgba, Notation)> { … }
pub fn format(color: Rgba, original: &str) -> String { … }
pub fn format_color(req: &FormatRequest) -> String { … }
pub fn parse_color(text: &str) -> Option<Parsed> { … }
```

## Used by

- IPC: `colors_find`, `colors_format`, `colors_parse` (`app/src-tauri/src/commands/emmet.rs`).
- UI: `app/ui/src/features/colors` (gutter swatches as line markers, inline swatches, the color
  picker, Show Color Picker). Swatches and the picker call Rust in-process through
  `forge-wasm` in both modes.
- Browser mode: built into `forge-wasm` (`emmet.rs`).

## Dependencies

- Workspace crates: none
- External: `serde`

## Tests

Unit tests for parsing every notation, HSL/HWB conversions, formatting in each notation, the
named colors and the Tailwind palette, finding colors in CSS, SCSS, Sass, HTML, Vue and TSX,
and UTF-16 offsets.
