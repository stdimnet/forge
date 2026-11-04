//! Text attributes, the attribute list of the Color Scheme page and the two bundled schemes.

use serde::{Deserialize, Serialize};

/// Decoration drawn under, through or around the text (IntelliJ's effect types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Effect {
    Underline,
    BoldUnderline,
    WaveUnderline,
    Strikethrough,
    DottedLine,
    Bordered,
}

impl Effect {
    pub const ALL: [Effect; 6] = [
        Effect::Underline,
        Effect::BoldUnderline,
        Effect::WaveUnderline,
        Effect::Strikethrough,
        Effect::DottedLine,
        Effect::Bordered,
    ];

    /// Name in the settings encoding and the UI.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Underline => "underline",
            Self::BoldUnderline => "boldUnderline",
            Self::WaveUnderline => "waveUnderline",
            Self::Strikethrough => "strikethrough",
            Self::DottedLine => "dottedLine",
            Self::Bordered => "bordered",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.as_str() == s)
    }

    /// Label in PHPStorm's effect list.
    pub fn title(self) -> &'static str {
        match self {
            Self::Underline => "Underscored",
            Self::BoldUnderline => "Bold underscored",
            Self::WaveUnderline => "Underwaved",
            Self::Strikethrough => "Strikeout",
            Self::DottedLine => "Dotted line",
            Self::Bordered => "Bordered",
        }
    }

    /// `EFFECT_TYPE` in .icls files.
    pub fn icls_id(self) -> u8 {
        match self {
            Self::Bordered => 0,
            Self::Underline => 1,
            Self::WaveUnderline => 2,
            Self::Strikethrough => 3,
            Self::BoldUnderline => 4,
            Self::DottedLine => 5,
        }
    }

    pub fn from_icls(id: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.icls_id() == id)
    }
}

/// How text of one attribute looks. Colors are `#rrggbb`; `None` keeps the editor's.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextAttributes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreground: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    pub bold: bool,
    pub italic: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<Effect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect_color: Option<String>,
}

impl TextAttributes {
    pub fn fg(color: &str) -> Self {
        Self {
            foreground: Some(color.into()),
            ..Self::default()
        }
    }

    pub fn bg(color: &str) -> Self {
        Self {
            background: Some(color.into()),
            ..Self::default()
        }
    }

    fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Normalize colors (`#RGB`, `#RRGGBB` in any case, with or without `#`) and drop
    /// invalid ones; an effect color without an effect is dropped.
    pub fn normalized(mut self) -> Self {
        self.foreground = self.foreground.as_deref().and_then(normalize_color);
        self.background = self.background.as_deref().and_then(normalize_color);
        self.effect_color = self
            .effect
            .and(self.effect_color.as_deref())
            .and_then(normalize_color);
        self
    }

    /// The settings encoding: space separated `fg=#…`, `bg=#…`, `bold`, `italic`,
    /// `effect=<name>`, `effectColor=#…`.
    pub fn encode(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(c) = &self.foreground {
            parts.push(format!("fg={c}"));
        }
        if let Some(c) = &self.background {
            parts.push(format!("bg={c}"));
        }
        if self.bold {
            parts.push("bold".into());
        }
        if self.italic {
            parts.push("italic".into());
        }
        if let Some(e) = self.effect {
            parts.push(format!("effect={}", e.as_str()));
        }
        if let Some(c) = &self.effect_color {
            parts.push(format!("effectColor={c}"));
        }
        parts.join(" ")
    }

    /// Read [`TextAttributes::encode`]; unknown words are ignored.
    pub fn decode(text: &str) -> Self {
        let mut out = Self::default();
        for word in text.split_whitespace() {
            match word.split_once('=') {
                Some(("fg", c)) => out.foreground = Some(c.into()),
                Some(("bg", c)) => out.background = Some(c.into()),
                Some(("effect", e)) => out.effect = Effect::parse(e),
                Some(("effectColor", c)) => out.effect_color = Some(c.into()),
                None if word == "bold" => out.bold = true,
                None if word == "italic" => out.italic = true,
                _ => {}
            }
        }
        out.normalized()
    }
}

