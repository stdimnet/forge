//! A [`SiteFinder`] without a PHP parser, for browser mode (WebAssembly cannot run the
//! tree-sitter grammar): string literals are found lexically, the call around them by
//! walking back to its `(`, and QueryBuilder aliases are read from the whole file.

use crate::model::{Entity, Snapshot};
use crate::site::{builder_aliases, mode_for, Segment, Site, SiteFinder};

/// The lexical finder.
pub struct Lexical;

/// A string literal: the whole token and its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Lit {
    start: usize,
    end: usize,
    content_start: usize,
    content_end: usize,
}

fn line_end(b: &[u8], i: usize) -> usize {
    b[i..]
        .iter()
        .position(|&c| c == b'\n')
        .map_or(b.len(), |p| i + p)
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// `<<<ID`, `<<< "ID"` or `<<<'ID'` followed by a newline: (header length, identifier).
fn heredoc_header(src: &str, i: usize) -> Option<(usize, &str)> {
    let b = src.as_bytes();
    let mut j = i + 3;
    while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
        j += 1;
    }
    let quote = match b.get(j) {
        Some(&q @ (b'\'' | b'"')) => {
            j += 1;
            Some(q)
        }
        _ => None,
    };
    let id_start = j;
    if !b
        .get(j)
        .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
    {
        return None;
    }
    while j < b.len() && is_word(b[j]) {
        j += 1;
    }
    let id = &src[id_start..j];
    if let Some(q) = quote {
        if b.get(j) != Some(&q) {
            return None;
        }
        j += 1;
    }
    if b.get(j) == Some(&b'\r') {
        j += 1;
    }
    (b.get(j) == Some(&b'\n')).then_some((j + 1 - i, id))
}

/// String literals of a PHP file, skipping comments.
fn php_strings(src: &str) -> Vec<Lit> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if (c == b'/' && b.get(i + 1) == Some(&b'/')) || (c == b'#' && b.get(i + 1) != Some(&b'['))
        {
            i = line_end(b, i);
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i = src[i + 2..].find("*/").map_or(b.len(), |p| i + 2 + p + 2);
            continue;
        }
        if c == b'\'' || c == b'"' {
            let mut j = i + 1;
            while j < b.len() && b[j] != c {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            out.push(Lit {
                start: i,
                end: (j + 1).min(b.len()),
                content_start: i + 1,
                content_end: j.min(b.len()),
            });
            i = j + 1;
            continue;
        }
        if b[i..].starts_with(b"<<<") {
            if let Some((header, id)) = heredoc_header(src, i) {
                let body_start = i + header;
                // The closing identifier: on its own line, optionally indented.
                let mut line = body_start;
                let mut close = None;
                while line < b.len() {
                    let mut k = line;
                    while k < b.len() && (b[k] == b' ' || b[k] == b'\t') {
                        k += 1;
                    }
                    if src[k..].starts_with(id) && !b.get(k + id.len()).is_some_and(|c| is_word(*c))
                    {
                        close = Some((line, k + id.len()));
                        break;
                    }
                    line = line_end(b, line) + 1;
                }
                let (body_end, end) = close.unwrap_or((b.len(), b.len()));
                out.push(Lit {
                    start: i,
                    end,
                    content_start: body_start,
                    content_end: body_end.saturating_sub(1).max(body_start),
                });
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// `Name` before `pos` (skipping whitespace) and the text before it.
fn method_before(src: &str, open: usize) -> Option<(&str, &str)> {
    let mut head_start = open.saturating_sub(200);
    while !src.is_char_boundary(head_start) {
        head_start += 1;
    }
    let head = src[head_start..open].trim_end();
    let b = head.as_bytes();
    let mut s = b.len();
    while s > 0 && is_word(b[s - 1]) {
        s -= 1;
    }
    if s == b.len() || !(b[s].is_ascii_alphabetic() || b[s] == b'_') {
        return None;
    }
    let object = head[..s].trim_end();
    let object = ["?->", "->", "::"]
        .iter()
        .find_map(|op| object.strip_suffix(op))
        .unwrap_or(object);
    Some((&head[s..], object))
}

/// `Name::class` at `k` (not part of a longer name).
fn class_constant(src: &str, k: usize) -> Option<(&str, usize)> {
    let b = src.as_bytes();
    if k > 0 && (is_word(b[k - 1]) || b[k - 1] == b'\\' || b[k - 1] == b'$') {
        return None;
    }
    let mut j = k;
    while j < b.len() && (is_word(b[j]) || b[j] == b'\\') {
        j += 1;
    }
    if j == k || !src[j..].starts_with("::class") || b.get(j + 7).is_some_and(|c| is_word(*c)) {
        return None;
    }
    Some((&src[k..j], j + 7))
}

/// Namespace and class of the file, for `$this->createQueryBuilder()` in repositories.
fn this_entity<'a>(snap: &'a Snapshot, path: &str, src: &str) -> Option<&'a Entity> {
    if let Some(r) = snap.repositories.iter().find(|r| r.path == path) {
        return snap.entity_of_repository(&r.class);
    }
    let word_after = |kw: &str| {
        src.find(kw).map(|i| {
            src[i + kw.len()..]
                .trim_start()
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '\\'))
                .next()
                .unwrap_or_default()
        })
    };
    let class = word_after("\nclass ")?;
    let fqn = match word_after("namespace ") {
        Some(ns) if !ns.is_empty() => format!("{ns}\\{class}"),
        _ => class.to_string(),
    };
    snap.entity_of_repository(&fqn)
}

