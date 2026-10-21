//! CSS abbreviations: `m10` → `margin: 10px;`, `p10-20` → `padding: 10px 20px;`,
//! `w100p` → `width: 100%;`, `df` → `display: flex;`, `pos:a` → `position: absolute;`,
//! `bgc#fff` → `background-color: #fff;`, `bd1-s#f00`, `m-10`, `lh1.5`, `c#3!`, at-rules
//! (`@m`, `@kf`, `@f`) and several properties at once (`m10+p5`).

use crate::markup::{Output, Stop};
use crate::text::utf16_of;

#[derive(Debug, Clone)]
pub struct CssOptions {
    pub indent: String,
    /// `false` in indented syntaxes (Sass, Stylus): no `;` after a declaration.
    pub semicolons: bool,
}

impl Default for CssOptions {
    fn default() -> Self {
        Self {
            indent: "\t".into(),
            semicolons: true,
        }
    }
}

/// `(abbreviation, property, value)`: the value is a template with `${n:default}` stops, a
/// keyword list `a|b|c` (the first is the default) or empty.
const PROPS: &[(&str, &str, &str)] = &[
    ("ac", "align-content", "start|end|flex-start|flex-end|center|space-between|space-around|stretch|space-evenly"),
    ("ai", "align-items", "start|end|flex-start|flex-end|center|baseline|stretch"),
    ("as", "align-self", "start|end|auto|flex-start|flex-end|center|baseline|stretch"),
    ("anim", "animation", ""),
    ("animdel", "animation-delay", ""),
    ("animdir", "animation-direction", "normal|alternate|alternate-reverse|reverse"),
    ("animdur", "animation-duration", ""),
    ("animfm", "animation-fill-mode", "both|forwards|backwards|none"),
    ("animic", "animation-iteration-count", "1|infinite"),
    ("animn", "animation-name", ""),
    ("animps", "animation-play-state", "running|paused"),
    ("animtf", "animation-timing-function", "linear|ease|ease-in|ease-out|ease-in-out"),
    ("ap", "appearance", "none"),
    ("ar", "aspect-ratio", ""),
    ("b", "bottom", ""),
    ("bd", "border", "${1:1px} ${2:solid} ${3:#000}"),
    ("bdb", "border-bottom", "${1:1px} ${2:solid} ${3:#000}"),
    ("bdl", "border-left", "${1:1px} ${2:solid} ${3:#000}"),
    ("bdr", "border-right", "${1:1px} ${2:solid} ${3:#000}"),
    ("bdt", "border-top", "${1:1px} ${2:solid} ${3:#000}"),
    ("bdc", "border-color", "${1:#000}"),
    ("bdcl", "border-collapse", "collapse|separate"),
    ("bdrs", "border-radius", ""),
    ("bds", "border-style", "none|hidden|dotted|dashed|solid|double|groove|ridge|inset|outset"),
    ("bdsp", "border-spacing", ""),
    ("bdw", "border-width", ""),
    ("bg", "background", "${1:#000}"),
    ("bga", "background-attachment", "fixed|scroll"),
    ("bgbk", "background-break", "bounding-box|each-box|continuous"),
    ("bgc", "background-color", "${1:#fff}"),
    ("bgcp", "background-clip", "padding-box|border-box|content-box|no-clip"),
    ("bgi", "background-image", "url(${1})"),
    ("bgo", "background-origin", "padding-box|border-box|content-box"),
    ("bgp", "background-position", "${1:0} ${2:0}"),
    ("bgpx", "background-position-x", ""),
    ("bgpy", "background-position-y", ""),
    ("bgr", "background-repeat", "no-repeat|repeat-x|repeat-y|space|round"),
    ("bgsz", "background-size", "contain|cover"),
    ("bgs", "background-size", "contain|cover"),
    ("bxsh", "box-shadow", "${1:0} ${2:0} ${3:0} ${4:#000}"),
    ("bxz", "box-sizing", "border-box|content-box"),
    ("c", "color", "${1:#000}"),
    ("cl", "clear", "both|left|right|none"),
    ("cm", "comment", ""),
    ("cnt", "content", "'${1}'"),
    ("coi", "counter-increment", ""),
    ("colm", "columns", ""),
    ("colmc", "column-count", ""),
    ("colmg", "column-gap", ""),
    ("cor", "counter-reset", ""),
    ("cp", "clip-path", ""),
    ("cps", "caption-side", "top|bottom"),
    ("cur", "cursor", "pointer|auto|default|crosshair|hand|help|move|text|not-allowed|grab"),
    ("d", "display", "block|none|flex|inline-flex|inline|inline-block|grid|inline-grid|subgrid|list-item|contents|table|inline-table|table-caption|table-column|table-column-group|table-header-group|table-footer-group|table-row|table-row-group|table-cell"),
    ("ec", "empty-cells", "show|hide"),
    ("f", "font", ""),
    ("fef", "font-effect", "none|engrave|emboss|outline"),
    ("ff", "font-family", "serif|sans-serif|cursive|fantasy|monospace"),
    ("fl", "float", "left|right|none"),
    ("fs", "font-style", "italic|normal|oblique"),
    ("fst", "font-stretch", "normal|ultra-condensed|extra-condensed|condensed|semi-condensed|semi-expanded|expanded|extra-expanded|ultra-expanded"),
    ("fv", "font-variant", "normal|small-caps"),
    ("fw", "font-weight", "normal|bold|bolder|lighter"),
    ("fx", "flex", ""),
    ("fxb", "flex-basis", "fill|max-content|min-content|fit-content|content"),
    ("fxd", "flex-direction", "row|row-reverse|column|column-reverse"),
    ("fxf", "flex-flow", ""),
    ("fxg", "flex-grow", ""),
    ("fxsh", "flex-shrink", ""),
    ("fxw", "flex-wrap", "nowrap|wrap|wrap-reverse"),
    ("fz", "font-size", ""),
    ("fza", "font-size-adjust", ""),
    ("g", "gap", ""),
    ("ga", "grid-area", ""),
    ("gac", "grid-auto-columns", ""),
    ("gaf", "grid-auto-flow", "row|column|dense"),
    ("gar", "grid-auto-rows", ""),
    ("gc", "grid-column", ""),
    ("gce", "grid-column-end", ""),
    ("gcg", "grid-column-gap", ""),
    ("gcs", "grid-column-start", ""),
    ("gg", "grid-gap", ""),
    ("gr", "grid-row", ""),
    ("gre", "grid-row-end", ""),
    ("grg", "grid-row-gap", ""),
    ("grs", "grid-row-start", ""),
    ("gt", "grid-template", ""),
    ("gta", "grid-template-areas", ""),
    ("gtc", "grid-template-columns", "repeat(${1:2}, ${2:1fr})"),
    ("gtr", "grid-template-rows", "repeat(${1:2}, ${2:1fr})"),
    ("h", "height", ""),
    ("ins", "inset", ""),
    ("jc", "justify-content", "start|end|stretch|flex-start|flex-end|center|space-between|space-around|space-evenly"),
    ("ji", "justify-items", "start|end|center|stretch"),
    ("js", "justify-self", "start|end|center|stretch"),
    ("l", "left", ""),
    ("lg", "background-image", "linear-gradient(${1})"),
    ("lh", "line-height", ""),
    ("lis", "list-style", ""),
    ("lisi", "list-style-image", ""),
    ("lisp", "list-style-position", "outside|inside"),
    ("list", "list-style-type", "disc|circle|square|decimal|decimal-leading-zero|lower-roman|upper-roman"),
    ("lts", "letter-spacing", "normal"),
    ("m", "margin", ""),
    ("mb", "margin-bottom", ""),
    ("mah", "max-height", ""),
    ("maw", "max-width", ""),
    ("mih", "min-height", ""),
    ("miw", "min-width", ""),
    ("ml", "margin-left", ""),
    ("mr", "margin-right", ""),
    ("mt", "margin-top", ""),
    ("o", "outline", ""),
    ("oc", "outline-color", "${1:#000}"),
    ("of", "object-fit", "fill|contain|cover|none|scale-down"),
    ("oo", "outline-offset", ""),
    ("op", "opacity", ""),
    ("ord", "order", ""),
    ("os", "outline-style", "none|dotted|dashed|solid|double|groove|ridge|inset|outset"),
    ("ov", "overflow", "hidden|visible|scroll|auto"),
    ("ovs", "overflow-style", "scrollbar|auto|panner|move|marquee"),
    ("ovx", "overflow-x", "hidden|visible|scroll|auto"),
    ("ovy", "overflow-y", "hidden|visible|scroll|auto"),
    ("ow", "outline-width", ""),
    ("op", "opacity", ""),
    ("p", "padding", ""),
    ("pb", "padding-bottom", ""),
    ("pe", "pointer-events", "auto|none"),
    ("pl", "padding-left", ""),
    ("pos", "position", "relative|absolute|fixed|static|sticky"),
    ("pr", "padding-right", ""),
    ("pt", "padding-top", ""),
    ("q", "quotes", ""),
    ("r", "right", ""),
    ("rsz", "resize", "none|both|horizontal|vertical"),
    ("t", "top", ""),
    ("ta", "text-align", "left|center|right|justify"),
    ("tal", "text-align-last", "auto|left|center|right"),
    ("tbl", "table-layout", "auto|fixed"),
    ("td", "text-decoration", "none|underline|overline|line-through"),
    ("te", "text-emphasis", ""),
    ("th", "text-height", "auto|font-size|text-size|max-size"),
    ("ti", "text-indent", ""),
    ("tj", "text-justify", "auto|inter-word|inter-ideograph|inter-cluster|distribute|kashida|tibetan"),
    ("to", "text-outline", ""),
    ("tov", "text-overflow", "ellipsis|clip"),
    ("tr", "text-replace", ""),
    ("trf", "transform", ""),
    ("trfo", "transform-origin", ""),
    ("trfs", "transform-style", "preserve-3d|flat"),
    ("trs", "transition", "${1:prop} ${2:time}"),
    ("trsde", "transition-delay", ""),
    ("trsdu", "transition-duration", ""),
    ("trsp", "transition-property", ""),
    ("trstf", "transition-timing-function", "ease|linear|ease-in|ease-out|ease-in-out"),
    ("tsh", "text-shadow", "${1:0} ${2:0} ${3:0} ${4:#000}"),
    ("tt", "text-transform", "uppercase|lowercase|capitalize|none"),
    ("tw", "text-wrap", "none|normal|unrestricted|suppress"),
    ("us", "user-select", "none|auto|text|all"),
    ("v", "visibility", "hidden|visible|collapse"),
    ("va", "vertical-align", "top|super|text-top|middle|baseline|bottom|text-bottom|sub"),
    ("w", "width", ""),
    ("whs", "white-space", "nowrap|pre|pre-wrap|pre-line|normal"),
    ("whsc", "white-space-collapse", "normal|keep-all|loose|break-strict|break-all"),
    ("wid", "widows", ""),
    ("wm", "writing-mode", "lr-tb|lr-tb|lr-bt|rl-tb|rl-bt|tb-rl|tb-lr|bt-lr|bt-rl"),
    ("wob", "word-break", "normal|keep-all|break-all"),
    ("wow", "word-wrap", "normal|break-word"),
    ("ws", "word-spacing", ""),
    ("z", "z-index", ""),
    ("zm", "zoom", ""),
];