/// `#rrggbb` for `#RGB`, `#RRGGBB`, `RRGGBB` and IntelliJ's short forms (`0` = `#000000`,
/// leading zeros dropped); `None` for anything else. An alpha byte (`#RRGGBBAA`) is dropped.
pub fn normalize_color(raw: &str) -> Option<String> {
    let hex = raw.trim().trim_start_matches('#');
    if hex.is_empty() || hex.len() > 8 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let hex = hex.to_ascii_lowercase();
    let full = match hex.len() {
        3 if raw.trim().starts_with('#') => hex.chars().flat_map(|c| [c, c]).collect(),
        8 => hex[..6].to_string(),
        7 => return None,
        _ => format!("{hex:0>6}"),
    };
    Some(format!("#{full}"))
}

/// Which parts of an attribute the page lets the user edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fields {
    pub foreground: bool,
    pub background: bool,
    /// Bold and italic.
    pub font: bool,
    pub effect: bool,
}

const TEXT_FIELDS: Fields = Fields {
    foreground: true,
    background: true,
    font: true,
    effect: true,
};
const FG_ONLY: Fields = Fields {
    foreground: true,
    background: false,
    font: false,
    effect: false,
};
const BG_ONLY: Fields = Fields {
    foreground: false,
    background: true,
    font: false,
    effect: false,
};
const FG_BG: Fields = Fields {
    foreground: true,
    background: true,
    font: false,
    effect: false,
};

/// Where an attribute ends up in the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A token category: `--syn-<name>` plus `-bg`, `-weight`, `-style`, `-deco`, `-box`.
    Syntax(&'static str),
    /// Editor colors: the variable for the foreground and the one for the background.
    Editor {
        fg: Option<&'static str>,
        bg: Option<&'static str>,
    },
}

/// One entry of the attribute tree.
#[derive(Debug, Clone, Copy)]
pub struct AttributeSpec {
    pub key: &'static str,
    pub title: &'static str,
    /// Tree group ("General", "Language Defaults").
    pub group: &'static str,
    pub fields: Fields,
    pub target: Target,
    /// IntelliJ names that map onto it, the first one is written on export. Names of the
    /// `<colors>` section start with `@` (their value is the color itself).
    pub icls: &'static [&'static str],
}