impl SiteFinder for Lexical {
    fn sites(&self, snap: &Snapshot, path: &str, src: &str) -> Vec<Site> {
        let b = src.as_bytes();
        let strings = php_strings(src);
        let resolve = |name: &str| {
            snap.entity(name)
                .map(|e| e.class.clone())
                .unwrap_or_else(|| name.trim_start_matches('\\').to_string())
        };
        let mut preset: Option<Vec<(String, String)>> = None;
        let mut sites = Vec::new();
        let mut seen_calls = Vec::new();
        for s in &strings {
            // Walk back to the call's opening parenthesis.
            let mut depth = 0;
            let mut open = None;
            let mut i = s.start;
            while i > 0 && s.start - i < 4000 {
                i -= 1;
                match b[i] {
                    b')' | b']' => depth += 1,
                    b'(' | b'[' if depth > 0 => depth -= 1,
                    b'(' => {
                        open = Some(i);
                        break;
                    }
                    b'[' | b';' | b'{' | b'}' => break,
                    b'\'' | b'"' => {
                        if let Some(lit) = strings.iter().find(|x| x.end == i + 1) {
                            i = lit.start;
                        }
                    }
                    _ => {}
                }
            }
            let Some(open) = open else { continue };
            if seen_calls.contains(&open) {
                continue;
            }
            let Some((method, object)) = method_before(src, open) else {
                continue;
            };
            seen_calls.push(open);
            // Split the argument list and collect string pieces per argument.
            let mut arg = 0;
            let mut segments: Vec<Segment> = Vec::new();
            let mut dql = String::new();
            let mut flush = |arg: usize, segments: &mut Vec<Segment>, dql: &mut String| {
                if let Some(mode) = mode_for(method, arg, object) {
                    if !segments.is_empty() {
                        let preset = if mode == crate::dql::Mode::Full {
                            Vec::new()
                        } else {
                            preset
                                .get_or_insert_with(|| {
                                    builder_aliases(
                                        snap,
                                        src,
                                        &resolve,
                                        this_entity(snap, path, src),
                                    )
                                })
                                .clone()
                        };
                        sites.push(Site {
                            dql: std::mem::take(dql),
                            mode,
                            preset,
                            segments: std::mem::take(segments),
                            method: method.to_string(),
                        });
                    }
                }
                segments.clear();
                dql.clear();
            };
            let mut depth = 0;
            let mut k = open + 1;
            while k < b.len() {
                if let Some(lit) = strings.iter().find(|x| x.start == k) {
                    if depth == 0 {
                        segments.push(Segment {
                            dql_start: dql.len(),
                            src_start: lit.content_start,
                            len: lit.content_end - lit.content_start,
                        });
                        dql.push_str(&src[lit.content_start..lit.content_end]);
                    }
                    k = lit.end;
                    continue;
                }
                match b[k] {
                    b'(' | b'[' => depth += 1,
                    b')' | b']' if depth > 0 => depth -= 1,
                    b')' => break,
                    b',' if depth == 0 => {
                        flush(arg, &mut segments, &mut dql);
                        arg += 1;
                    }
                    _ if depth == 0 => {
                        if let Some((name, next)) = class_constant(src, k) {
                            dql.push_str(&resolve(name));
                            k = next;
                            continue;
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            flush(arg, &mut segments, &mut dql);
        }
        sites.sort_by_key(|s| s.segments.first().map(|x| x.src_start).unwrap_or(0));
        sites
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dql::Mode;
    use crate::fixture;
    use crate::site;

    const REPO: &str = "src/Repository/OrderRepository.php";
    const SRC: &str = r#"<?php
namespace App\Repository;

use App\Entity\Order;

class OrderRepository extends ServiceEntityRepository
{
    public function findPaid(): array
    {
        // createQuery('not this')
        return $this->createQueryBuilder('o')
            ->join('o.customer', 'c')
            ->andWhere('o.status = :status')
            ->orderBy('o.paidAt', 'DESC')
            ->getQuery()
            ->getResult();
    }

    public function totals(): array
    {
        $this->render('order/show.html.twig');
        return $this->getEntityManager()->createQuery(
            'SELECT c.email, SUM(o.total) AS total FROM ' . Order::class . ' o
             JOIN o.customer c WHERE o.nope = 1'
        )->getResult();
    }

    public function heredoc(): array
    {
        return $this->getEntityManager()->createQuery(<<<'DQL'
            SELECT o FROM App\Entity\Order o
            DQL)->getResult();
    }
}
"#;

    #[test]
    fn finds_create_query_and_query_builder_arguments() {
        let snap = fixture::snapshot();
        let sites = Lexical.sites(&snap, REPO, SRC);
        let got: Vec<(&str, Mode, &str)> = sites
            .iter()
            .map(|s| (s.method.as_str(), s.mode, s.dql.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("join", Mode::Join, "o.customer"),
                ("andWhere", Mode::Expression, "o.status = :status"),
                ("orderBy", Mode::Expression, "o.paidAt"),
                (
                    "createQuery",
                    Mode::Full,
                    "SELECT c.email, SUM(o.total) AS total FROM App\\Entity\\Order o\n             JOIN o.customer c WHERE o.nope = 1"
                ),
                ("createQuery", Mode::Full, "            SELECT o FROM App\\Entity\\Order o"),
            ]
        );
        assert_eq!(
            sites[1].preset,
            [
                ("o".to_string(), "App\\Entity\\Order".to_string()),
                ("c".to_string(), "App\\Entity\\Customer".to_string())
            ]
        );
    }

    #[test]
    fn maps_offsets_and_serves_completion_goto_and_diagnostics() {
        let snap = fixture::snapshot();
        let pos = SRC.find("o.status = :status").unwrap() + 2;
        let site = Lexical.site_at(&snap, REPO, SRC, pos).unwrap();
        assert_eq!(site.to_dql(pos), Some(2));
        assert!(Lexical
            .site_at(&snap, REPO, SRC, SRC.find("order/show").unwrap())
            .is_none());
        let (from, c) = site::complete(&Lexical, &snap, REPO, SRC, pos + 3).unwrap();
        assert_eq!(&SRC[from..pos + 3], "sta");
        assert_eq!(c.items[0].label, "status");
        let loc = &site::goto(
            &Lexical,
            &snap,
            REPO,
            SRC,
            SRC.find("o.customer c").unwrap() + 3,
        )[0];
        assert_eq!(loc.path, "src/Entity/Order.php");
        let d = site::diagnostics(&Lexical, &snap, REPO, SRC);
        assert_eq!(
            d.iter().map(|x| &SRC[x.start..x.end]).collect::<Vec<_>>(),
            ["nope"]
        );
        let h = site::highlight(&Lexical, &snap, REPO, SRC);
        let text = |s: &site::SourceSpan| &SRC[s.from..s.to];
        assert!(h
            .spans
            .iter()
            .any(|s| text(s) == "status" && s.kind == crate::dql::SpanKind::Field));
        assert!(h
            .spans
            .iter()
            .any(|s| text(s) == "SELECT" && s.kind == crate::dql::SpanKind::Keyword));
        assert_eq!(h.injected.len(), 6);
    }
}
