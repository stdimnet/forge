//! Participants, their colors and where they are (presence).
//!
//! Each participant gets one of eight colors (`--collab-1` … `--collab-8` in the theme) in
//! join order; the host is always color 1. Colors are assigned by the host, which also
//! stamps them into carets (awareness), so a guest cannot take someone else's color or name.
//!
//! Presence is session-wide: the file a participant looks at, its caret and the lines in
//! view, and whom it follows. Follow, Jump To and Make Everyone Follow You are built on it.
//! Presence goes through the host, which passes each participant only what the participant
//! may see ([`Presence::visible_to`]): a guest never learns the name of a file it cannot open.

use serde::{Deserialize, Serialize};

use crate::permissions::{Permissions, Policy};

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
    /// The guest's connection dropped; the host keeps its place while it reconnects.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reconnecting: bool,
}

/// A 1-based caret position (line, and column in UTF-16 units like the editor counts them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caret {
    pub line: u32,
    pub column: u32,
}

/// The 1-based first and last line in view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewport {
    pub top_line: u32,
    pub bottom_line: u32,
}

/// Where a participant is. All fields are optional: a participant without an open project
/// file has no `file`, and older peers send nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Presence {
    /// Workspace-relative path of the file in the participant's active editor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caret: Option<Caret>,
    /// The selection's other end, when text is selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Caret>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Viewport>,
    /// The participant this one follows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub following: Option<ParticipantId>,
}

/// Longest path a presence may name.
const MAX_PRESENCE_PATH: usize = 4096;

impl Presence {
    /// What a participant with `policy` may see of this presence: the file and the position
    /// in it only when the participant may open the file (the host's own presence in an
    /// excluded file shows as "no file"). `None` policy: the host, which sees everything.
    pub fn visible_to(&self, policy: Option<&Policy>) -> Presence {
        let visible = match (&self.file, policy) {
            (Some(file), Some(policy)) => policy.check_path(file).is_ok(),
            (Some(_), None) => true,
            (None, _) => false,
        };
        if visible {
            self.clone()
        } else {
            Presence {
                following: self.following,
                ..Presence::default()
            }
        }
    }

    /// A presence as a participant sent it, made safe to pass on: positions are kept only
    /// with a plain workspace-relative file, the viewport is ordered, and `following` must
    /// name another participant of `known`.
    pub fn sanitized(mut self, me: ParticipantId, known: &[ParticipantId]) -> Presence {
        let plain = self.file.as_deref().is_some_and(|f| {
            f.len() <= MAX_PRESENCE_PATH && crate::permissions::is_plain_relative(f)
        });
        if !plain {
            self.file = None;
        }
        if self.file.is_none() {
            self.caret = None;
            self.anchor = None;
            self.viewport = None;
        }
        if let Some(v) = &mut self.viewport {
            if v.bottom_line < v.top_line {
                std::mem::swap(&mut v.top_line, &mut v.bottom_line);
            }
        }
        if self
            .following
            .is_some_and(|f| f == me || !known.contains(&f))
        {
            self.following = None;
        }
        self
    }
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

    fn at(file: &str) -> Presence {
        Presence {
            file: Some(file.into()),
            caret: Some(Caret {
                line: 12,
                column: 4,
            }),
            anchor: None,
            viewport: Some(Viewport {
                top_line: 1,
                bottom_line: 40,
            }),
            following: Some(ParticipantId(2)),
        }
    }

    #[test]
    fn presence_hides_files_the_viewer_cannot_open() {
        let policy = Policy::new(
            Permissions::READ_ONLY,
            crate::excludes::Excludes::defaults(),
        );
        let visible = at("app/Models/User.php");
        assert_eq!(visible.visible_to(Some(&policy)), visible);
        for hidden in [".env", ".git/config", "/etc/passwd", "../outside.php"] {
            let p = at(hidden).visible_to(Some(&policy));
            assert_eq!(p.file, None, "{hidden}");
            assert_eq!(p.caret, None);
            assert_eq!(p.viewport, None);
            // Whom someone follows is not secret.
            assert_eq!(p.following, Some(ParticipantId(2)));
        }
        // The host sees everything.
        assert_eq!(at(".env").visible_to(None), at(".env"));
    }

    #[test]
    fn sent_presence_is_sanitized() {
        let known = [ParticipantId::HOST, ParticipantId(1), ParticipantId(2)];
        let p = at("src/a.php").sanitized(ParticipantId(1), &known);
        assert_eq!(p, at("src/a.php"));
        // Following oneself or someone unknown is dropped.
        assert_eq!(
            at("a.php").sanitized(ParticipantId(2), &known).following,
            None
        );
        assert_eq!(
            at("a.php")
                .sanitized(ParticipantId(1), &[ParticipantId(1)])
                .following,
            None
        );
        // Positions without a usable file are dropped.
        let p = at("/abs/path.php").sanitized(ParticipantId(1), &known);
        assert_eq!((p.file, p.caret, p.viewport), (None, None, None));
        let mut p = at("a.php");
        p.viewport = Some(Viewport {
            top_line: 30,
            bottom_line: 3,
        });
        let v = p.sanitized(ParticipantId(1), &known).viewport.unwrap();
        assert_eq!((v.top_line, v.bottom_line), (3, 30));
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Alex \n"), "Alex");
        assert_eq!(clean_name("\u{7}"), "Guest");
        assert_eq!(clean_name(&"x".repeat(100)).len(), 64);
    }
}
