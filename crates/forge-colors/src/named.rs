//! Named colors: the CSS color keywords and Tailwind CSS's default palette.

use crate::color::Rgba;

/// The 148 CSS named colors (`transparent` is handled on its own).
const NAMED: &[(&str, u32)] = &[
    ("aliceblue", 0xf0f8ff),
    ("antiquewhite", 0xfaebd7),
    ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4),
    ("azure", 0xf0ffff),
    ("beige", 0xf5f5dc),
    ("bisque", 0xffe4c4),
    ("black", 0x000000),
    ("blanchedalmond", 0xffebcd),
    ("blue", 0x0000ff),
    ("blueviolet", 0x8a2be2),
    ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887),
    ("cadetblue", 0x5f9ea0),
    ("chartreuse", 0x7fff00),
    ("chocolate", 0xd2691e),
    ("coral", 0xff7f50),
    ("cornflowerblue", 0x6495ed),
    ("cornsilk", 0xfff8dc),
    ("crimson", 0xdc143c),
    ("cyan", 0x00ffff),
    ("darkblue", 0x00008b),
    ("darkcyan", 0x008b8b),
    ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xa9a9a9),
    ("darkkhaki", 0xbdb76b),
    ("darkmagenta", 0x8b008b),
    ("darkolivegreen", 0x556b2f),
    ("darkorange", 0xff8c00),
    ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000),
    ("darksalmon", 0xe9967a),
    ("darkseagreen", 0x8fbc8f),
    ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f),
    ("darkslategrey", 0x2f4f4f),
    ("darkturquoise", 0x00ced1),
    ("darkviolet", 0x9400d3),
    ("deeppink", 0xff1493),
    ("deepskyblue", 0x00bfff),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff),
    ("firebrick", 0xb22222),
    ("floralwhite", 0xfffaf0),
    ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff),
    ("gainsboro", 0xdcdcdc),
    ("ghostwhite", 0xf8f8ff),
    ("gold", 0xffd700),
    ("goldenrod", 0xdaa520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xadff2f),
    ("grey", 0x808080),
    ("honeydew", 0xf0fff0),
    ("hotpink", 0xff69b4),
    ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082),
    ("ivory", 0xfffff0),
    ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa),
    ("lavenderblush", 0xfff0f5),
    ("lawngreen", 0x7cfc00),
    ("lemonchiffon", 0xfffacd),
    ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080),
    ("lightcyan", 0xe0ffff),
    ("lightgoldenrodyellow", 0xfafad2),
    ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90),
    ("lightgrey", 0xd3d3d3),
    ("lightpink", 0xffb6c1),
    ("lightsalmon", 0xffa07a),
    ("lightseagreen", 0x20b2aa),
    ("lightskyblue", 0x87cefa),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de),
    ("lightyellow", 0xffffe0),
    ("lime", 0x00ff00),
    ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6),
    ("magenta", 0xff00ff),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa),
    ("mediumblue", 0x0000cd),
    ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db),
    ("mediumseagreen", 0x3cb371),
    ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a),
    ("mediumturquoise", 0x48d1cc),
    ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xf5fffa),
    ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5),
    ("navajowhite", 0xffdead),
    ("navy", 0x000080),
    ("oldlace", 0xfdf5e6),
    ("olive", 0x808000),
    ("olivedrab", 0x6b8e23),
    ("orange", 0xffa500),
    ("orangered", 0xff4500),
    ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa),
    ("palegreen", 0x98fb98),
    ("paleturquoise", 0xafeeee),
    ("palevioletred", 0xdb7093),
    ("papayawhip", 0xffefd5),
    ("peachpuff", 0xffdab9),
    ("peru", 0xcd853f),
    ("pink", 0xffc0cb),
    ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xff0000),
    ("rosybrown", 0xbc8f8f),
    ("royalblue", 0x4169e1),
    ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072),
    ("sandybrown", 0xf4a460),
    ("seagreen", 0x2e8b57),
    ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d),
    ("silver", 0xc0c0c0),
    ("skyblue", 0x87ceeb),
    ("slateblue", 0x6a5acd),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f),
    ("steelblue", 0x4682b4),
    ("tan", 0xd2b48c),
    ("teal", 0x008080),
    ("thistle", 0xd8bfd8),
    ("tomato", 0xff6347),
    ("turquoise", 0x40e0d0),
    ("violet", 0xee82ee),
    ("wheat", 0xf5deb3),
    ("white", 0xffffff),
    ("whitesmoke", 0xf5f5f5),
    ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

