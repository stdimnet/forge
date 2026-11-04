//! Looking up and formatting messages for a locale, with fallback to English.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use crate::catalog::{builtin_messages, SOURCE_LOCALE};
use crate::message::{Args, Message};

/// Messages of one locale plus the fallback chain down to English.
#[derive(Debug, Clone)]
pub struct Localizer {
    locale: String,
    /// Locale ids to try in order, ending with English: `de-AT`, `de`, `en`.
    chain: Vec<String>,
    messages: HashMap<String, HashMap<String, String>>,
    tz_offset_minutes: i32,
}

/// Normalize a locale id: `de_DE.UTF-8` → `de-DE`, empty or `auto` → `en`.
pub fn normalize_locale(locale: &str) -> String {
    let base = locale.split(['.', '@']).next().unwrap_or("").trim();
    if base.is_empty() || base.eq_ignore_ascii_case("auto") || base == "C" || base == "POSIX" {
        return SOURCE_LOCALE.to_string();
    }
    let mut parts = base.split(['-', '_']);
    let lang = parts.next().unwrap_or("").to_ascii_lowercase();
    let rest: Vec<String> = parts
        .map(|p| {
            if p.len() == 2 {
                p.to_ascii_uppercase()
            } else {
                p.to_string()
            }
        })
        .collect();
    std::iter::once(lang)
        .chain(rest)
        .collect::<Vec<_>>()
        .join("-")
}

fn chain_of(locale: &str) -> Vec<String> {
    let mut chain = Vec::new();
    let mut cur = locale.to_string();
    loop {
        chain.push(cur.clone());
        match cur.rfind('-') {
            Some(i) => cur.truncate(i),
            None => break,
        }
    }
    if !chain.iter().any(|c| c == SOURCE_LOCALE) {
        chain.push(SOURCE_LOCALE.to_string());
    }
    chain
}

impl Localizer {
    /// A localizer with the bundled catalogs.
    pub fn new(locale: &str) -> Self {
        let mut l = Self::empty(locale);
        for loc in l.chain.clone() {
            let msgs = builtin_messages(&loc);
            if !msgs.is_empty() {
                l.messages.insert(loc, msgs);
            }
        }
        l
    }