/// The attributes of Settings | Editor | Color Scheme, in tree order.
pub const ATTRIBUTES: &[AttributeSpec] = &[
    AttributeSpec {
        key: "TEXT",
        title: "Default text",
        group: "General",
        fields: FG_BG,
        target: Target::Editor {
            fg: Some("--editor-fg"),
            bg: Some("--bg-editor"),
        },
        icls: &["TEXT"],
    },
    AttributeSpec {
        key: "CARET",
        title: "Caret",
        group: "General",
        fields: FG_ONLY,
        target: Target::Editor {
            fg: Some("--editor-caret"),
            bg: None,
        },
        icls: &["@CARET_COLOR"],
    },
    AttributeSpec {
        key: "CARET_ROW",
        title: "Caret row",
        group: "General",
        fields: BG_ONLY,
        target: Target::Editor {
            fg: None,
            bg: Some("--editor-caret-row"),
        },
        icls: &["@CARET_ROW_COLOR"],
    },
    AttributeSpec {
        key: "SELECTION",
        title: "Selection background",
        group: "General",
        fields: BG_ONLY,
        target: Target::Editor {
            fg: None,
            bg: Some("--editor-selection"),
        },
        icls: &["@SELECTION_BACKGROUND"],
    },
    AttributeSpec {
        key: "LINE_NUMBERS",
        title: "Line number",
        group: "General",
        fields: FG_ONLY,
        target: Target::Editor {
            fg: Some("--editor-line-number"),
            bg: None,
        },
        icls: &["@LINE_NUMBERS_COLOR"],
    },
    AttributeSpec {
        key: "SEARCH_RESULT",
        title: "Search result",
        group: "General",
        fields: BG_ONLY,
        target: Target::Editor {
            fg: None,
            bg: Some("--editor-search-match"),
        },
        icls: &["SEARCH_RESULT_ATTRIBUTES", "TEXT_SEARCH_RESULT_ATTRIBUTES"],
    },
    AttributeSpec {
        key: "KEYWORD",
        title: "Keyword",
        group: "Language Defaults",
        fields: TEXT_FIELDS,
        target: Target::Syntax("keyword"),
        icls: &["DEFAULT_KEYWORD", "PHP_KEYWORD"],
    },
    AttributeSpec {
        key: "STRING",
        title: "String",
        group: "Language Defaults",
        fields: TEXT_FIELDS,
        target: Target::Syntax("string"),
        icls: &["DEFAULT_STRING", "PHP_STRING"],
    },
    AttributeSpec {
        key: "NUMBER",
        title: "Number",
        group: "Language Defaults",
        fields: TEXT_FIELDS,
        target: Target::Syntax("number"),
        icls: &["DEFAULT_NUMBER", "PHP_NUMBER"],
    },
    AttributeSpec {
        key: "COMMENT",
        title: "Comment",
        group: "Language Defaults",
        fields: TEXT_FIELDS,
        target: Target::Syntax("comment"),
        icls: &[
            "DEFAULT_LINE_COMMENT",
            "DEFAULT_BLOCK_COMMENT",
            "DEFAULT_DOC_COMMENT",
            "PHP_COMMENT",
        ],
    },
    AttributeSpec {
        key: "VARIABLE",
        title: "Variable",
        group: "Language Defaults",
        fields: TEXT_FIELDS,
        target: Target::Syntax("variable"),
        icls: &["PHP_VAR", "DEFAULT_LOCAL_VARIABLE", "DEFAULT_IDENTIFIER"],
    },
    AttributeSpec {
        key: "FUNCTION",
        title: "Function",
        group: "Language Defaults",
        fields: TEXT_FIELDS,
        target: Target::Syntax("function"),
        icls: &[
            "DEFAULT_FUNCTION_DECLARATION",
            "DEFAULT_FUNCTION_CALL",
            "PHP_FUNCTION_CALL",
        ],
    },
    AttributeSpec {
        key: "TYPE",
        title: "Class name",
        group: "Language Defaults",
        fields: TEXT_FIELDS,
        target: Target::Syntax("type"),
        icls: &["DEFAULT_CLASS_NAME", "DEFAULT_CLASS_REFERENCE", "PHP_CLASS"],
    },
    AttributeSpec {
        key: "ATTRIBUTE",
        title: "Property and attribute",
        group: "Language Defaults",
        fields: TEXT_FIELDS,
        target: Target::Syntax("attribute"),
        icls: &[
            "DEFAULT_INSTANCE_FIELD",
            "DEFAULT_ATTRIBUTE",
            "DEFAULT_METADATA",
            "HTML_ATTRIBUTE_NAME",
        ],
    },
    AttributeSpec {
        key: "OPERATOR",
        title: "Operator sign",
        group: "Language Defaults",
        fields: TEXT_FIELDS,
        target: Target::Syntax("operator"),
        icls: &[
            "DEFAULT_OPERATION_SIGN",
            "DEFAULT_BRACES",
            "DEFAULT_PARENTHS",
        ],
    },
    AttributeSpec {
        key: "TAG",
        title: "Markup tag",
        group: "Language Defaults",
        fields: TEXT_FIELDS,
        target: Target::Syntax("tag"),
        icls: &["DEFAULT_TAG", "HTML_TAG_NAME", "XML_TAG_NAME"],
    },
];

pub fn attribute(key: &str) -> Option<&'static AttributeSpec> {
    ATTRIBUTES.iter().find(|a| a.key == key)
}

/// The bundled schemes, matching the theme CSS (`app/ui/src/theme/theme.css`): Ember and Anvil
/// for the Forge themes, High Contrast and High Contrast Light for the high-contrast themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bundled {
    Ember,
    Anvil,
    HighContrast,
    HighContrastLight,
}

/// `true` (a dark UI theme) follows Ember, `false` Anvil.
impl From<bool> for Bundled {
    fn from(dark: bool) -> Self {
        Self::for_theme(dark, false)
    }
}

