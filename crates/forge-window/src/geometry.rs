//! Where dialog windows open and the geometry Forge remembers for each dialog.
//!
//! All values are physical pixels in desktop coordinates, as the windowing system reports
//! them for outer window positions, sizes and monitor work areas.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn right(&self) -> i64 {
        self.x as i64 + self.width as i64
    }

    fn bottom(&self) -> i64 {
        self.y as i64 + self.height as i64
    }

    fn center(&self) -> (i64, i64) {
        (
            self.x as i64 + self.width as i64 / 2,
            self.y as i64 + self.height as i64 / 2,
        )
    }

    fn contains_point(&self, (px, py): (i64, i64)) -> bool {
        px >= self.x as i64 && px < self.right() && py >= self.y as i64 && py < self.bottom()
    }

    /// Area of the intersection with `other`.
    fn overlap(&self, other: &Rect) -> i64 {
        let w = self.right().min(other.right()) - (self.x as i64).max(other.x as i64);
        let h = self.bottom().min(other.bottom()) - (self.y as i64).max(other.y as i64);
        if w <= 0 || h <= 0 {
            0
        } else {
            w * h
        }
    }

    /// `size` centered on this rectangle.
    pub fn center_of(&self, size: Size) -> Rect {
        let (cx, cy) = self.center();
        Rect::new(
            (cx - size.width as i64 / 2) as i32,
            (cy - size.height as i64 / 2) as i32,
            size.width,
            size.height,
        )
    }

    /// Shrink to fit into `area` and move inside it.
    pub fn clamp_into(&self, area: &Rect) -> Rect {
        let width = self.width.min(area.width);
        let height = self.height.min(area.height);
        let max_x = area.right() - width as i64;
        let max_y = area.bottom() - height as i64;
        let x = (self.x as i64).clamp(area.x as i64, max_x.max(area.x as i64));
        let y = (self.y as i64).clamp(area.y as i64, max_y.max(area.y as i64));
        Rect::new(x as i32, y as i32, width, height)
    }
}

/// Height of the strip at the top of a window that must stay on screen so it can be dragged.
const TITLE_STRIP: u32 = 32;

/// Geometry of a dialog window that opens over `parent`.
///
/// - The size is the remembered one, else `default`; never smaller than `min`.
/// - A remembered position is kept when the dialog's title strip is still on a monitor
///   (monitors may have changed since); otherwise the dialog is centered on its parent.
/// - The result is moved (and if needed shrunk) into the monitor it is mostly on.
pub fn place_dialog(
    parent: Rect,
    default: Size,
    min: Size,
    remembered: Option<Rect>,
    monitors: &[Rect],
) -> Rect {
    let size = remembered.map_or(default, |r| Size {
        width: r.width,
        height: r.height,
    });
    let size = Size {
        width: size.width.max(min.width),
        height: size.height.max(min.height),
    };
    let rect = match remembered {
        Some(r) if title_visible(&Rect::new(r.x, r.y, size.width, size.height), monitors) => {
            Rect::new(r.x, r.y, size.width, size.height)
        }
        _ => parent.center_of(size),
    };
    match monitor_for(&rect, &parent, monitors) {
        Some(area) => rect.clamp_into(&area),
        None => rect,
    }
}

fn title_visible(rect: &Rect, monitors: &[Rect]) -> bool {
    let strip = Rect::new(rect.x, rect.y, rect.width, TITLE_STRIP.min(rect.height));
    let needed = (strip.width as i64 * strip.height as i64) / 3;
    monitors.iter().map(|m| strip.overlap(m)).sum::<i64>() >= needed.max(1)
}

/// The monitor `rect` is mostly on, else the one under the parent's center.
fn monitor_for(rect: &Rect, parent: &Rect, monitors: &[Rect]) -> Option<Rect> {
    let best = monitors
        .iter()
        .max_by_key(|m| rect.overlap(m))
        .filter(|m| rect.overlap(m) > 0);
    best.or_else(|| monitors.iter().find(|m| m.contains_point(parent.center())))
        .or_else(|| monitors.first())
        .copied()
}

/// Remembered geometry of each dialog, keyed by dialog id. Stored as JSON by the shell.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogStates {
    #[serde(default)]
    pub dialogs: BTreeMap<String, Rect>,
}