/// Tailwind CSS v3 default palette: shades 50, 100, 200 … 900, 950.
const TAILWIND: &[(&str, [u32; 11])] = &[
    (
        "slate",
        [
            0xf8fafc, 0xf1f5f9, 0xe2e8f0, 0xcbd5e1, 0x94a3b8, 0x64748b, 0x475569, 0x334155,
            0x1e293b, 0x0f172a, 0x020617,
        ],
    ),
    (
        "gray",
        [
            0xf9fafb, 0xf3f4f6, 0xe5e7eb, 0xd1d5db, 0x9ca3af, 0x6b7280, 0x4b5563, 0x374151,
            0x1f2937, 0x111827, 0x030712,
        ],
    ),
    (
        "zinc",
        [
            0xfafafa, 0xf4f4f5, 0xe4e4e7, 0xd4d4d8, 0xa1a1aa, 0x71717a, 0x52525b, 0x3f3f46,
            0x27272a, 0x18181b, 0x09090b,
        ],
    ),
    (
        "neutral",
        [
            0xfafafa, 0xf5f5f5, 0xe5e5e5, 0xd4d4d4, 0xa3a3a3, 0x737373, 0x525252, 0x404040,
            0x262626, 0x171717, 0x0a0a0a,
        ],
    ),
    (
        "stone",
        [
            0xfafaf9, 0xf5f5f4, 0xe7e5e4, 0xd6d3d1, 0xa8a29e, 0x78716c, 0x57534e, 0x44403c,
            0x292524, 0x1c1917, 0x0c0a09,
        ],
    ),
    (
        "red",
        [
            0xfef2f2, 0xfee2e2, 0xfecaca, 0xfca5a5, 0xf87171, 0xef4444, 0xdc2626, 0xb91c1c,
            0x991b1b, 0x7f1d1d, 0x450a0a,
        ],
    ),
    (
        "orange",
        [
            0xfff7ed, 0xffedd5, 0xfed7aa, 0xfdba74, 0xfb923c, 0xf97316, 0xea580c, 0xc2410c,
            0x9a3412, 0x7c2d12, 0x431407,
        ],
    ),
    (
        "amber",
        [
            0xfffbeb, 0xfef3c7, 0xfde68a, 0xfcd34d, 0xfbbf24, 0xf59e0b, 0xd97706, 0xb45309,
            0x92400e, 0x78350f, 0x451a03,
        ],
    ),
    (
        "yellow",
        [
            0xfefce8, 0xfef9c3, 0xfef08a, 0xfde047, 0xfacc15, 0xeab308, 0xca8a04, 0xa16207,
            0x854d0e, 0x713f12, 0x422006,
        ],
    ),
    (
        "lime",
        [
            0xf7fee7, 0xecfccb, 0xd9f99d, 0xbef264, 0xa3e635, 0x84cc16, 0x65a30d, 0x4d7c0f,
            0x3f6212, 0x365314, 0x1a2e05,
        ],
    ),
    (
        "green",
        [
            0xf0fdf4, 0xdcfce7, 0xbbf7d0, 0x86efac, 0x4ade80, 0x22c55e, 0x16a34a, 0x15803d,
            0x166534, 0x14532d, 0x052e16,
        ],
    ),
    (
        "emerald",
        [
            0xecfdf5, 0xd1fae5, 0xa7f3d0, 0x6ee7b7, 0x34d399, 0x10b981, 0x059669, 0x047857,
            0x065f46, 0x064e3b, 0x022c22,
        ],
    ),
    (
        "teal",
        [
            0xf0fdfa, 0xccfbf1, 0x99f6e4, 0x5eead4, 0x2dd4bf, 0x14b8a6, 0x0d9488, 0x0f766e,
            0x115e59, 0x134e4a, 0x042f2e,
        ],
    ),
    (
        "cyan",
        [
            0xecfeff, 0xcffafe, 0xa5f3fc, 0x67e8f9, 0x22d3ee, 0x06b6d4, 0x0891b2, 0x0e7490,
            0x155e75, 0x164e63, 0x083344,
        ],
    ),
    (
        "sky",
        [
            0xf0f9ff, 0xe0f2fe, 0xbae6fd, 0x7dd3fc, 0x38bdf8, 0x0ea5e9, 0x0284c7, 0x0369a1,
            0x075985, 0x0c4a6e, 0x082f49,
        ],
    ),
    (
        "blue",
        [
            0xeff6ff, 0xdbeafe, 0xbfdbfe, 0x93c5fd, 0x60a5fa, 0x3b82f6, 0x2563eb, 0x1d4ed8,
            0x1e40af, 0x1e3a8a, 0x172554,
        ],
    ),
    (
        "indigo",
        [
            0xeef2ff, 0xe0e7ff, 0xc7d2fe, 0xa5b4fc, 0x818cf8, 0x6366f1, 0x4f46e5, 0x4338ca,
            0x3730a3, 0x312e81, 0x1e1b4b,
        ],
    ),
    (
        "violet",
        [
            0xf5f3ff, 0xede9fe, 0xddd6fe, 0xc4b5fd, 0xa78bfa, 0x8b5cf6, 0x7c3aed, 0x6d28d9,
            0x5b21b6, 0x4c1d95, 0x2e1065,
        ],
    ),
    (
        "purple",
        [
            0xfaf5ff, 0xf3e8ff, 0xe9d5ff, 0xd8b4fe, 0xc084fc, 0xa855f7, 0x9333ea, 0x7e22ce,
            0x6b21a8, 0x581c87, 0x3b0764,
        ],
    ),
    (
        "fuchsia",
        [
            0xfdf4ff, 0xfae8ff, 0xf5d0fe, 0xf0abfc, 0xe879f9, 0xd946ef, 0xc026d3, 0xa21caf,
            0x86198f, 0x701a75, 0x4a044e,
        ],
    ),
    (
        "pink",
        [
            0xfdf2f8, 0xfce7f3, 0xfbcfe8, 0xf9a8d4, 0xf472b6, 0xec4899, 0xdb2777, 0xbe185d,
            0x9d174d, 0x831843, 0x500724,
        ],
    ),
    (
        "rose",
        [
            0xfff1f2, 0xffe4e6, 0xfecdd3, 0xfda4af, 0xfb7185, 0xf43f5e, 0xe11d48, 0xbe123c,
            0x9f1239, 0x881337, 0x4c0519,
        ],
    ),
];

