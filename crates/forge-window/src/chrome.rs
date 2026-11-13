//! Title bar style and button layout for `appearance.windowControls`.

use serde::{Deserialize, Serialize};

use crate::desktop::{Desktop, Os};

/// How Forge draws window controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlsStyle {
    /// Windows 11: minimize / maximize / close rectangles, red close hover.
    Windows,
    /// macOS traffic lights with hover glyphs.
    Macos,
    /// GNOME / Adwaita: round buttons.
    Gnome,
    /// KDE Plasma / Breeze: plain glyphs with round hover.
    Kde,
    /// Generic Linux (Xfce, Cinnamon, tiling window managers, …).
    Linux,
}

impl ControlsStyle {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "windows" => ControlsStyle::Windows,
            "macos" => ControlsStyle::Macos,
            "gnome" => ControlsStyle::Gnome,
            "kde" => ControlsStyle::Kde,
            "linux" => ControlsStyle::Linux,
            _ => return None,
        })
    }

    /// The style that matches the platform.
    pub fn auto(os: Os, desktop: Desktop) -> Self {
        match (os, desktop) {
            (Os::Windows, _) | (_, Desktop::Windows) => ControlsStyle::Windows,
            (Os::Macos, _) | (_, Desktop::Macos) => ControlsStyle::Macos,
            (_, Desktop::Gnome) => ControlsStyle::Gnome,
            (_, Desktop::Kde) => ControlsStyle::Kde,
            _ => ControlsStyle::Linux,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Button {
    Minimize,
    Maximize,
    Close,
}

/// Buttons at the left and right end of the title bar, in display order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ButtonLayout {
    pub left: Vec<Button>,
    pub right: Vec<Button>,
}

impl ButtonLayout {
    fn new(left: &[Button], right: &[Button]) -> Self {
        Self {
            left: left.to_vec(),
            right: right.to_vec(),
        }
    }

    /// The layout a style uses when the desktop does not say otherwise.
    pub fn default_for(style: ControlsStyle) -> Self {
        use Button::*;
        match style {
            ControlsStyle::Macos => Self::new(&[Close, Minimize, Maximize], &[]),
            // GNOME's default `button-layout` is `appmenu:close`.
            ControlsStyle::Gnome => Self::new(&[], &[Close]),
            ControlsStyle::Windows | ControlsStyle::Kde | ControlsStyle::Linux => {
                Self::new(&[], &[Minimize, Maximize, Close])
            }
        }
    }

    /// GNOME's `org.gnome.desktop.wm.preferences button-layout`, as `gsettings get` prints it
    /// (`'appmenu:minimize,maximize,close'`). Names other than the three buttons (`appmenu`,
    /// `icon`, `menu`, `spacer`) are skipped. `None` when the value has no buttons at all and
    /// no `:` (unparsable output).
    pub fn parse_gnome(value: &str) -> Option<Self> {
        let value = value.trim().trim_matches(|c| c == '\'' || c == '"').trim();
        let (left, right) = match value.split_once(':') {
            Some((l, r)) => (l, r),
            None => (value, ""),
        };
        let names = |side: &str| -> Vec<Button> {
            let mut out = Vec::new();
            for name in side.split(',') {
                let button = match name.trim() {
                    "close" => Button::Close,
                    "minimize" => Button::Minimize,
                    "maximize" => Button::Maximize,
                    _ => continue,
                };
                if !out.contains(&button) {
                    out.push(button);
                }
            }
            out
        };
        let layout = Self {
            left: names(left),
            right: names(right),
        };
        let right_left: Vec<_> = layout
            .right
            .iter()
            .filter(|b| !layout.left.contains(b))
            .copied()
            .collect();
        let layout = Self {
            left: layout.left,
            right: right_left,
        };
        (!layout.is_empty() || value.contains(':')).then_some(layout)
    }

    /// KWin's button letters (`ButtonsOnLeft=MS`, `ButtonsOnRight=HIAX`): `I` minimize,
    /// `A` maximize, `X` close; the others (menu, help, shade, …) are skipped.
    pub fn parse_kde(left: &str, right: &str) -> Self {
        let letters = |s: &str| -> Vec<Button> {
            let mut out = Vec::new();
            for c in s.chars() {
                let button = match c {
                    'I' => Button::Minimize,
                    'A' => Button::Maximize,
                    'X' => Button::Close,
                    _ => continue,
                };
                if !out.contains(&button) {
                    out.push(button);
                }
            }
            out
        };
        let left = letters(left);
        let right = letters(right)
            .into_iter()
            .filter(|b| !left.contains(b))
            .collect();
        Self { left, right }
    }

    /// The button order from `~/.config/kwinrc` (`[org.kde.kdecoration2]`), or `None` when
    /// the file does not set it (Plasma then uses its defaults).
    pub fn from_kwinrc(text: &str) -> Option<Self> {
        let mut in_section = false;
        let (mut left, mut right) = (None, None);
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_section = line == "[org.kde.kdecoration2]";
                continue;
            }
            if !in_section {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                match key.trim() {
                    "ButtonsOnLeft" => left = Some(value.trim().to_string()),
                    "ButtonsOnRight" => right = Some(value.trim().to_string()),
                    _ => {}
                }
            }
        }
        if left.is_none() && right.is_none() {
            return None;
        }
        // A missing key keeps Breeze's default for that side.
        Some(Self::parse_kde(
            left.as_deref().unwrap_or("MS"),
            right.as_deref().unwrap_or("HIAX"),
        ))
    }

    pub fn is_empty(&self) -> bool {
        self.left.is_empty() && self.right.is_empty()
    }

    pub fn contains(&self, button: Button) -> bool {
        self.left.contains(&button) || self.right.contains(&button)
    }

    /// The buttons of a dialog window: only close, on the side the main window has it (like
    /// PHPStorm's dialogs). macOS keeps all three traffic lights (minimize is shown disabled).
    pub fn for_dialog(&self, style: ControlsStyle) -> Self {
        if style == ControlsStyle::Macos {
            return Self::default_for(ControlsStyle::Macos);
        }
        let keep = |side: &[Button]| {
            side.iter()
                .filter(|b| **b == Button::Close)
                .copied()
                .collect::<Vec<_>>()
        };
        let layout = Self {
            left: keep(&self.left),
            right: keep(&self.right),
        };
        if layout.is_empty() {
            Self::new(&[], &[Button::Close])
        } else {
            layout
        }
    }
}