/// At-rules and other snippets that are not a single declaration (`\t` is one indent).
const RULES: &[(&str, &str)] = &[
    (
        "@f",
        "@font-face {\n\tfont-family: ${1};\n\tsrc: url(${2});\n}",
    ),
    (
        "@font-face",
        "@font-face {\n\tfont-family: ${1};\n\tsrc: url(${2});\n}",
    ),
    ("@i", "@import url(${1});"),
    ("@import", "@import url(${1});"),
    ("@kf", "@keyframes ${1:identifier} {\n\t${2}\n}"),
    ("@keyframes", "@keyframes ${1:identifier} {\n\t${2}\n}"),
    ("@m", "@media ${1:screen} {\n\t${2}\n}"),
    ("@media", "@media ${1:screen} {\n\t${2}\n}"),
    ("@cs", "@charset \"${1:UTF-8}\";"),
    ("@sup", "@supports ${1:(prop: value)} {\n\t${2}\n}"),
    ("@ct", "@container ${1:(min-width: 400px)} {\n\t${2}\n}"),
];

/// Property names for exact names (`color`) and fuzzy matches (`bxsz`).
const PROPERTY_NAMES: &[&str] = &[
    "accent-color",
    "align-content",
    "align-items",
    "align-self",
    "animation",
    "appearance",
    "aspect-ratio",
    "backdrop-filter",
    "backface-visibility",
    "background",
    "background-attachment",
    "background-blend-mode",
    "background-clip",
    "background-color",
    "background-image",
    "background-origin",
    "background-position",
    "background-repeat",
    "background-size",
    "block-size",
    "border",
    "border-block",
    "border-bottom",
    "border-bottom-left-radius",
    "border-bottom-right-radius",
    "border-collapse",
    "border-color",
    "border-image",
    "border-inline",
    "border-left",
    "border-radius",
    "border-right",
    "border-spacing",
    "border-style",
    "border-top",
    "border-top-left-radius",
    "border-top-right-radius",
    "border-width",
    "bottom",
    "box-shadow",
    "box-sizing",
    "break-after",
    "break-before",
    "break-inside",
    "caption-side",
    "caret-color",
    "clear",
    "clip",
    "clip-path",
    "color",
    "color-scheme",
    "column-count",
    "column-gap",
    "column-rule",
    "column-span",
    "column-width",
    "columns",
    "contain",
    "container",
    "content",
    "counter-increment",
    "counter-reset",
    "cursor",
    "direction",
    "display",
    "empty-cells",
    "fill",
    "filter",
    "flex",
    "flex-basis",
    "flex-direction",
    "flex-flow",
    "flex-grow",
    "flex-shrink",
    "flex-wrap",
    "float",
    "font",
    "font-family",
    "font-feature-settings",
    "font-size",
    "font-style",
    "font-variant",
    "font-weight",
    "gap",
    "grid",
    "grid-area",
    "grid-auto-columns",
    "grid-auto-flow",
    "grid-auto-rows",
    "grid-column",
    "grid-row",
    "grid-template",
    "grid-template-areas",
    "grid-template-columns",
    "grid-template-rows",
    "height",
    "hyphens",
    "image-rendering",
    "inline-size",
    "inset",
    "isolation",
    "justify-content",
    "justify-items",
    "justify-self",
    "left",
    "letter-spacing",
    "line-clamp",
    "line-height",
    "list-style",
    "list-style-position",
    "list-style-type",
    "margin",
    "margin-block",
    "margin-bottom",
    "margin-inline",
    "margin-left",
    "margin-right",
    "margin-top",
    "mask",
    "max-height",
    "max-width",
    "min-height",
    "min-width",
    "mix-blend-mode",
    "object-fit",
    "object-position",
    "opacity",
    "order",
    "outline",
    "outline-color",
    "outline-offset",
    "outline-style",
    "outline-width",
    "overflow",
    "overflow-wrap",
    "overflow-x",
    "overflow-y",
    "overscroll-behavior",
    "padding",
    "padding-block",
    "padding-bottom",
    "padding-inline",
    "padding-left",
    "padding-right",
    "padding-top",
    "perspective",
    "place-content",
    "place-items",
    "place-self",
    "pointer-events",
    "position",
    "quotes",
    "resize",
    "right",
    "rotate",
    "row-gap",
    "scale",
    "scroll-behavior",
    "scroll-margin",
    "scroll-padding",
    "scroll-snap-align",
    "scroll-snap-type",
    "stroke",
    "stroke-width",
    "tab-size",
    "table-layout",
    "text-align",
    "text-decoration",
    "text-decoration-color",
    "text-indent",
    "text-overflow",
    "text-shadow",
    "text-transform",
    "text-underline-offset",
    "top",
    "touch-action",
    "transform",
    "transform-origin",
    "transition",
    "transition-delay",
    "transition-duration",
    "transition-property",
    "transition-timing-function",
    "translate",
    "user-select",
    "vertical-align",
    "visibility",
    "white-space",
    "width",
    "will-change",
    "word-break",
    "word-spacing",
    "word-wrap",
    "writing-mode",
    "z-index",
    "zoom",
];