    /// A localizer without any messages (tests, or catalogs added by hand).
    pub fn empty(locale: &str) -> Self {
        let locale = normalize_locale(locale);
        Self {
            chain: chain_of(&locale),
            locale,
            messages: HashMap::new(),
            tz_offset_minutes: 0,
        }
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// Minutes east of UTC that dates and times are shown in.
    pub fn set_timezone_offset(&mut self, minutes: i32) {
        self.tz_offset_minutes = minutes;
    }

    /// Add (or replace) messages of `locale`, e.g. a plugin's translations. Locales outside
    /// the fallback chain are ignored.
    pub fn add_messages(
        &mut self,
        locale: &str,
        messages: impl IntoIterator<Item = (String, String)>,
    ) {
        let locale = normalize_locale(locale);
        if self.chain.contains(&locale) {
            self.messages.entry(locale).or_default().extend(messages);
        }
    }

    /// The raw message for `key` and the locale it comes from, following the chain.
    pub fn lookup(&self, key: &str) -> Option<(&str, &str)> {
        self.chain.iter().find_map(|loc| {
            self.messages
                .get(loc)
                .and_then(|m| m.get(key))
                .map(|msg| (msg.as_str(), loc.as_str()))
        })
    }

    /// Whether `key` has a translation in the selected language (not just English).
    pub fn is_translated(&self, key: &str) -> bool {
        self.translation(key).is_some()
    }

    fn translation(&self, key: &str) -> Option<(&str, &str)> {
        self.chain
            .iter()
            .filter(|loc| loc.as_str() != SOURCE_LOCALE)
            .find_map(|loc| {
                self.messages
                    .get(loc)
                    .and_then(|m| m.get(key))
                    .map(|msg| (msg.as_str(), loc.as_str()))
            })
    }

    /// Format a message source in `locale`; a message that does not parse is shown as is.
    pub fn format_message(&self, message: &str, locale: &str, args: &Args) -> String {
        match Message::parse(message) {
            Ok(m) => m.format(locale, args, self.tz_offset_minutes),
            Err(_) => message.to_string(),
        }
    }

    /// The text of `key` without arguments.
    pub fn t(&self, key: &str) -> String {
        self.t_args(key, &Args::new())
    }

    /// The text of `key` formatted with `args`. A key missing in the selected language falls
    /// back to English; a key missing everywhere (a bug that `pnpm i18n:check` reports) is
    /// shown as the key.
    pub fn t_args(&self, key: &str, args: &Args) -> String {
        match self.lookup(key) {
            Some((msg, loc)) => self.format_message(msg, loc, args),
            None => key.to_string(),
        }
    }

    /// The translation of `key` in the selected language, else `english` as is. For text whose
    /// English source lives in code or manifests (menu labels, action titles, settings).
    pub fn t_or(&self, key: &str, english: &str) -> String {
        match self.translation(key) {
            Some((msg, loc)) => self.format_message(msg, loc, &Args::new()),
            None => english.to_string(),
        }
    }

    /// Like [`Localizer::t_or`] but `None` when there is no translation.
    pub fn translate(&self, key: &str) -> Option<String> {
        self.translation(key)
            .map(|(msg, loc)| self.format_message(msg, loc, &Args::new()))
    }
}

// ---- The process-wide localizer

struct Global {
    current: Arc<Localizer>,
    /// Messages added at runtime (plugin translations), re-applied on a locale change.
    extra: Vec<(String, String, String)>,
    tz_offset_minutes: i32,
}

fn global() -> &'static RwLock<Global> {
    static GLOBAL: OnceLock<RwLock<Global>> = OnceLock::new();
    GLOBAL.get_or_init(|| {
        RwLock::new(Global {
            current: Arc::new(Localizer::new(SOURCE_LOCALE)),
            extra: Vec::new(),
            tz_offset_minutes: 0,
        })
    })
}

fn rebuild(g: &mut Global, locale: &str) {
    let mut l = Localizer::new(locale);
    l.set_timezone_offset(g.tz_offset_minutes);
    for (loc, k, v) in &g.extra {
        l.add_messages(loc, [(k.clone(), v.clone())]);
    }
    g.current = Arc::new(l);
}

/// The process-wide localizer (English until [`set_locale`] is called).
pub fn current() -> Arc<Localizer> {
    global().read().expect("i18n lock poisoned").current.clone()
}

/// Select the language of Rust-originated text (menus, action titles, settings, errors).
pub fn set_locale(locale: &str) {
    let mut g = global().write().expect("i18n lock poisoned");
    if g.current.locale() != normalize_locale(locale) {
        rebuild(&mut g, locale);
    }
}

/// Minutes east of UTC for dates and times in messages.
pub fn set_timezone_offset(minutes: i32) {
    let mut g = global().write().expect("i18n lock poisoned");
    g.tz_offset_minutes = minutes;
    let locale = g.current.locale().to_string();
    rebuild(&mut g, &locale);
}

/// Add translations at runtime, e.g. from a plugin manifest's `[l10n.<locale>]` table.
pub fn add_messages(locale: &str, messages: impl IntoIterator<Item = (String, String)>) {
    let mut g = global().write().expect("i18n lock poisoned");
    let locale = normalize_locale(locale);
    let mut changed = false;
    for (k, v) in messages {
        if !g
            .extra
            .iter()
            .any(|(l, ek, ev)| *l == locale && *ek == k && *ev == v)
        {
            g.extra.push((locale.clone(), k, v));
            changed = true;
        }
    }
    if changed {
        let current = g.current.locale().to_string();
        rebuild(&mut g, &current);
    }
}

/// [`Localizer::t`] of the process-wide localizer.
pub fn t(key: &str) -> String {
    current().t(key)
}