/// Colors of a bundled scheme, in the order of [`ATTRIBUTES`] below.
struct Palette {
    text: (&'static str, &'static str),
    caret: &'static str,
    caret_row: &'static str,
    selection: &'static str,
    line_numbers: &'static str,
    search_result: &'static str,
    /// KEYWORD, STRING, NUMBER, COMMENT, VARIABLE, FUNCTION, TYPE, ATTRIBUTE, OPERATOR, TAG.
    syntax: [&'static str; 10],
}

const EMBER: Palette = Palette {
    text: ("#d9d9df", "#141417"),
    caret: "#ff7a1a",
    caret_row: "#1e1e23",
    selection: "#3a2a1e",
    line_numbers: "#8a8a95",
    search_result: "#483c24",
    syntax: [
        "#ff9a57", "#a5d17f", "#7cc5e8", "#80808e", "#c9a6ff", "#ffd479", "#6fd2c4", "#d0c070",
        "#c4c4cc", "#ff9a57",
    ],
};

const ANVIL: Palette = Palette {
    text: ("#26272b", "#ffffff"),
    caret: "#c4500a",
    caret_row: "#f1f2f6",
    selection: "#ffe3cc",
    line_numbers: "#686b74",
    search_result: "#f8ecd2",
    syntax: [
        "#b8460b", "#3c7d24", "#1b6fa0", "#6e717b", "#7a3fc4", "#8a5a00", "#157a6e", "#806a00",
        "#3a3c42", "#b8460b",
    ],
};

const HIGH_CONTRAST: Palette = Palette {
    text: ("#ffffff", "#000000"),
    caret: "#ffa64d",
    caret_row: "#1a1a1a",
    selection: "#5c3300",
    line_numbers: "#cccccc",
    search_result: "#5c4a00",
    syntax: [
        "#ffb877", "#9cf08c", "#8fdcff", "#c8c8c8", "#e6ccff", "#ffe28f", "#80f0e0", "#f5e98f",
        "#ffffff", "#ffb877",
    ],
};

const HIGH_CONTRAST_LIGHT: Palette = Palette {
    text: ("#000000", "#ffffff"),
    caret: "#9a3a00",
    caret_row: "#ededed",
    selection: "#ffd9b3",
    line_numbers: "#3d3d3d",
    search_result: "#ffe680",
    syntax: [
        "#8a2c00", "#1d5c0c", "#004f8a", "#4a4a4a", "#561f99", "#614000", "#00574d", "#574a00",
        "#000000", "#8a2c00",
    ],
};

impl Bundled {
    pub const ALL: [Bundled; 4] = [
        Bundled::Ember,
        Bundled::Anvil,
        Bundled::HighContrast,
        Bundled::HighContrastLight,
    ];

    /// The scheme that follows a UI theme: dark or light, high contrast or not.
    pub fn for_theme(dark: bool, high_contrast: bool) -> Self {
        match (dark, high_contrast) {
            (true, false) => Self::Ember,
            (false, false) => Self::Anvil,
            (true, true) => Self::HighContrast,
            (false, true) => Self::HighContrastLight,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Ember => "Ember",
            Self::Anvil => "Anvil",
            Self::HighContrast => "High Contrast",
            Self::HighContrastLight => "High Contrast Light",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.name() == s)
    }

    pub fn dark(self) -> bool {
        matches!(self, Self::Ember | Self::HighContrast)
    }

    /// PHPStorm's scheme of the same kind (the `parent_scheme` of exported files).
    pub fn icls_parent(self) -> &'static str {
        match self {
            Self::Ember => "Darcula",
            Self::Anvil | Self::HighContrastLight => "Default",
            Self::HighContrast => "High contrast",
        }
    }

    /// The bundled scheme an IntelliJ parent maps onto.
    pub fn from_icls_parent(parent: &str) -> Self {
        let p = parent.to_ascii_lowercase();
        if p.contains("high contrast") || p.contains("high_contrast") || p.contains("highcontrast") {
            if p.contains("light") {
                Self::HighContrastLight
            } else {
                Self::HighContrast
            }
        } else if p.contains("darcula") || p.contains("dark") || p.contains("ember") {
            Self::Ember
        } else {
            Self::Anvil
        }
    }

