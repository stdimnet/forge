//! Formatter markers (Code Style | General | Formatter): the lines between `@formatter:off`
//! and `@formatter:on` comments keep the text they had before formatting.

use super::Opts;

/// Byte ranges of the lines between the markers (from the line after the off marker to the
/// start of the line with the on marker, or the end of the text).
fn regions(text: &str, off: &str, on: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        match start {
            None if line.contains(off) => start = Some(at + line.len()),
            Some(s) if line.contains(on) => {
                out.push((s, at));
                start = None;
            }
            _ => {}
        }
        at += line.len();
    }
    if let Some(s) = start {
        out.push((s, text.len()));
    }
    out
}

/// `formatted` with the marked regions of `original` put back. `None` when the markers are
/// off or the two texts do not have the same regions.
pub fn restore(original: &str, formatted: &str, general: &Opts<'_>) -> Option<String> {
    if !general.bool("formatterTags.enabled") {
        return None;
    }
    let off = general.str("formatterTags.off");
    let on = general.str("formatterTags.on");
    if off.trim().is_empty() || on.trim().is_empty() {
        return None;
    }
    let before = regions(original, &off, &on);
    let after = regions(formatted, &off, &on);
    if before.is_empty() || before.len() != after.len() {
        return None;
    }
    let mut out = String::with_capacity(formatted.len());
    let mut at = 0;
    for ((os, oe), (fs, fe)) in before.into_iter().zip(after) {
        out.push_str(&formatted[at..fs]);
        out.push_str(&original[os..oe]);
        at = fe;
    }
    out.push_str(&formatted[at..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{model, GENERAL};
    use serde_json::json;

    #[test]
    fn keeps_marked_regions() {
        let mut values = model().defaults();
        let general = Opts::new(GENERAL, &values);
        let original = "a=1\n// @formatter:off\nb   =   2\n// @formatter:on\nc=3\n";
        let formatted = "a = 1\n// @formatter:off\nb = 2\n// @formatter:on\nc = 3\n";
        assert_eq!(restore(original, formatted, &general), None);
        values.insert("general.formatterTags.enabled".into(), json!(true));
        let general = Opts::new(GENERAL, &values);
        assert_eq!(
            restore(original, formatted, &general).unwrap(),
            "a = 1\n// @formatter:off\nb   =   2\n// @formatter:on\nc = 3\n"
        );
        // Unclosed: to the end of the file.
        assert_eq!(
            restore(
                "x=1\n# @formatter:off\ny=2\n",
                "x = 1\n# @formatter:off\ny = 2\n",
                &general
            )
            .unwrap(),
            "x = 1\n# @formatter:off\ny=2\n"
        );
        // Different regions: nothing restored.
        assert_eq!(restore(original, "a = 1\n", &general), None);
    }
}
