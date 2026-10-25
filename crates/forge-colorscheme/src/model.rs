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

/// The two bundled schemes, matching the theme CSS (`app/ui/src/theme/theme.css`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bundled {
    Ember,
    Anvil,
}

impl Bundled {
    pub const ALL: [Bundled; 2] = [Bundled::Ember, Bundled::Anvil];

    pub fn name(self) -> &'static str {
        match self {
            Self::Ember => "Ember",
            Self::Anvil => "Anvil",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.name() == s)
    }

    pub fn dark(self) -> bool {
        self == Self::Ember
    }

    /// PHPStorm's scheme of the same kind (the `parent_scheme` of exported files).
    pub fn icls_parent(self) -> &'static str {
        match self {
            Self::Ember => "Darcula",
            Self::Anvil => "Default",
        }
    }

    /// The bundled scheme an IntelliJ parent maps onto.
    pub fn from_icls_parent(parent: &str) -> Self {
        let p = parent.to_ascii_lowercase();
        if p.contains("darcula") || p.contains("dark") || p.contains("ember") {
            Self::Ember
        } else {
            Self::Anvil
        }
    }

    /// The attributes of the scheme (every key of [`ATTRIBUTES`]).
    pub fn attributes(self) -> Vec<(&'static str, TextAttributes)> {
        let dark = self.dark();
        let pick = |d: &str, l: &str| if dark { d.to_string() } else { l.to_string() };
        let fg = |d: &str, l: &str| TextAttributes {
            foreground: Some(pick(d, l)),
            ..TextAttributes::default()
        };
        vec![
            (
                "TEXT",
                TextAttributes {
                    foreground: Some(pick("#d9d9df", "#26272b")),
                    background: Some(pick("#141417", "#ffffff")),
                    ..TextAttributes::default()
                },
            ),
            ("CARET", fg("#ff7a1a", "#e8620c")),
            ("CARET_ROW", TextAttributes::bg(&pick("#1e1e23", "#f1f2f6"))),
            ("SELECTION", TextAttributes::bg(&pick("#3a2a1e", "#ffe3cc"))),
            ("LINE_NUMBERS", fg("#5f5f6a", "#9a9da6")),
            (
                "SEARCH_RESULT",
                TextAttributes::bg(&pick("#483c24", "#f8ecd2")),
            ),
            ("KEYWORD", fg("#ff9a57", "#b8460b")),
            ("STRING", fg("#a5d17f", "#3c7d24")),
            ("NUMBER", fg("#7cc5e8", "#1b6fa0")),
            ("COMMENT", fg("#6b6b78", "#8c8f99").italic()),
            ("VARIABLE", fg("#c9a6ff", "#7a3fc4")),
            ("FUNCTION", fg("#ffd479", "#8a5a00")),
            ("TYPE", fg("#6fd2c4", "#157a6e")),
            ("ATTRIBUTE", fg("#d0c070", "#806a00")),
            ("OPERATOR", fg("#c4c4cc", "#3a3c42")),
            ("TAG", fg("#ff9a57", "#b8460b")),
        ]
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