/// Properties whose numbers have no unit.
const UNITLESS: &[&str] = &[
    "z-index",
    "line-height",
    "opacity",
    "font-weight",
    "zoom",
    "order",
    "flex",
    "flex-grow",
    "flex-shrink",
    "orphans",
    "widows",
    "counter-increment",
    "counter-reset",
    "column-count",
    "animation-iteration-count",
    "tab-size",
    "fill-opacity",
    "stroke-opacity",
    "grid-row",
    "grid-column",
    "grid-row-start",
    "grid-row-end",
    "grid-column-start",
    "grid-column-end",
    "grid-area",
    "aspect-ratio",
    "scale",
];

/// Keywords any property understands, by abbreviation.
const GLOBAL_KEYWORDS: &[(&str, &str)] = &[
    ("a", "auto"),
    ("i", "inherit"),
    ("n", "none"),
    ("s", "solid"),
    ("da", "dashed"),
    ("do", "dotted"),
    ("t", "transparent"),
    ("h", "hidden"),
    ("u", "unset"),
    ("in", "initial"),
];

/// Expand a CSS abbreviation; unknown properties are an error, so Tab keeps indenting.
pub fn expand(abbr: &str, opts: &CssOptions) -> Result<Output, String> {
    let parts = split_plus(abbr);
    let mut out = String::new();
    let mut stops: Vec<(usize, usize, usize)> = Vec::new();
    let mut base = 0;
    for (k, part) in parts.iter().enumerate() {
        if k > 0 {
            out.push('\n');
        }
        let max = render(part, opts, &mut out, &mut stops, base)?;
        base = max;
    }
    let stops = stops
        .into_iter()
        .map(|(from, to, index)| Stop {
            from: utf16_of(&out, from),
            to: utf16_of(&out, to),
            index,
        })
        .collect();
    Ok(Output { text: out, stops })
}