/// Button layouts configured on this system, read by the shell.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemLayouts {
    /// From `gsettings get org.gnome.desktop.wm.preferences button-layout`.
    #[serde(default)]
    pub gnome: Option<ButtonLayout>,
    /// From `kwinrc`.
    #[serde(default)]
    pub kde: Option<ButtonLayout>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TitleAlign {
    Left,
    Center,
}

/// Everything the UI needs to draw a title bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowChrome {
    pub os: Os,
    pub desktop: Desktop,
    /// The setting value the chrome was resolved from (`auto` for unknown values).
    pub setting: String,
    /// Look of the controls. With `native` it is still set, for in-page dialogs in browser mode.
    pub style: ControlsStyle,
    /// The OS draws the title bar (`decorations: true`); Forge draws no controls.
    pub native: bool,
    /// Buttons of the main window.
    pub buttons: ButtonLayout,
    /// Buttons of dialog windows.
    pub dialog_buttons: ButtonLayout,
    /// Where dialog titles go.
    pub title_align: TitleAlign,
}

/// Resolve the setting `appearance.windowControls` for this platform.
pub fn resolve(setting: &str, os: Os, desktop: Desktop, system: &SystemLayouts) -> WindowChrome {
    let setting = match setting.trim() {
        s @ ("windows" | "macos" | "gnome" | "kde" | "linux" | "native") => s.to_string(),
        _ => "auto".to_string(),
    };
    let native = setting == "native";
    let style = ControlsStyle::parse(&setting).unwrap_or_else(|| ControlsStyle::auto(os, desktop));
    let system_layout = match style {
        ControlsStyle::Gnome => system.gnome.clone(),
        ControlsStyle::Kde => system.kde.clone(),
        _ => None,
    };
    let buttons = system_layout.unwrap_or_else(|| ButtonLayout::default_for(style));
    let dialog_buttons = buttons.for_dialog(style);
    let title_align = match style {
        ControlsStyle::Windows => TitleAlign::Left,
        _ => TitleAlign::Center,
    };
    WindowChrome {
        os,
        desktop,
        setting,
        style,
        native,
        buttons,
        dialog_buttons,
        title_align,
    }
}