    fn palette(self) -> &'static Palette {
        match self {
            Self::Ember => &EMBER,
            Self::Anvil => &ANVIL,
            Self::HighContrast => &HIGH_CONTRAST,
            Self::HighContrastLight => &HIGH_CONTRAST_LIGHT,
        }
    }

    /// The attributes of the scheme (every key of [`ATTRIBUTES`]).
    pub fn attributes(self) -> Vec<(&'static str, TextAttributes)> {
        let p = self.palette();
        let high_contrast = matches!(self, Self::HighContrast | Self::HighContrastLight);
        let mut out = vec![
            (
                "TEXT",
                TextAttributes {
                    foreground: Some(p.text.0.into()),
                    background: Some(p.text.1.into()),
                    ..TextAttributes::default()
                },
            ),
            ("CARET", TextAttributes::fg(p.caret)),
            ("CARET_ROW", TextAttributes::bg(p.caret_row)),
            ("SELECTION", TextAttributes::bg(p.selection)),
            ("LINE_NUMBERS", TextAttributes::fg(p.line_numbers)),
            ("SEARCH_RESULT", TextAttributes::bg(p.search_result)),
        ];
        const SYNTAX: [&str; 10] = [
            "KEYWORD",
            "STRING",
            "NUMBER",
            "COMMENT",
            "VARIABLE",
            "FUNCTION",
            "TYPE",
            "ATTRIBUTE",
            "OPERATOR",
            "TAG",
        ];
        for (key, color) in SYNTAX.into_iter().zip(p.syntax) {
            let mut a = TextAttributes::fg(color);
            if key == "COMMENT" {
                a = a.italic();
            }
            // High contrast does not rely on color alone: keywords are bold as well.
            if high_contrast && key == "KEYWORD" {
                a.bold = true;
            }
            out.push((key, a));
        }
        out
    }

    pub fn attribute(self, key: &str) -> TextAttributes {
        self.attributes()
            .into_iter()
            .find(|(k, _)| *k == key)
            .map(|(_, a)| a)
            .unwrap_or_default()
    }
}

/// Inline CSS of one attribute (for the preview and the attribute list).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CssStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_decoration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub box_shadow: Option<String>,
}

/// CSS `text-decoration` / `box-shadow` of an effect.
fn effect_css(effect: Effect, color: &str) -> (Option<String>, Option<String>) {
    match effect {
        Effect::Underline => (Some(format!("underline {color}")), None),
        Effect::BoldUnderline => (Some(format!("underline 2px {color}")), None),
        Effect::WaveUnderline => (Some(format!("underline wavy {color}")), None),
        Effect::Strikethrough => (Some(format!("line-through {color}")), None),
        Effect::DottedLine => (Some(format!("underline dotted {color}")), None),
        Effect::Bordered => (None, Some(format!("inset 0 0 0 1px {color}"))),
    }
}

impl TextAttributes {
    /// Inline CSS; `text_fg` colors effects that have no color of their own.
    pub fn style(&self, text_fg: &str) -> CssStyle {
        let fg = self.foreground.clone();
        let (deco, shadow) = match self.effect {
            Some(e) => {
                let color = self
                    .effect_color
                    .clone()
                    .or_else(|| fg.clone())
                    .unwrap_or_else(|| text_fg.to_string());
                effect_css(e, &color)
            }
            None => (None, None),
        };
        CssStyle {
            color: fg,
            background_color: self.background.clone(),
            font_weight: self.bold.then(|| "bold".into()),
            font_style: self.italic.then(|| "italic".into()),
            text_decoration: deco,
            box_shadow: shadow,
        }
    }

