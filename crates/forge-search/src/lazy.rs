use std::path::PathBuf;

use forge_find::{FileMatches, FindPaged, LineMatch};

/// The matching lines of one file, kept as compact byte ranges until a page needs their text.
pub(crate) enum Rows {
    /// Match ranges (byte offsets) and, per row, the index after its last range.
    Ranges {
        ranges: Vec<(u32, u32)>,
        row_ends: Vec<u32>,
    },
    /// Rows built eagerly, for files the compact form cannot describe (matches spanning lines,
    /// text that is not valid UTF-8).
    Lines(Vec<LineMatch>),
}

impl Rows {
    pub(crate) fn len(&self) -> usize {
        match self {
            Rows::Ranges { row_ends, .. } => row_ends.len(),
            Rows::Lines(rows) => rows.len(),
        }
    }
}

pub(crate) struct LazyFile {
    pub path: String,
    pub rows: Rows,
}

/// Groups sorted match `ranges` of `bytes` into rows (lines) and cuts them to `budget` matches,
/// the way `Finder::search` does: a line whose matches would overshoot the budget is left out.
/// Returns the kept ranges, the row ends, the match count and whether the budget cut anything.
pub(crate) fn group_rows(
    bytes: &[u8],
    ranges: &[(usize, usize)],
    budget: usize,
) -> (Vec<(u32, u32)>, Vec<u32>, usize, bool) {
    let mut kept: Vec<(u32, u32)> = Vec::new();
    let mut row_ends: Vec<u32> = Vec::new();
    let mut count = 0;
    let mut i = 0;
    while i < ranges.len() {
        let mut j = i + 1;
        while j < ranges.len() && !bytes[ranges[j - 1].1..ranges[j].0].contains(&b'\n') {
            j += 1;
        }
        let found = j - i;
        if count + found > budget {
            return (kept, row_ends, count, true);
        }
        count += found;
        kept.extend(ranges[i..j].iter().map(|&(s, e)| (s as u32, e as u32)));
        row_ends.push(kept.len() as u32);
        i = j;
    }
    (kept, row_ends, count, false)
}

/// A Find in Files result held as match ranges. Totals are known right away; the text of a row is
/// read from its file when a page asks for it, so a search that matches millions of lines costs
/// memory in proportion to the matches, not to their text.
pub struct FoundText {
    pub(crate) root: PathBuf,
    pub(crate) files: Vec<LazyFile>,
    pub(crate) context: usize,
    pub match_count: usize,
    pub truncated: bool,
    pub error: Option<String>,
}

impl FoundText {
    pub(crate) fn failed(root: PathBuf, error: String) -> Self {
        Self {
            root,
            files: Vec::new(),
            context: 0,
            match_count: 0,
            truncated: false,
            error: Some(error),
        }
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn row_count(&self) -> usize {
        self.files.iter().map(|f| f.rows.len()).sum()
    }

    /// The rows `[offset, offset + count)` in result order; a file cut by the page edge appears
    /// with its share of the rows.
    pub fn page(&self, offset: usize, count: usize) -> Vec<FileMatches> {
        let mut out = Vec::new();
        let (mut skip, mut left) = (offset, count);
        for file in &self.files {
            if left == 0 {
                break;
            }
            let rows = file.rows.len();
            if skip >= rows {
                skip -= rows;
                continue;
            }
            let take = (rows - skip).min(left);
            let matches = match &file.rows {
                Rows::Lines(lines) => lines[skip..skip + take].to_vec(),
                Rows::Ranges { ranges, row_ends } => {
                    let from = if skip == 0 {
                        0
                    } else {
                        row_ends[skip - 1] as usize
                    };
                    let to = row_ends[skip + take - 1] as usize;
                    self.rows_of(&file.path, &ranges[from..to])
                }
            };
            out.push(FileMatches {
                path: file.path.clone(),
                matches,
            });
            left -= take;
            skip = 0;
        }
        out
    }

    fn rows_of(&self, path: &str, ranges: &[(u32, u32)]) -> Vec<LineMatch> {
        let Ok(bytes) = std::fs::read(self.root.join(path)) else {
            return Vec::new();
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            return Vec::new();
        };
        let ranges: Vec<(usize, usize)> = ranges
            .iter()
            .map(|&(s, e)| (s as usize, e as usize))
            .collect();
        let mut rows = forge_find::lines_from_ranges(text, &ranges, usize::MAX).0;
        forge_find::add_context(text, &mut rows, self.context);
        rows
    }

    /// The totals with the first `page_size` rows.
    pub fn paged(&self, id: u64, page_size: usize) -> FindPaged {
        FindPaged {
            id,
            match_count: self.match_count,
            truncated: self.truncated,
            error: self.error.clone(),
            file_count: self.files.len(),
            row_count: self.row_count(),
            files: self.page(0, page_size),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_lines_and_the_budget_cuts_whole_lines() {
        let text = b"beta beta\nx\nbeta\nbeta beta beta\n";
        let ranges: Vec<(usize, usize)> = text
            .windows(4)
            .enumerate()
            .filter(|(_, w)| *w == b"beta")
            .map(|(i, _)| (i, i + 4))
            .collect();
        let (kept, ends, count, cut) = group_rows(text, &ranges, 100);
        assert_eq!(
            (kept.len(), ends.clone(), count, cut),
            (6, vec![2, 3, 6], 6, false)
        );
        let (_, ends, count, cut) = group_rows(text, &ranges, 4);
        assert_eq!((ends, count, cut), (vec![2, 3], 3, true));
        let (_, ends, count, cut) = group_rows(text, &ranges, 0);
        assert_eq!((ends.len(), count, cut), (0, 0, true));
    }
}