/// `m10+p5` → `["m10", "p5"]`; a `+` that does not start a new abbreviation stays.
fn split_plus(abbr: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let bytes = abbr.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'+'
            && i > start
            && bytes
                .get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'@')
        {
            parts.push(&abbr[start..i]);
            start = i + 1;
        }
    }
    parts.push(&abbr[start..]);
    parts
}

/// One declaration or rule; returns the highest stop index used.
fn render(
    abbr: &str,
    opts: &CssOptions,
    out: &mut String,
    stops: &mut Vec<(usize, usize, usize)>,
    base: usize,
) -> Result<usize, String> {
    let (abbr, important) = match abbr.strip_suffix('!') {
        Some(a) => (a, true),
        None => (abbr, false),
    };
    if abbr.is_empty() {
        return Err("empty abbreviation".into());
    }
    if abbr.starts_with('@') {
        let (_, template) = RULES
            .iter()
            .find(|(k, _)| *k == abbr)
            .ok_or_else(|| format!("unknown at-rule {abbr}"))?;
        let template = template.replace('\t', &opts.indent);
        let template = if opts.semicolons {
            template
        } else {
            template.replace(";\n", "\n").replace(';', "")
        };
        return Ok(push_template(out, stops, &template, base));
    }
    let (property, defaults, value) = resolve(abbr)?;
    out.push_str(property);
    out.push_str(": ");
    let mut max = base;
    match value {
        Some(v) => out.push_str(&v),
        None if defaults.contains("${") => max = push_template(out, stops, defaults, base),
        None => {
            let first = defaults.split('|').next().unwrap_or("");
            let from = out.len();
            out.push_str(first);
            max = base + 1;
            stops.push((from, out.len(), max));
        }
    }
    if important {
        out.push_str(" !important");
    }
    if opts.semicolons {
        out.push(';');
    }
    Ok(max)
}

