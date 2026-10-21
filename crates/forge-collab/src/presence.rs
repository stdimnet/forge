//! Participants and their colors.
//!
//! Each participant gets one of eight colors (`--collab-1` … `--collab-8` in the theme) in
//! join order; the host is always color 1. Colors are assigned by the host, which also
//! stamps them into carets (awareness), so a guest cannot take someone else's color or name.

use serde::{Deserialize, Serialize};

use crate::permissions::Permissions;

/// Number of participant colors in the theme.
pub const COLORS: u8 = 8;

/// A participant's id within one session. The host is always [`ParticipantId::HOST`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParticipantId(pub u32);

impl ParticipantId {
    pub const HOST: Self = Self(0);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Host,
    Guest,
}

/// A participant as everyone sees them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Participant {
    pub id: ParticipantId,
    pub name: String,
    pub role: Role,
    /// 1-based index into the theme's participant colors.
    pub color: u8,
    pub permissions: Permissions,
}

/// Hands out colors in join order: the host gets 1, guests cycle through 2 to 8.
#[derive(Debug, Clone, Default)]
pub struct ColorAssigner {
    guests: u32,
}

impl ColorAssigner {
    pub fn host() -> u8 {
        1
    }

    pub fn next_guest(&mut self) -> u8 {
        let color = 2 + (self.guests % u32::from(COLORS - 1)) as u8;
        self.guests += 1;
        color
    }
}

/// CSS color of a participant for carets (`var(--collab-N)`).
pub fn caret_color(color: u8) -> String {
    format!("var(--collab-{})", color.clamp(1, COLORS))
}

/// CSS color of a participant's selections: the caret color at 25 % opacity.
pub fn selection_color(color: u8) -> String {
    format!(
        "color-mix(in srgb, var(--collab-{}) 25%, transparent)",
        color.clamp(1, COLORS)
    )
}

/// A participant name as shown next to carets: trimmed, without control characters and at
/// most 64 characters. Empty names become "Guest".
pub fn clean_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect::<String>()
        .trim()
        .to_string();
    if cleaned.is_empty() {
        "Guest".into()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_is_one_and_guests_cycle_through_the_rest() {
        assert_eq!(ColorAssigner::host(), 1);
        let mut c = ColorAssigner::default();
        let colors: Vec<u8> = (0..9).map(|_| c.next_guest()).collect();
        assert_eq!(colors, vec![2, 3, 4, 5, 6, 7, 8, 2, 3]);
    }

    #[test]
    fn css_colors_use_theme_tokens() {
        assert_eq!(caret_color(3), "var(--collab-3)");
        assert_eq!(caret_color(42), "var(--collab-8)");
        assert!(selection_color(2).contains("var(--collab-2) 25%"));
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Alex \n"), "Alex");
        assert_eq!(clean_name("\u{7}"), "Guest");
        assert_eq!(clean_name(&"x".repeat(100)).len(), 64);
    }
}