const SHADES: [u32; 11] = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950];

fn rgb(v: u32) -> Rgba {
    Rgba::new(
        f64::from((v >> 16) & 0xff),
        f64::from((v >> 8) & 0xff),
        f64::from(v & 0xff),
        1.0,
    )
}

/// A CSS named color (any case), including `transparent`.
pub fn named_color(name: &str) -> Option<Rgba> {
    let lower = name.to_ascii_lowercase();
    if lower == "transparent" {
        return Some(Rgba::new(0.0, 0.0, 0.0, 0.0));
    }
    NAMED
        .iter()
        .find(|(n, _)| *n == lower)
        .map(|(_, v)| rgb(*v))
}

/// The name of an opaque color, if it has one (`gray` rather than `grey`).
pub fn name_of(color: Rgba) -> Option<&'static str> {
    if (color.a - 1.0).abs() > 1e-9 {
        return (color.a == 0.0 && color.r == 0.0 && color.g == 0.0 && color.b == 0.0)
            .then_some("transparent");
    }
    let v =
        ((color.r.round() as u32) << 16) | ((color.g.round() as u32) << 8) | color.b.round() as u32;
    NAMED.iter().find(|(_, c)| *c == v).map(|(n, _)| *n)
}

/// A Tailwind palette color: `red`, `500` → `#ef4444`; `black`, `white`.
pub fn tailwind_color(name: &str, shade: Option<u32>) -> Option<Rgba> {
    match (name, shade) {
        ("black", None) => return Some(rgb(0x000000)),
        ("white", None) => return Some(rgb(0xffffff)),
        _ => {}
    }
    let shade = shade?;
    let i = SHADES.iter().position(|s| *s == shade)?;
    TAILWIND
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| rgb(v[i]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_colors() {
        assert_eq!(named_color("RED").unwrap().hex(), "#ff0000");
        assert_eq!(named_color("transparent").unwrap().a, 0.0);
        assert!(named_color("reddish").is_none());
        assert_eq!(name_of(rgb(0x808080)), Some("gray"));
        assert_eq!(name_of(rgb(0x00ffff)), Some("aqua"));
        assert_eq!(name_of(rgb(0x123456)), None);
        assert_eq!(NAMED.len(), 148);
    }

    #[test]
    fn tailwind_palette() {
        assert_eq!(tailwind_color("blue", Some(500)).unwrap().hex(), "#3b82f6");
        assert_eq!(tailwind_color("slate", Some(950)).unwrap().hex(), "#020617");
        assert_eq!(tailwind_color("white", None).unwrap().hex(), "#ffffff");
        assert!(tailwind_color("blue", Some(550)).is_none());
        assert!(tailwind_color("brand", Some(500)).is_none());
    }
}