impl DialogStates {
    /// Parse the stored JSON; damaged or foreign content starts over empty.
    pub fn from_json(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn get(&self, id: &str) -> Option<Rect> {
        self.dialogs.get(id).copied()
    }

    /// Remember a dialog's geometry; returns whether it changed. Degenerate sizes (a window
    /// that was minimized or not laid out yet) are ignored.
    pub fn remember(&mut self, id: &str, rect: Rect) -> bool {
        if rect.width < 100 || rect.height < 60 {
            return false;
        }
        self.dialogs.insert(id.to_string(), rect) != Some(rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };
    const PARENT: Rect = Rect {
        x: 240,
        y: 90,
        width: 1440,
        height: 900,
    };
    const DEFAULT: Size = Size {
        width: 900,
        height: 640,
    };
    const MIN: Size = Size {
        width: 500,
        height: 360,
    };

    #[test]
    fn centers_on_the_parent_by_default() {
        let r = place_dialog(PARENT, DEFAULT, MIN, None, &[SCREEN]);
        assert_eq!(r, Rect::new(510, 220, 900, 640));
    }

    #[test]
    fn keeps_a_remembered_geometry() {
        let saved = Rect::new(100, 50, 1000, 700);
        assert_eq!(
            place_dialog(PARENT, DEFAULT, MIN, Some(saved), &[SCREEN]),
            saved
        );
    }

    #[test]
    fn remembered_size_never_drops_below_the_minimum() {
        let saved = Rect::new(100, 50, 200, 100);
        assert_eq!(
            place_dialog(PARENT, DEFAULT, MIN, Some(saved), &[SCREEN]),
            Rect::new(100, 50, 500, 360)
        );
    }

    #[test]
    fn recenters_when_the_remembered_monitor_is_gone() {
        // Saved on a second monitor to the right that is no longer connected.
        let saved = Rect::new(2400, 200, 800, 600);
        let r = place_dialog(PARENT, DEFAULT, MIN, Some(saved), &[SCREEN]);
        assert_eq!(
            r,
            PARENT.center_of(Size {
                width: 800,
                height: 600
            })
        );
    }

    #[test]
    fn uses_the_second_monitor_when_present() {
        let second = Rect::new(1920, 0, 2560, 1440);
        let saved = Rect::new(2400, 200, 800, 600);
        assert_eq!(
            place_dialog(PARENT, DEFAULT, MIN, Some(saved), &[SCREEN, second]),
            saved
        );
    }

    #[test]
    fn moves_partly_offscreen_dialogs_inside() {
        // Title strip visible, but the bottom hangs off the screen.
        let saved = Rect::new(1500, 700, 900, 640);
        let r = place_dialog(PARENT, DEFAULT, MIN, Some(saved), &[SCREEN]);
        assert_eq!(r, Rect::new(1020, 440, 900, 640));
    }

    #[test]
    fn shrinks_into_small_screens() {
        let small = Rect::new(0, 0, 800, 600);
        let parent = Rect::new(0, 0, 800, 600);
        let r = place_dialog(parent, DEFAULT, MIN, None, &[small]);
        assert_eq!(r, Rect::new(0, 0, 800, 600));
    }

    #[test]
    fn parent_on_a_monitor_with_negative_coordinates() {
        let left = Rect::new(-1920, 0, 1920, 1080);
        let parent = Rect::new(-1700, 100, 1200, 800);
        let r = place_dialog(parent, DEFAULT, MIN, None, &[SCREEN, left]);
        assert_eq!(r, Rect::new(-1550, 180, 900, 640));
    }

    #[test]
    fn works_without_monitor_information() {
        let r = place_dialog(PARENT, DEFAULT, MIN, None, &[]);
        assert_eq!(r, Rect::new(510, 220, 900, 640));
    }

    #[test]
    fn dialog_states_round_trip_and_ignore_damage() {
        let mut s = DialogStates::default();
        assert!(s.remember("settings", Rect::new(10, 20, 900, 640)));
        assert!(!s.remember("settings", Rect::new(10, 20, 900, 640)));
        assert!(
            !s.remember("settings", Rect::new(0, 0, 0, 0)),
            "minimized sizes are skipped"
        );
        let json = s.to_json();
        let back = DialogStates::from_json(&json);
        assert_eq!(back.get("settings"), Some(Rect::new(10, 20, 900, 640)));
        assert_eq!(back.get("other"), None);
        assert_eq!(DialogStates::from_json("not json"), DialogStates::default());
        assert_eq!(DialogStates::from_json("{}"), DialogStates::default());
    }
}
