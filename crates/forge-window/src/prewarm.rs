//! Dialog windows that are built ahead of time ("parked").
//!
//! A dialog window boots the whole UI before it can show anything, which takes far longer
//! than a dialog may take to appear. The Settings window is therefore built hidden while the
//! IDE is idle and kept parked; opening it only hands the parked window its props
//! (`activate`) and shows it.
//!
//! The UI of a parked window reports when it has booted. A request that arrives earlier is
//! kept and handed over with that report, so the window is activated exactly once whichever
//! comes first.

use std::collections::HashMap;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
enum State {
    /// The window's UI is still loading.
    Booting,
    /// The UI has loaded and waits for its props.
    Ready,
    /// The dialog was requested while the UI was loading; it gets these props when it reports.
    Requested(Value),
}

#[derive(Debug, Clone, PartialEq)]
struct Parked {
    owner: String,
    state: State,
}

/// How a parked window gets the props of the dialog it now shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Activation {
    /// Its UI waits for them: send them now.
    Send(Value),
    /// Its UI is still loading and gets them when it reports (`ParkedDialogs::booted`).
    WhenBooted,
}

/// What the UI of a window that has booted does next.
#[derive(Debug, Clone, PartialEq)]
pub enum Booted {
    /// Stay hidden until the dialog is requested.
    Wait,
    /// The dialog was requested meanwhile: show it with these props.
    Activate(Value),
}

/// The parked dialog windows by window label.
#[derive(Debug, Default)]
pub struct ParkedDialogs {
    windows: HashMap<String, Parked>,
}

impl ParkedDialogs {
    /// A hidden window `label` was built for `owner`. `false` when one is parked already.
    pub fn park(&mut self, label: &str, owner: &str) -> bool {
        if self.windows.contains_key(label) {
            return false;
        }
        self.windows.insert(
            label.to_string(),
            Parked {
                owner: owner.to_string(),
                state: State::Booting,
            },
        );
        true
    }

    /// The window waits for a request (it may still be loading).
    pub fn is_parked(&self, label: &str) -> bool {
        self.windows
            .get(label)
            .is_some_and(|p| !matches!(p.state, State::Requested(_)))
    }

    /// The dialog was requested with `props`: `None` when no window is parked for it (build
    /// one, or focus the open one).
    pub fn activate(&mut self, label: &str, props: Value) -> Option<Activation> {
        let parked = self.windows.get_mut(label)?;
        match parked.state {
            State::Booting => {
                parked.state = State::Requested(props);
                Some(Activation::WhenBooted)
            }
            State::Ready => {
                self.windows.remove(label);
                Some(Activation::Send(props))
            }
            State::Requested(_) => None,
        }
    }

    /// The UI of window `label` has loaded.
    pub fn booted(&mut self, label: &str) -> Booted {
        match self.windows.get_mut(label) {
            Some(parked) if parked.state == State::Booting => {
                parked.state = State::Ready;
                Booted::Wait
            }
            Some(Parked {
                state: State::Requested(_),
                ..
            }) => match self.windows.remove(label).map(|p| p.state) {
                Some(State::Requested(props)) => Booted::Activate(props),
                _ => Booted::Wait,
            },
            _ => Booted::Wait,
        }
    }

    /// Forget window `label` (it was destroyed). `true` when it was parked.
    pub fn remove(&mut self, label: &str) -> bool {
        self.windows.remove(label).is_some()
    }

    /// Labels of the windows parked for `owner` that no dialog was requested for: they go
    /// with their owner.
    pub fn waiting_for(&self, owner: &str) -> Vec<String> {
        let mut labels: Vec<String> = self
            .windows
            .iter()
            .filter(|(_, p)| p.owner == owner && !matches!(p.state, State::Requested(_)))
            .map(|(label, _)| label.clone())
            .collect();
        labels.sort();
        labels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const LABEL: &str = "dialog-settings";

    #[test]
    fn a_booted_window_is_activated_at_once() {
        let mut parked = ParkedDialogs::default();
        assert!(parked.park(LABEL, "main"));
        assert_eq!(parked.booted(LABEL), Booted::Wait);
        assert!(parked.is_parked(LABEL));
        let props = json!({ "page": "keymap" });
        assert_eq!(
            parked.activate(LABEL, props.clone()),
            Some(Activation::Send(props))
        );
        // The window is the open dialog now: a second request focuses it.
        assert!(!parked.is_parked(LABEL));
        assert_eq!(parked.activate(LABEL, json!({})), None);
    }

    #[test]
    fn a_request_while_the_window_boots_is_handed_over_when_it_reports() {
        let mut parked = ParkedDialogs::default();
        parked.park(LABEL, "main");
        let props = json!({ "page": "plugins" });
        assert_eq!(
            parked.activate(LABEL, props.clone()),
            Some(Activation::WhenBooted)
        );
        // Asked twice while it loads (#227): still one activation.
        assert_eq!(parked.activate(LABEL, json!({ "page": "other" })), None);
        assert!(!parked.is_parked(LABEL));
        assert_eq!(parked.booted(LABEL), Booted::Activate(props));
        // Reported twice: nothing more to do.
        assert_eq!(parked.booted(LABEL), Booted::Wait);
    }

    #[test]
    fn one_window_is_parked_per_dialog() {
        let mut parked = ParkedDialogs::default();
        assert!(parked.park(LABEL, "main"));
        assert!(!parked.park(LABEL, "main"));
        assert!(parked.remove(LABEL));
        assert!(!parked.remove(LABEL));
        assert!(parked.park(LABEL, "main"));
    }

    #[test]
    fn a_window_that_is_not_parked_is_not_activated() {
        let mut parked = ParkedDialogs::default();
        assert_eq!(parked.activate(LABEL, json!({})), None);
        assert_eq!(parked.booted(LABEL), Booted::Wait);
        assert!(!parked.is_parked(LABEL));
    }

    #[test]
    fn parked_windows_go_with_their_owner() {
        let mut parked = ParkedDialogs::default();
        parked.park(LABEL, "main");
        parked.park("dialog-other", "main");
        parked.park("dialog-light", "dialog-light-edit");
        // A requested dialog is about to be shown: it closes like an open dialog.
        parked.activate("dialog-other", json!({}));
        assert_eq!(parked.waiting_for("main"), vec![LABEL.to_string()]);
        assert!(parked.waiting_for("nobody").is_empty());
    }
}
