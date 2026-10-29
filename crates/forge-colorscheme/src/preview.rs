//! The sample code of the Color Scheme page, split into segments by attribute.

use serde::{Deserialize, Serialize};

/// A piece of a preview line; `attribute` is a key of [`crate::model::ATTRIBUTES`] (none:
/// default text), `overlay` the selection or search result drawn behind it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewSegment {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribute: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<String>,
}

const OVERLAYS: [&str; 2] = ["SELECTION", "SEARCH_RESULT"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLine {
    pub segments: Vec<PreviewSegment>,
    /// The line with the caret (drawn with "Caret row").
    #[serde(default)]
    pub caret_row: bool,
}

/// `‹KEY:text›` marks a segment of attribute KEY; `‹SELECTION:…›` and `‹SEARCH_RESULT:…›`
/// mark text drawn with those backgrounds; a line starting with `>` holds the caret.
const SAMPLE: &str = r#"‹OPERATOR:<?php›
‹COMMENT:// Totals of an order, with the tax rate applied›
‹KEYWORD:namespace› ‹TYPE:App\Billing›‹OPERATOR:;›

‹ATTRIBUTE:#[Immutable]›
‹KEYWORD:final class› ‹TYPE:Invoice› ‹KEYWORD:extends› ‹TYPE:Document›
‹OPERATOR:{›
    ‹KEYWORD:private› ‹TYPE:float› ‹VARIABLE:$rate› ‹OPERATOR:=› ‹NUMBER:1.19›‹OPERATOR:;›

    ‹KEYWORD:public function› ‹FUNCTION:total›‹OPERATOR:(›‹TYPE:array› ‹VARIABLE:$items›‹OPERATOR:):› ‹TYPE:string›
    ‹OPERATOR:{›
>        ‹VARIABLE:$sum› ‹OPERATOR:=› ‹NUMBER:0›‹OPERATOR:;›
        ‹KEYWORD:foreach› ‹OPERATOR:(›‹VARIABLE:$items› ‹KEYWORD:as› ‹SELECTION:‹VARIABLE:$item››‹OPERATOR:) {›
            ‹VARIABLE:$sum› ‹OPERATOR:+=› ‹VARIABLE:$item›‹OPERATOR:->›‹ATTRIBUTE:price› ‹OPERATOR:*› ‹VARIABLE:$this›‹OPERATOR:->›‹ATTRIBUTE:rate›‹OPERATOR:;›
        ‹OPERATOR:}›
        ‹KEYWORD:return› ‹FUNCTION:number_format›‹OPERATOR:(›‹VARIABLE:$sum›‹OPERATOR:,› ‹NUMBER:2›‹OPERATOR:)› ‹OPERATOR:.› ‹STRING:" ‹SEARCH_RESULT:EUR›"›‹OPERATOR:;›
    ‹OPERATOR:}›
‹OPERATOR:}›
‹OPERATOR:?>›
‹OPERATOR:<›‹TAG:div› ‹ATTRIBUTE:class›‹OPERATOR:=›‹STRING:"total"›‹OPERATOR:>›Total‹OPERATOR:</›‹TAG:div›‹OPERATOR:>›"#;

/// Split a sample line; inside `‹SELECTION:…›` and `‹SEARCH_RESULT:…›` the text keeps its
/// own attribute and gets the overlay.
fn parse_line(line: &str) -> Vec<PreviewSegment> {
    let mut out: Vec<PreviewSegment> = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut text = String::new();
    let flush = |text: &mut String, stack: &[String], out: &mut Vec<PreviewSegment>| {
        if text.is_empty() {
            return;
        }
        let is_overlay = |k: &&String| OVERLAYS.contains(&k.as_str());
        out.push(PreviewSegment {
            text: std::mem::take(text),
            attribute: stack.iter().rev().find(|k| !is_overlay(k)).cloned(),
            overlay: stack.iter().find(is_overlay).cloned(),
        });
    };
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '‹' => {
                flush(&mut text, &stack, &mut out);
                let mut key = String::new();
                for k in chars.by_ref() {
                    if k == ':' {
                        break;
                    }
                    key.push(k);
                }
                stack.push(key);
            }
            '›' => {
                flush(&mut text, &stack, &mut out);
                stack.pop();
            }
            _ => text.push(c),
        }
    }
    flush(&mut text, &stack, &mut out);
    out
}

pub fn preview() -> Vec<PreviewLine> {
    SAMPLE
        .lines()
        .map(|line| {
            let (caret_row, line) = match line.strip_prefix('>') {
                Some(rest) => (true, rest),
                None => (false, line),
            };
            PreviewLine {
                segments: parse_line(line),
                caret_row,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::attribute;

    #[test]
    fn marks_only_known_attributes() {
        let lines = preview();
        assert!(lines.len() > 15);
        assert_eq!(lines.iter().filter(|l| l.caret_row).count(), 1);
        for line in &lines {
            for s in &line.segments {
                if let Some(a) = &s.attribute {
                    assert!(attribute(a).is_some(), "{a}");
                }
                assert!(!s.text.contains(['‹', '›']));
            }
        }
    }

    #[test]
    fn splits_nested_marks() {
        let segs = parse_line("‹KEYWORD:as› ‹SELECTION:‹VARIABLE:$item››!");
        let pairs: Vec<(&str, Option<&str>, Option<&str>)> = segs
            .iter()
            .map(|s| {
                (
                    s.text.as_str(),
                    s.attribute.as_deref(),
                    s.overlay.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            pairs,
            [
                ("as", Some("KEYWORD"), None),
                (" ", None, None),
                ("$item", Some("VARIABLE"), Some("SELECTION")),
                ("!", None, None)
            ]
        );
    }
}