#[cfg(test)]
mod tests {
    use super::Button::*;
    use super::*;

    #[test]
    fn auto_style_follows_the_platform() {
        assert_eq!(
            ControlsStyle::auto(Os::Windows, Desktop::Windows),
            ControlsStyle::Windows
        );
        assert_eq!(
            ControlsStyle::auto(Os::Macos, Desktop::Macos),
            ControlsStyle::Macos
        );
        assert_eq!(
            ControlsStyle::auto(Os::Linux, Desktop::Gnome),
            ControlsStyle::Gnome
        );
        assert_eq!(
            ControlsStyle::auto(Os::Linux, Desktop::Kde),
            ControlsStyle::Kde
        );
        assert_eq!(
            ControlsStyle::auto(Os::Linux, Desktop::Other),
            ControlsStyle::Linux
        );
    }

    #[test]
    fn parses_gnome_button_layouts() {
        let l = ButtonLayout::parse_gnome("'appmenu:close'\n").unwrap();
        assert_eq!(l, ButtonLayout::new(&[], &[Close]));
        let l = ButtonLayout::parse_gnome("'appmenu:minimize,maximize,close'").unwrap();
        assert_eq!(l, ButtonLayout::new(&[], &[Minimize, Maximize, Close]));
        let l = ButtonLayout::parse_gnome("close,minimize,maximize:appmenu").unwrap();
        assert_eq!(l, ButtonLayout::new(&[Close, Minimize, Maximize], &[]));
        let l = ButtonLayout::parse_gnome("'icon:minimize,spacer,close,close'").unwrap();
        assert_eq!(l, ButtonLayout::new(&[], &[Minimize, Close]));
        // No buttons at all is a valid choice.
        assert_eq!(
            ButtonLayout::parse_gnome("'appmenu:'"),
            Some(ButtonLayout::default())
        );
        // Not a layout (e.g. an error message on stdout).
        assert_eq!(ButtonLayout::parse_gnome("No such schema"), None);
        assert_eq!(ButtonLayout::parse_gnome(""), None);
    }

    #[test]
    fn parses_kwin_button_letters() {
        assert_eq!(
            ButtonLayout::parse_kde("MS", "HIAX"),
            ButtonLayout::new(&[], &[Minimize, Maximize, Close])
        );
        assert_eq!(
            ButtonLayout::parse_kde("XIA", "M"),
            ButtonLayout::new(&[Close, Minimize, Maximize], &[])
        );
        assert_eq!(
            ButtonLayout::parse_kde("X", "X"),
            ButtonLayout::new(&[Close], &[])
        );
    }

    #[test]
    fn reads_kwinrc() {
        let rc = "[Windows]\nButtonsOnRight=X\n\n[org.kde.kdecoration2]\nBorderSize=Normal\nButtonsOnLeft=XAI\nButtonsOnRight=\nlibrary=org.kde.breeze\n";
        assert_eq!(
            ButtonLayout::from_kwinrc(rc),
            Some(ButtonLayout::new(&[Close, Maximize, Minimize], &[]))
        );
        let rc = "[org.kde.kdecoration2]\nButtonsOnRight=IX\n";
        assert_eq!(
            ButtonLayout::from_kwinrc(rc),
            Some(ButtonLayout::new(&[], &[Minimize, Close]))
        );
        assert_eq!(
            ButtonLayout::from_kwinrc("[org.kde.kdecoration2]\nlibrary=x\n"),
            None
        );
        assert_eq!(ButtonLayout::from_kwinrc(""), None);
    }

