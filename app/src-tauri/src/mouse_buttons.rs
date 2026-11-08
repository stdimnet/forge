//! Mouse side buttons (back / forward, PHPStorm's `Button4` / `Button5`) for webviews that hide
//! them from the page.
//!
//! WebKitGTK reports the side buttons (GDK buttons 8 and 9) to the page as a button-less
//! `mousedown` / `mouseup` (`button` 0, `which` 0), so the keymap cannot tell them from a left
//! click or from each other. On Linux this plugin takes them from the webview's GTK widget
//! instead and sends [`SIDE_BUTTON_EVENT`] to that webview; the UI resolves the click through
//! the keymap like any other mouse shortcut (`app/ui/src/keymap/dispatch.ts`). WebView2
//! (Windows) reports them to the page as DOM buttons 3 and 4, which the UI handles directly.

use serde::Serialize;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::Runtime;

/// Sent to the webview that received the click.
pub const SIDE_BUTTON_EVENT: &str = "keymap://side-button";

/// A side button click in DOM terms, the shape of the keymap's mouse input.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SideButtonClick {
    /// DOM button number: 3 back, 4 forward.
    pub button: u8,
    pub ctrl_key: bool,
    pub alt_key: bool,
    pub shift_key: bool,
    pub meta_key: bool,
    /// Position in the webview (CSS pixels at 100 % zoom), for the focus context.
    pub x: f64,
    pub y: f64,
}

/// The DOM button of an X11 / GDK button number, if it is a side button.
pub fn dom_side_button(gdk_button: u32) -> Option<u8> {
    match gdk_button {
        8 => Some(3),
        9 => Some(4),
        _ => None,
    }
}

/// Modifier state as GDK reports it, independent of the gtk crate (testable everywhere).
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

/// The click to send for a GDK button release, or `None` for other buttons.
pub fn side_button_click(
    gdk_button: u32,
    modifiers: Modifiers,
    (x, y): (f64, f64),
) -> Option<SideButtonClick> {
    Some(SideButtonClick {
        button: dom_side_button(gdk_button)?,
        ctrl_key: modifiers.control,
        alt_key: modifiers.alt,
        shift_key: modifiers.shift,
        meta_key: modifiers.meta,
        x,
        y,
    })
}

/// Installs the side button hook on every webview Forge creates.
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("forge-mouse-buttons")
        .on_webview_ready(|webview| {
            #[cfg(target_os = "linux")]
            linux::install(webview);
            #[cfg(not(target_os = "linux"))]
            let _ = webview;
        })
        .build()
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{side_button_click, Modifiers, SIDE_BUTTON_EVENT};
    use gtk::gdk::ModifierType;
    use gtk::glib::Propagation;
    use gtk::prelude::WidgetExt;
    use tauri::{Emitter, EventTarget, Runtime, Webview};

    pub(super) fn install<R: Runtime>(webview: Webview<R>) {
        let target = webview.clone();
        let result = webview.with_webview(move |platform| {
            let widget = platform.inner();
            // Keep both halves of the click from WebKit: its button-less mousedown would
            // otherwise reach the page like a left button press (selection, drag).
            widget.connect_button_press_event(|_, e| {
                if super::dom_side_button(e.button()).is_some() {
                    Propagation::Stop
                } else {
                    Propagation::Proceed
                }
            });
            widget.connect_button_release_event(move |_, e| {
                let state = e.state();
                let modifiers = Modifiers {
                    control: state.contains(ModifierType::CONTROL_MASK),
                    alt: state.contains(ModifierType::MOD1_MASK),
                    shift: state.contains(ModifierType::SHIFT_MASK),
                    meta: state.intersects(ModifierType::SUPER_MASK | ModifierType::META_MASK),
                };
                let Some(click) = side_button_click(e.button(), modifiers, e.position()) else {
                    return Propagation::Proceed;
                };
                let label = target.label().to_string();
                if let Err(err) =
                    target.emit_to(EventTarget::webview(label), SIDE_BUTTON_EVENT, click)
                {
                    tracing::warn!("cannot send the mouse side button: {err}");
                }
                Propagation::Stop
            });
        });
        if let Err(e) = result {
            tracing::warn!("cannot hook the mouse side buttons: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_gdk_side_buttons_to_dom_buttons() {
        assert_eq!(dom_side_button(8), Some(3));
        assert_eq!(dom_side_button(9), Some(4));
        for other in [0, 1, 2, 3, 4, 5, 6, 7, 10] {
            assert_eq!(dom_side_button(other), None, "{other}");
        }
    }

    #[test]
    fn builds_the_click_with_modifiers_and_position() {
        let click = side_button_click(
            9,
            Modifiers {
                control: true,
                shift: true,
                ..Default::default()
            },
            (12.0, 34.5),
        )
        .unwrap();
        assert_eq!(
            click,
            SideButtonClick {
                button: 4,
                ctrl_key: true,
                alt_key: false,
                shift_key: true,
                meta_key: false,
                x: 12.0,
                y: 34.5,
            }
        );
        assert!(side_button_click(1, Modifiers::default(), (0.0, 0.0)).is_none());
    }

    #[test]
    fn serializes_like_the_keymaps_mouse_input() {
        let click = side_button_click(8, Modifiers::default(), (1.0, 2.0)).unwrap();
        let json = serde_json::to_value(click).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "button": 3, "ctrlKey": false, "altKey": false, "shiftKey": false,
                "metaKey": false, "x": 1.0, "y": 2.0
            })
        );
    }
}