/// Write a template with `${n}` / `${n:default}` stops (numbered after `base`).
fn push_template(
    out: &mut String,
    stops: &mut Vec<(usize, usize, usize)>,
    template: &str,
    base: usize,
) -> usize {
    let mut max = base;
    let mut rest = template;
    while let Some(i) = rest.find("${") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        let n: usize = after[..digits].parse().unwrap_or(1);
        let tail = &after[digits..];
        let (default, next) = match tail.strip_prefix(':') {
            Some(d) => {
                let end = d.find('}').unwrap_or(d.len());
                (&d[..end], d.get(end + 1..).unwrap_or(""))
            }
            None => ("", tail.strip_prefix('}').unwrap_or(tail)),
        };
        let from = out.len();
        out.push_str(default);
        stops.push((from, out.len(), base + n));
        max = max.max(base + n);
        rest = next;
    }
    out.push_str(rest);
    max
}

/// `(property, default value, value)` of one abbreviation.
fn resolve(abbr: &str) -> Result<(&'static str, &'static str, Option<String>), String> {
    if let Some((p, v)) = abbr.split_once(':') {
        let (property, defaults) = property_of(p).ok_or_else(|| format!("unknown property {p}"))?;
        let value = if v.is_empty() {
            None
        } else {
            Some(keyword(defaults, v).map_or_else(|| value_of(property, defaults, v), Ok)?)
        };
        return Ok((property, defaults, value));
    }
    let split = value_start(abbr);
    let (p, v) = abbr.split_at(split);
    if v.is_empty() {
        if let Some((property, defaults)) = property_of_exact(p) {
            return Ok((property, defaults, None));
        }
        // `df` = `d` + keyword `f`; `posa`, `tac`, `fwb`, `ovh`.
        for cut in (1..p.len()).rev() {
            let (head, kw) = p.split_at(cut);
            if let Some((property, defaults)) = PROPS
                .iter()
                .find(|(k, _, _)| *k == head)
                .map(|(_, p, d)| (*p, *d))
            {
                if let Some(k) = keyword(defaults, kw).filter(|_| !defaults.contains("${")) {
                    return Ok((property, defaults, Some(k)));
                }
            }
        }
        let (property, defaults) = fuzzy(p).ok_or_else(|| format!("unknown property {p}"))?;
        return Ok((property, defaults, None));
    }
    let (property, defaults) = property_of(p).ok_or_else(|| format!("unknown property {p}"))?;
    let value = value_of(property, defaults, v)?;
    Ok((property, defaults, Some(value)))
}