    #[test]
    fn dialogs_keep_only_close() {
        let main = ButtonLayout::new(&[Close], &[Minimize, Maximize]);
        assert_eq!(
            main.for_dialog(ControlsStyle::Gnome),
            ButtonLayout::new(&[Close], &[])
        );
        let no_close = ButtonLayout::new(&[], &[Minimize]);
        assert_eq!(
            no_close.for_dialog(ControlsStyle::Linux),
            ButtonLayout::new(&[], &[Close])
        );
        let mac = ButtonLayout::default_for(ControlsStyle::Macos);
        assert_eq!(mac.for_dialog(ControlsStyle::Macos), mac);
    }

    #[test]
    fn resolves_auto_with_system_layouts() {
        let system = SystemLayouts {
            gnome: ButtonLayout::parse_gnome("'appmenu:minimize,close'"),
            kde: ButtonLayout::from_kwinrc(
                "[org.kde.kdecoration2]\nButtonsOnLeft=X\nButtonsOnRight=IA",
            ),
        };
        let gnome = resolve("auto", Os::Linux, Desktop::Gnome, &system);
        assert_eq!(gnome.style, ControlsStyle::Gnome);
        assert!(!gnome.native);
        assert_eq!(gnome.buttons, ButtonLayout::new(&[], &[Minimize, Close]));
        assert_eq!(gnome.dialog_buttons, ButtonLayout::new(&[], &[Close]));
        assert_eq!(gnome.title_align, TitleAlign::Center);

        let kde = resolve("auto", Os::Linux, Desktop::Kde, &system);
        assert_eq!(kde.style, ControlsStyle::Kde);
        assert_eq!(
            kde.buttons,
            ButtonLayout::new(&[Close], &[Minimize, Maximize])
        );
        assert_eq!(kde.dialog_buttons, ButtonLayout::new(&[Close], &[]));

        let win = resolve(
            "auto",
            Os::Windows,
            Desktop::Windows,
            &SystemLayouts::default(),
        );
        assert_eq!(win.style, ControlsStyle::Windows);
        assert_eq!(win.title_align, TitleAlign::Left);
        assert_eq!(
            win.buttons,
            ButtonLayout::new(&[], &[Minimize, Maximize, Close])
        );

        let other = resolve("auto", Os::Linux, Desktop::Other, &system);
        assert_eq!(other.style, ControlsStyle::Linux);
        assert_eq!(
            other.buttons,
            ButtonLayout::default_for(ControlsStyle::Linux)
        );
    }

    #[test]
    fn forced_styles_and_native() {
        let system = SystemLayouts {
            gnome: ButtonLayout::parse_gnome("'close:'"),
            kde: None,
        };
        let mac = resolve("macos", Os::Linux, Desktop::Gnome, &system);
        assert_eq!(mac.style, ControlsStyle::Macos);
        assert_eq!(
            mac.buttons,
            ButtonLayout::new(&[Close, Minimize, Maximize], &[])
        );
        // A forced GNOME look still follows the system's GNOME layout.
        let gnome = resolve("gnome", Os::Linux, Desktop::Kde, &system);
        assert_eq!(gnome.buttons, ButtonLayout::new(&[Close], &[]));
        // KDE forced on GNOME without kwinrc: Breeze defaults.
        let kde = resolve("kde", Os::Linux, Desktop::Gnome, &system);
        assert_eq!(kde.buttons, ButtonLayout::default_for(ControlsStyle::Kde));

        let native = resolve("native", Os::Linux, Desktop::Kde, &system);
        assert!(native.native);
        assert_eq!(native.style, ControlsStyle::Kde);
        assert_eq!(native.setting, "native");

        let unknown = resolve("bogus", Os::Macos, Desktop::Macos, &system);
        assert_eq!(unknown.setting, "auto");
        assert_eq!(unknown.style, ControlsStyle::Macos);
    }

    #[test]
    fn serializes_for_the_ui() {
        let chrome = resolve("auto", Os::Linux, Desktop::Gnome, &SystemLayouts::default());
        let json = serde_json::to_value(&chrome).unwrap();
        assert_eq!(json["style"], "gnome");
        assert_eq!(json["dialogButtons"]["right"][0], "close");
        assert_eq!(json["titleAlign"], "center");
    }
}