/// [`Localizer::t_args`] of the process-wide localizer.
pub fn t_args(key: &str, args: &Args) -> String {
    current().t_args(key, args)
}

/// [`Localizer::t_or`] of the process-wide localizer.
pub fn t_or(key: &str, english: &str) -> String {
    current().t_or(key, english)
}

/// Build message arguments: `args!{ "count" => 3, "name" => path }`.
#[macro_export]
macro_rules! args {
    ($($k:expr => $v:expr),* $(,)?) => {{
        #[allow(unused_mut)]
        let mut m = $crate::Args::new();
        $( m.insert($k.to_string(), ::serde_json::json!($v)); )*
        m
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args;

    fn sample(locale: &str) -> Localizer {
        let mut l = Localizer::empty(locale);
        l.add_messages(
            "en",
            [
                ("a.title".to_string(), "Project".to_string()),
                (
                    "a.files".to_string(),
                    "{n, plural, one {# file} other {# files}}".to_string(),
                ),
                ("a.only-en".to_string(), "Only English".to_string()),
            ],
        );
        l.add_messages(
            "de",
            [
                ("a.title".to_string(), "Projekt".to_string()),
                (
                    "a.files".to_string(),
                    "{n, plural, one {# Datei} other {# Dateien}}".to_string(),
                ),
            ],
        );
        l.add_messages(
            "de-AT",
            [("a.title".to_string(), "Projekt (AT)".to_string())],
        );
        l
    }

    #[test]
    fn normalizes_locale_ids() {
        assert_eq!(normalize_locale("de_DE.UTF-8"), "de-DE");
        assert_eq!(normalize_locale("DE-at"), "de-AT");
        assert_eq!(normalize_locale(""), "en");
        assert_eq!(normalize_locale("auto"), "en");
        assert_eq!(chain_of("de-AT"), ["de-AT", "de", "en"]);
        assert_eq!(chain_of("en"), ["en"]);
    }

    #[test]
    fn lookup_falls_back_to_english_never_to_the_key() {
        let de = sample("de");
        assert_eq!(de.t("a.title"), "Projekt");
        assert_eq!(de.t("a.only-en"), "Only English");
        assert_eq!(de.t_args("a.files", &args! { "n" => 2 }), "2 Dateien");
        assert_eq!(sample("de-AT").t("a.title"), "Projekt (AT)");
        assert_eq!(sample("de-CH").t("a.title"), "Projekt");
        assert_eq!(sample("fr").t("a.title"), "Project");
        assert_eq!(sample("en").t_args("a.files", &args! { "n" => 1 }), "1 file");
        // Only a key missing in English too shows as itself.
        assert_eq!(de.t("a.nowhere"), "a.nowhere");
    }

    #[test]
    fn t_or_uses_code_english() {
        let de = sample("de");
        assert_eq!(de.t_or("a.title", "Project"), "Projekt");
        assert_eq!(de.t_or("a.only-en", "From code"), "From code");
        assert!(de.is_translated("a.title"));
        assert!(!de.is_translated("a.only-en"));
        assert_eq!(sample("en").t_or("a.title", "From code"), "From code");
    }

    #[test]
    fn bundled_catalogs_load() {
        let de = Localizer::new("de");
        assert_eq!(de.t("common.cancel"), "Abbrechen");
        assert_eq!(Localizer::new("en").t("common.cancel"), "Cancel");
        assert_eq!(Localizer::new("xx").t("common.cancel"), "Cancel");
    }

    #[test]
    fn global_switches_locale_and_keeps_extra_messages() {
        add_messages("de", [("test.extra".to_string(), "Zusatz".to_string())]);
        add_messages("en", [("test.extra".to_string(), "Extra".to_string())]);
        set_locale("de");
        assert_eq!(t("test.extra"), "Zusatz");
        assert_eq!(t("common.cancel"), "Abbrechen");
        set_locale("en");
        assert_eq!(t("test.extra"), "Extra");
        assert_eq!(t_or("common.cancel", "Cancel it"), "Cancel it");
    }
}
