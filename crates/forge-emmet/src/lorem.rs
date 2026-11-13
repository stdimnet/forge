//! `lorem` / `loremN` / `lipsumN`: placeholder text. Deterministic (the same abbreviation
//! always gives the same words), which keeps previews stable and tests simple.

const COMMON: &[&str] = &[
    "lorem",
    "ipsum",
    "dolor",
    "sit",
    "amet",
    "consectetur",
    "adipisicing",
    "elit",
];

const WORDS: &[&str] = &[
    "exercitationem",
    "perferendis",
    "perspiciatis",
    "laborum",
    "eveniet",
    "sunt",
    "iure",
    "nam",
    "nobis",
    "eum",
    "cum",
    "officiis",
    "excepturi",
    "odio",
    "consectetur",
    "quasi",
    "aut",
    "quisquam",
    "vel",
    "eligendi",
    "itaque",
    "non",
    "odit",
    "tempore",
    "quaerat",
    "dignissimos",
    "facilis",
    "neque",
    "nihil",
    "expedita",
    "vitae",
    "vero",
    "ipsum",
    "nisi",
    "animi",
    "cumque",
    "pariatur",
    "velit",
    "modi",
    "natus",
    "iusto",
    "eaque",
    "sequi",
    "illo",
    "sed",
    "ex",
    "et",
    "voluptatibus",
    "tempora",
    "veritatis",
    "ratione",
    "assumenda",
    "incidunt",
    "nostrum",
    "placeat",
    "aliquid",
    "fuga",
    "provident",
    "praesentium",
    "rem",
    "necessitatibus",
    "suscipit",
    "adipisci",
    "quidem",
    "possimus",
    "voluptas",
    "debitis",
    "sint",
    "accusantium",
    "unde",
    "sapiente",
    "voluptate",
    "qui",
    "aspernatur",
    "laudantium",
    "soluta",
    "amet",
    "quo",
    "aliquam",
    "saepe",
    "culpa",
    "libero",
    "ipsa",
    "dicta",
    "reiciendis",
    "nesciunt",
    "doloribus",
    "autem",
    "impedit",
    "minima",
    "maiores",
    "repudiandae",
    "ipsam",
    "obcaecati",
    "ullam",
    "enim",
    "totam",
    "delectus",
    "ducimus",
    "quis",
    "voluptates",
    "dolores",
    "molestiae",
    "harum",
    "dolorem",
    "quia",
    "voluptatem",
    "molestias",
    "magni",
    "distinctio",
    "omnis",
    "illum",
    "dolorum",
    "voluptatum",
    "ea",
    "quas",
    "quam",
    "corporis",
    "quae",
    "blanditiis",
    "atque",
    "deserunt",
    "laboriosam",
    "earum",
    "consequuntur",
    "hic",
    "cupiditate",
    "quibusdam",
    "accusamus",
    "ut",
    "rerum",
    "error",
    "minus",
    "eius",
    "ab",
    "ad",
    "nemo",
    "fugit",
    "officia",
    "at",
    "in",
    "id",
    "quos",
    "reprehenderit",
    "numquam",
    "iste",
    "fugiat",
    "sit",
    "inventore",
    "beatae",
    "repellendus",
    "magnam",
    "recusandae",
    "quod",
    "explicabo",
    "doloremque",
    "aperiam",
    "consequatur",
    "asperiores",
    "commodi",
    "optio",
    "dolor",
    "labore",
    "temporibus",
    "repellat",
    "veniam",
    "architecto",
    "est",
    "esse",
    "mollitia",
    "nulla",
    "a",
    "similique",
    "eos",
    "alias",
    "dolore",
    "tenetur",
    "deleniti",
    "porro",
    "facere",
    "maxime",
    "corrupti",
];

/// Small deterministic generator (xorshift).
struct Rng(u64);

impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

/// `count` words of placeholder text in sentences; the first text (`seed` 0) starts with
/// "Lorem ipsum dolor sit amet".
pub fn lorem(count: usize, seed: usize) -> String {
    let count = count.clamp(1, 10_000);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (seed as u64).wrapping_mul(0x2545_F491_4F6C_DD1D));
    let mut words: Vec<String> = Vec::with_capacity(count);
    if seed == 0 {
        words.extend(COMMON.iter().take(count).map(|w| w.to_string()));
    }
    while words.len() < count {
        words.push(WORDS[rng.next(WORDS.len())].to_string());
    }
    // Sentences of 5..=15 words (shorter at the end), each capitalized, some with a comma.
    let mut out = String::new();
    let mut i = 0;
    while i < words.len() {
        let len = (5 + rng.next(11)).min(words.len() - i);
        let mut sentence = words[i..i + len].to_vec();
        if len >= 8 {
            let comma = 4 + rng.next(len - 5);
            sentence[comma].push(',');
        }
        let first = &mut sentence[0];
        *first = first[..1].to_uppercase() + &first[1..];
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&sentence.join(" "));
        out.push(if rng.next(10) == 0 { '?' } else { '.' });
        i += len;
    }
    out
}

/// `lorem`, `lorem10`, `lipsum5` → word count (30 without a number).
pub fn lorem_count(name: &str) -> Option<usize> {
    let lower = name.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("lorem")
        .or_else(|| lower.strip_prefix("lipsum"))?;
    if rest.is_empty() {
        return Some(30);
    }
    rest.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_words_and_starts_classically() {
        let t = lorem(10, 0);
        assert!(t.starts_with("Lorem ipsum dolor sit amet"), "{t}");
        assert_eq!(t.split_whitespace().count(), 10);
        assert!(t.ends_with('.') || t.ends_with('?'));
        assert_eq!(lorem(10, 0), t, "deterministic");
        let other = lorem(4, 3);
        assert_eq!(other.split_whitespace().count(), 4);
        assert!(!other.starts_with("Lorem"));
        assert!(other.chars().next().unwrap().is_uppercase());
    }

    #[test]
    fn parses_names() {
        assert_eq!(lorem_count("lorem"), Some(30));
        assert_eq!(lorem_count("lorem5"), Some(5));
        assert_eq!(lorem_count("lipsum12"), Some(12));
        assert_eq!(lorem_count("loremx"), None);
        assert_eq!(lorem_count("div"), None);
    }
}
