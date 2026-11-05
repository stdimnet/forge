//! Tint layers: the parts of an icon that the accent color, a folder color or monochrome
//! recolor, so a two-tone folder keeps its shape instead of becoming a silhouette.
//!
//! A theme marks them in its SVGs:
//!
//! | Marker | Recolors |
//! | --- | --- |
//! | `data-forge-tint` or `data-forge-tint="fill"`, `class="forge-tint"` | the fill |
//! | `data-forge-tint="stroke"`, `class="forge-tint-stroke"` | the stroke |
//! | `data-forge-tint="both"` | fill and stroke |
//!
//! A marked group (`<g data-forge-tint>`) recolors everything inside it. Icons without a
//! marker are drawn as a one-color mask, as with format 1 themes.

use regex_lite::Regex;

use crate::is_hex_color;

/// Whether `svg` marks a tint layer.
pub fn has_tint_layer(svg: &str) -> bool {
    svg.contains("data-forge-tint")
        || Regex::new(r#"class\s*=\s*["'][^"']*\bforge-tint(?:-stroke)?\b"#)
            .expect("valid regex")
            .is_match(svg)
}

/// `svg` with its tint layer recolored to `color` (`#rgb` / `#rrggbb`), or `None` when the
/// icon has no tint layer or the color is not a hex color. The color goes into a `<style>`
/// element right after the `<svg …>` start tag, so it overrides the marked elements' own
/// colors.
pub fn tint_svg(svg: &str, color: &str) -> Option<String> {
    if !is_hex_color(color) || !has_tint_layer(svg) {
        return None;
    }
    let start = svg.find("<svg")?;
    let end = start + svg[start..].find('>')?;
    if svg[..end].ends_with('/') {
        return None;
    }
    let c = color;
    let style = format!(
        "<style>\
         [data-forge-tint]:not([data-forge-tint=\"stroke\"]),[data-forge-tint]:not([data-forge-tint=\"stroke\"]) *,\
         .forge-tint,.forge-tint *{{fill:{c}!important}}\
         [data-forge-tint=\"stroke\"],[data-forge-tint=\"stroke\"] *,[data-forge-tint=\"both\"],[data-forge-tint=\"both\"] *,\
         .forge-tint-stroke,.forge-tint-stroke *{{stroke:{c}!important}}\
         </style>"
    );
    let mut out = String::with_capacity(svg.len() + style.len());
    out.push_str(&svg[..=end]);
    out.push_str(&style);
    out.push_str(&svg[end + 1..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check_svg;

    const TWO_TONE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M1 3h5l1 1h7v9H1z" fill="#d9a441" data-forge-tint/><path d="M3 7h10v5H3z" fill="#fff"/></svg>"##;

    #[test]
    fn markers_are_found_and_pass_the_svg_checks() {
        assert!(has_tint_layer(TWO_TONE));
        assert!(has_tint_layer(r#"<svg><path class="a forge-tint"/></svg>"#));
        assert!(has_tint_layer(
            r#"<svg><path class='forge-tint-stroke'/></svg>"#
        ));
        assert!(!has_tint_layer(
            r#"<svg><path class="forge-tinted"/></svg>"#
        ));
        assert!(!has_tint_layer(r#"<svg><path/></svg>"#));
        assert!(check_svg(TWO_TONE).is_ok());
    }

    #[test]
    fn tinting_injects_a_style_after_the_start_tag() {
        let out = tint_svg(TWO_TONE, "#3366ff").unwrap();
        assert!(out
            .starts_with(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><style>"#));
        assert!(out.contains("fill:#3366ff!important"));
        assert!(out.ends_with("</svg>"));
        assert!(
            check_svg(&out).is_ok(),
            "the tinted SVG passes the checks too"
        );
        let xml = format!("<?xml version=\"1.0\"?>\n{TWO_TONE}");
        assert!(tint_svg(&xml, "#fff").unwrap().contains("?>\n<svg"));
    }

    #[test]
    fn nothing_to_tint_or_a_bad_color_gives_none() {
        assert_eq!(tint_svg("<svg><path/></svg>", "#fff"), None);
        assert_eq!(tint_svg(TWO_TONE, "red"), None);
        assert_eq!(tint_svg(TWO_TONE, "#fff;}*{fill:red"), None);
        assert_eq!(tint_svg(r#"<svg data-forge-tint/>"#, "#fff"), None);
    }
}