/// Where the value part of `m10`, `m-10`, `bgc#fff`, `op.5` starts.
fn value_start(abbr: &str) -> usize {
    let b = abbr.as_bytes();
    for i in 0..b.len() {
        let next_digit = b.get(i + 1).is_some_and(u8::is_ascii_digit);
        if b[i].is_ascii_digit()
            || b[i] == b'#'
            || ((b[i] == b'-' || b[i] == b'.') && next_digit && i > 0)
        {
            return i;
        }
    }
    b.len()
}

fn property_of_exact(p: &str) -> Option<(&'static str, &'static str)> {
    if let Some((_, prop, d)) = PROPS.iter().find(|(k, _, _)| *k == p) {
        return Some((prop, d));
    }
    let name = PROPERTY_NAMES.iter().find(|n| **n == p)?;
    let defaults = PROPS
        .iter()
        .find(|(_, prop, _)| prop == name)
        .map_or("", |(_, _, d)| d);
    Some((name, defaults))
}

fn property_of(p: &str) -> Option<(&'static str, &'static str)> {
    property_of_exact(p).or_else(|| fuzzy(p))
}

/// Fuzzy property match like Emmet's: the letters in order, from the property's first
/// letter, preferring word starts (`bxsz` → `box-sizing`).
fn fuzzy(p: &str) -> Option<(&'static str, &'static str)> {
    if p.len() < 2 || !p.bytes().all(|c| c.is_ascii_lowercase() || c == b'-') {
        return None;
    }
    let mut best: Option<(i32, &'static str)> = None;
    for name in PROPERTY_NAMES {
        let Some(score) = fuzzy_score(p, name) else {
            continue;
        };
        if best.is_none_or(|(s, n)| score > s || (score == s && name.len() < n.len())) {
            best = Some((score, name));
        }
    }
    let (_, name) = best?;
    let defaults = PROPS
        .iter()
        .find(|(_, prop, _)| *prop == name)
        .map_or("", |(_, _, d)| d);
    Some((name, defaults))
}

fn fuzzy_score(abbr: &str, name: &str) -> Option<i32> {
    let a = abbr.as_bytes();
    let n = name.as_bytes();
    if a[0] != n[0] {
        return None;
    }
    let mut score = 0;
    let mut j = 0;
    let mut prev: Option<usize> = None;
    for &c in a {
        while j < n.len() && n[j] != c {
            j += 1;
        }
        if j == n.len() {
            return None;
        }
        score += 1;
        if j == 0 || n[j - 1] == b'-' {
            score += 2;
        }
        if prev == Some(j.wrapping_sub(1)) {
            score += 1;
        }
        prev = Some(j);
        j += 1;
    }
    Some(score)
}

/// A keyword of the property (`a` → `absolute` for `position`, `ib` → `inline-block`) or a
/// global one (`a` → `auto`).
fn keyword(defaults: &str, abbr: &str) -> Option<String> {
    if defaults.contains("${") || abbr.is_empty() {
        return GLOBAL_KEYWORDS
            .iter()
            .find(|(k, _)| *k == abbr)
            .map(|(_, v)| v.to_string());
    }
    let words: Vec<&str> = defaults.split('|').filter(|w| !w.is_empty()).collect();
    if let Some(w) = words.iter().find(|w| **w == abbr) {
        return Some(w.to_string());
    }
    // Initials of hyphenated words (`ib`, `sb` → space-between), then prefixes.
    let initials = |w: &str| {
        w.split('-')
            .filter_map(|p| p.chars().next())
            .collect::<String>()
    };
    if abbr.len() > 1 {
        if let Some(w) = words
            .iter()
            .find(|w| w.contains('-') && initials(w) == abbr)
        {
            return Some(w.to_string());
        }
    }
    if let Some(w) = words.iter().find(|w| w.starts_with(abbr)) {
        return Some(w.to_string());
    }
    GLOBAL_KEYWORDS
        .iter()
        .find(|(k, _)| *k == abbr)
        .map(|(_, v)| v.to_string())
}

/// The value part: numbers with units (`10-20`, `-10`, `100p`, `1.5`), colors (`#f`) and
/// keywords (`s` → `solid`), joined by spaces.
fn value_of(property: &str, defaults: &str, v: &str) -> Result<String, String> {
    let b: Vec<char> = v.chars().collect();
    let mut i = 0;
    let mut tokens: Vec<String> = Vec::new();
    let unitless = UNITLESS.contains(&property);
    while i < b.len() {
        let neg = b[i] == '-'
            && b.get(i + 1)
                .is_some_and(|c| c.is_ascii_digit() || *c == '.');
        if neg {
            i += 1;
        }
        let c = b[i];
        if c.is_ascii_digit() || c == '.' {
            let s = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == '.') {
                i += 1;
            }
            let num: String = b[s..i].iter().collect();
            let u = i;
            while i < b.len() && (b[i].is_ascii_alphabetic() || b[i] == '%') {
                i += 1;
            }
            let unit: String = b[u..i].iter().collect();
            let unit = match unit.as_str() {
                "" if unitless || num.parse::<f64>().is_ok_and(|n| n == 0.0) => "",
                "" if num.contains('.') => "em",
                "" => "px",
                "p" => "%",
                "e" => "em",
                "x" => "ex",
                "r" => "rem",
                other => other,
            }
            .to_string();
            tokens.push(format!("{}{num}{unit}", if neg { "-" } else { "" }));
        } else if c == '#' {
            let s = i + 1;
            i += 1;
            while i < b.len() && b[i].is_ascii_hexdigit() {
                i += 1;
            }
            let hex: String = b[s..i].iter().collect();
            if hex.is_empty() {
                return Err("empty color".into());
            }
            tokens.push(format!("#{}", expand_hex(&hex)));
        } else if c.is_ascii_alphabetic() {
            let s = i;
            while i < b.len() && b[i].is_ascii_alphabetic() {
                i += 1;
            }
            let word: String = b[s..i].iter().collect();
            tokens.push(keyword(defaults, &word).unwrap_or(word));
        } else {
            return Err(format!("unexpected '{c}' in value"));
        }
        // `10-20`: a separator; in `10--20` the second dash makes the next number negative.
        if i < b.len() && b[i] == '-' {
            i += 1;
        }
    }
    Ok(tokens.join(" "))
}