    /// CSS variables for the editor (`Target::Syntax` sets all of them, so a scheme fully
    /// replaces the theme's token styles).
    pub fn css_vars(&self, target: Target, text_fg: &str) -> Vec<(String, String)> {
        match target {
            Target::Editor { fg, bg } => {
                let mut out = Vec::new();
                if let (Some(var), Some(c)) = (fg, &self.foreground) {
                    out.push((var.to_string(), c.clone()));
                }
                if let (Some(var), Some(c)) = (bg, &self.background) {
                    out.push((var.to_string(), c.clone()));
                }
                out
            }
            Target::Syntax(name) => {
                let style = self.style(text_fg);
                let v = |suffix: &str, value: Option<String>, none: &str| {
                    (
                        format!("--syn-{name}{suffix}"),
                        value.unwrap_or_else(|| none.to_string()),
                    )
                };
                vec![
                    v("", style.color, text_fg),
                    v("-bg", style.background_color, "transparent"),
                    v("-weight", style.font_weight, "normal"),
                    v("-style", style.font_style, "normal"),
                    v("-deco", style.text_decoration, "none"),
                    v("-box", style.box_shadow, "none"),
                ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_colors() {
        assert_eq!(normalize_color("#ABC").as_deref(), Some("#aabbcc"));
        assert_eq!(normalize_color("CC7832").as_deref(), Some("#cc7832"));
        assert_eq!(normalize_color("0").as_deref(), Some("#000000"));
        assert_eq!(normalize_color("ff00").as_deref(), Some("#00ff00"));
        assert_eq!(normalize_color("#11223344").as_deref(), Some("#112233"));
        assert_eq!(normalize_color("red"), None);
        assert_eq!(normalize_color(""), None);
    }

    #[test]
    fn encodes_and_decodes_attributes() {
        let a = TextAttributes {
            foreground: Some("#cc7832".into()),
            background: None,
            bold: true,
            italic: true,
            effect: Some(Effect::WaveUnderline),
            effect_color: Some("#ff0000".into()),
        };
        let text = a.encode();
        assert_eq!(
            text,
            "fg=#cc7832 bold italic effect=waveUnderline effectColor=#ff0000"
        );
        assert_eq!(TextAttributes::decode(&text), a);
        assert_eq!(
            TextAttributes::decode("fg=nope junk"),
            TextAttributes::default()
        );
        // An effect color needs an effect.
        assert_eq!(
            TextAttributes::decode("effectColor=#ff0000").effect_color,
            None
        );
    }

    #[test]
    fn bundled_schemes_cover_every_attribute() {
        for b in Bundled::ALL {
            let keys: Vec<&str> = b.attributes().into_iter().map(|(k, _)| k).collect();
            let all: Vec<&str> = ATTRIBUTES.iter().map(|a| a.key).collect();
            assert_eq!(keys, all, "{b:?}");
        }
        assert_eq!(
            Bundled::Ember.attribute("KEYWORD").foreground.as_deref(),
            Some("#ff9a57")
        );
        assert!(Bundled::Anvil.attribute("COMMENT").italic);
        assert!(Bundled::HighContrast.attribute("KEYWORD").bold);
        assert!(!Bundled::Ember.attribute("KEYWORD").bold);
    }

    #[test]
    fn maps_themes_and_icls_parents() {
        assert_eq!(Bundled::from(true), Bundled::Ember);
        assert_eq!(Bundled::from(false), Bundled::Anvil);
        assert_eq!(Bundled::for_theme(true, true), Bundled::HighContrast);
        assert_eq!(Bundled::for_theme(false, true), Bundled::HighContrastLight);
        assert_eq!(Bundled::from_icls_parent("High contrast"), Bundled::HighContrast);
        assert_eq!(Bundled::from_icls_parent("Darcula"), Bundled::Ember);
        assert_eq!(Bundled::parse("High Contrast Light"), Some(Bundled::HighContrastLight));
        for b in Bundled::ALL {
            assert_eq!(Bundled::from_icls_parent(b.icls_parent()).dark(), b.dark(), "{b:?}");
        }
    }

    #[test]
    fn styles_effects_with_their_color_or_the_text_color() {
        let a = TextAttributes {
            foreground: Some("#111111".into()),
            effect: Some(Effect::Bordered),
            ..Default::default()
        };
        assert_eq!(
            a.style("#ffffff").box_shadow.as_deref(),
            Some("inset 0 0 0 1px #111111")
        );
        let b = TextAttributes {
            effect: Some(Effect::Strikethrough),
            effect_color: Some("#ff0000".into()),
            ..Default::default()
        };
        assert_eq!(
            b.style("#ffffff").text_decoration.as_deref(),
            Some("line-through #ff0000")
        );
        let vars = b.css_vars(Target::Syntax("keyword"), "#eeeeee");
        assert_eq!(vars[0], ("--syn-keyword".into(), "#eeeeee".into()));
        assert_eq!(
            vars[4],
            ("--syn-keyword-deco".into(), "line-through #ff0000".into())
        );
        let text = TextAttributes::fg("#abcdef");
        assert_eq!(
            text.css_vars(
                Target::Editor {
                    fg: Some("--editor-fg"),
                    bg: Some("--bg-editor")
                },
                ""
            ),
            vec![("--editor-fg".to_string(), "#abcdef".to_string())]
        );
    }
}