/// `f` → `fff`, `fc` → `fcfcfc`, `fc0` and longer stay.
fn expand_hex(hex: &str) -> String {
    match hex.len() {
        1 => hex.repeat(3),
        2 => hex.repeat(3),
        _ => hex.to_string(),
    }
}

/// Whether `word` could be a CSS abbreviation Emmet expands (for previews and JSX checks).
pub fn is_known(abbr: &str) -> bool {
    split_plus(abbr).iter().all(|p| {
        let p = p.strip_suffix('!').unwrap_or(p);
        if p.starts_with('@') {
            RULES.iter().any(|(k, _)| *k == p)
        } else {
            resolve(p).is_ok()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn css(abbr: &str) -> String {
        expand(abbr, &CssOptions::default()).unwrap().text
    }

    #[test]
    fn numbers_and_units() {
        assert_eq!(css("m10"), "margin: 10px;");
        assert_eq!(css("p10-20"), "padding: 10px 20px;");
        assert_eq!(css("m10-20-30-40"), "margin: 10px 20px 30px 40px;");
        assert_eq!(css("w100p"), "width: 100%;");
        assert_eq!(css("m-10"), "margin: -10px;");
        assert_eq!(css("m10--20"), "margin: 10px -20px;");
        assert_eq!(css("m1.5"), "margin: 1.5em;");
        assert_eq!(css("m2e"), "margin: 2em;");
        assert_eq!(css("p1r"), "padding: 1rem;");
        assert_eq!(css("h50vh"), "height: 50vh;");
        assert_eq!(css("m0"), "margin: 0;");
        assert_eq!(css("lh1.5"), "line-height: 1.5;");
        assert_eq!(css("z10"), "z-index: 10;");
        assert_eq!(css("op.5"), "opacity: .5;");
        assert_eq!(css("fw700"), "font-weight: 700;");
        assert_eq!(css("trsdu300ms"), "transition-duration: 300ms;");
    }

    #[test]
    fn keywords() {
        assert_eq!(css("df"), "display: flex;");
        assert_eq!(css("dib"), "display: inline-block;");
        assert_eq!(css("dn"), "display: none;");
        assert_eq!(css("pos:a"), "position: absolute;");
        assert_eq!(css("posr"), "position: relative;");
        assert_eq!(css("tac"), "text-align: center;");
        assert_eq!(css("fwb"), "font-weight: bold;");
        assert_eq!(css("ovh"), "overflow: hidden;");
        assert_eq!(css("jcsb"), "justify-content: space-between;");
        assert_eq!(css("m:a"), "margin: auto;");
        assert_eq!(css("w:auto"), "width: auto;");
        assert_eq!(css("fl"), "float: left;");
    }

    #[test]
    fn colors() {
        assert_eq!(css("bgc#fff"), "background-color: #fff;");
        assert_eq!(css("c#f"), "color: #fff;");
        assert_eq!(css("c#fc"), "color: #fcfcfc;");
        assert_eq!(css("c#3!"), "color: #333 !important;");
        assert_eq!(css("bd1-s#f00"), "border: 1px solid #f00;");
    }

    #[test]
    fn defaults_and_stops() {
        let out = expand("pos", &CssOptions::default()).unwrap();
        assert_eq!(out.text, "position: relative;");
        assert_eq!(
            (out.stops[0].from, out.stops[0].to, out.stops[0].index),
            (10, 18, 1)
        );
        let out = expand("m", &CssOptions::default()).unwrap();
        assert_eq!(out.text, "margin: ;");
        assert_eq!((out.stops[0].from, out.stops[0].to), (8, 8));
        let out = expand("bd", &CssOptions::default()).unwrap();
        assert_eq!(out.text, "border: 1px solid #000;");
        assert_eq!(out.stops.len(), 3);
        assert_eq!(css("color"), "color: #000;");
        assert_eq!(css("bxsz"), "box-sizing: border-box;");
    }

    #[test]
    fn several_declarations_and_rules() {
        let out = expand("m10+p5+bgc", &CssOptions::default()).unwrap();
        assert_eq!(
            out.text,
            "margin: 10px;\npadding: 5px;\nbackground-color: #fff;"
        );
        assert_eq!(out.stops[0].index, 1);
        let out = expand(
            "@m",
            &CssOptions {
                indent: "  ".into(),
                semicolons: true,
            },
        )
        .unwrap();
        assert_eq!(out.text, "@media screen {\n  \n}");
        assert_eq!(out.stops.len(), 2);
        assert_eq!(css("@i"), "@import url();");
    }

    #[test]
    fn indented_syntax_has_no_semicolons() {
        let o = CssOptions {
            indent: "  ".into(),
            semicolons: false,
        };
        assert_eq!(expand("m10", &o).unwrap().text, "margin: 10px");
    }

    #[test]
    fn unknown_words_do_not_expand() {
        for bad in ["xyzzy", "qq10", "@nope", "!", "m10#"] {
            assert!(expand(bad, &CssOptions::default()).is_err(), "{bad}");
        }
        assert!(is_known("m10") && is_known("df") && !is_known("qqq"));
    }
}
