//! Where languages are injected: `// language=` comments, heredoc labels and the built-in
//! rules (calls, tagged templates, assignments and the text of the string itself).

use std::sync::OnceLock;

use regex_lite::Regex;
use serde::Serialize;

use crate::languages::{self, Language};
use crate::lexer::{is_word, Comment, Host, Lexed, Literal, LiteralKind};

/// A built-in injection rule, listed in Settings | Editor | Language Injections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub id: &'static str,
    pub name: &'static str,
    /// Injected language id.
    pub language: &'static str,
    pub host: Host,
    /// Where it applies, e.g. "PDO::query(), mysqli_query(), …".
    pub places: &'static str,
}

/// The rule as the settings page shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleInfo {
    pub id: String,
    pub name: String,
    pub language: String,
    pub language_label: String,
    pub host: Host,
    pub host_label: String,
    pub places: String,
}

pub const RULES: &[Rule] = &[
    Rule {
        id: "php-sql-keywords",
        name: "SQL statements in strings",
        language: "sql",
        host: Host::Php,
        places: "Strings starting with SELECT … FROM, INSERT INTO, UPDATE … SET, DELETE FROM, WITH, CREATE/ALTER/DROP TABLE, …",
    },
    Rule {
        id: "php-sql-calls",
        name: "PDO, mysqli, pg_* and Doctrine DBAL queries",
        language: "sql",
        host: Host::Php,
        places: "PDO::query()/prepare()/exec(), mysqli_query(), pg_query(), Connection::executeQuery()/fetchAllAssociative(), createNativeQuery()",
    },
    Rule {
        id: "php-sql-laravel",
        name: "Laravel DB facade and raw expressions",
        language: "sql",
        host: Host::Php,
        places: "DB::select()/statement()/unprepared()/raw(), selectRaw(), whereRaw(), havingRaw(), orderByRaw(), groupByRaw()",
    },
    Rule {
        id: "php-sql-wpdb",
        name: "WordPress $wpdb queries",
        language: "sql",
        host: Host::Php,
        places: "$wpdb->query()/prepare()/get_results()/get_row()/get_var()/get_col()",
    },
    Rule {
        id: "php-heredoc",
        name: "Heredoc and nowdoc labels",
        language: "sql",
        host: Host::Php,
        places: "<<<SQL, <<<HTML, <<<CSS, <<<JSON, <<<JS, <<<XML, <<<REGEXP",
    },
    Rule {
        id: "php-html",
        name: "HTML in strings starting with a tag",
        language: "html",
        host: Host::Php,
        places: "Strings starting with <tag …>",
    },
    Rule {
        id: "php-json",
        name: "JSON in json_decode()",
        language: "json",
        host: Host::Php,
        places: "json_decode()",
    },
    Rule {
        id: "php-regexp",
        name: "Regular expressions in preg_* functions",
        language: "regexp",
        host: Host::Php,
        places: "preg_match(), preg_match_all(), preg_replace(), preg_replace_callback(), preg_split(), preg_grep(), preg_filter()",
    },
    Rule {
        id: "php-css",
        name: "CSS in wp_add_inline_style()",
        language: "css",
        host: Host::Php,
        places: "wp_add_inline_style()",
    },
    Rule {
        id: "js-sql-keywords",
        name: "SQL statements in strings",
        language: "sql",
        host: Host::Js,
        places: "Strings and templates starting with SELECT … FROM, INSERT INTO, UPDATE … SET, DELETE FROM, WITH, …",
    },
    Rule {
        id: "js-sql-calls",
        name: "Database client queries and sql`…` templates",
        language: "sql",
        host: Host::Js,
        places: "db.query()/execute()/prepare()/raw(), pool.query(), knex.raw(), sql`…`, $queryRaw`…`, $executeRaw`…`",
    },
    Rule {
        id: "js-html",
        name: "HTML strings and html`…` templates",
        language: "html",
        host: Host::Js,
        places: "Strings starting with <tag …>, html`…`, innerHTML/outerHTML assignments, insertAdjacentHTML(), document.write()",
    },
    Rule {
        id: "js-css",
        name: "CSS-in-JS templates",
        language: "css",
        host: Host::Js,
        places: "css`…`, styled.div`…`, styled(Component)`…`, keyframes`…`, createGlobalStyle`…`, style.cssText, insertRule()",
    },
    Rule {
        id: "js-json",
        name: "JSON in JSON.parse()",
        language: "json",
        host: Host::Js,
        places: "JSON.parse()",
    },
    Rule {
        id: "js-regexp",
        name: "Regular expressions in RegExp()",
        language: "regexp",
        host: Host::Js,
        places: "new RegExp(), RegExp()",
    },
];

pub fn rule_infos() -> Vec<RuleInfo> {
    RULES
        .iter()
        .map(|r| {
            let lang = languages::by_id(r.language).map_or(r.language, |l| l.label);
            RuleInfo {
                id: r.id.into(),
                name: r.name.into(),
                language: r.language.into(),
                // Heredoc labels name their own language.
                language_label: if r.id == "php-heredoc" {
                    "(from the label)".into()
                } else {
                    lang.into()
                },
                host: r.host,
                host_label: r.host.label().into(),
                places: r.places.into(),
            }
        })
        .collect()
}

/// How an injection was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// A `// language=…` comment (or `@lang` in a doc comment).
    Comment,
    /// A built-in rule (heredoc labels included).
    Rule,
}

/// An injection found in a source (byte offsets).
#[derive(Debug, Clone)]
pub struct Found {
    pub language: &'static Language,
    pub source: Source,
    pub rule: Option<&'static str>,
    /// Indices into `Lexed::literals`: the literal and the literals concatenated to it.
    pub literals: Vec<usize>,
    /// The injected text ranges.
    pub ranges: Vec<(usize, usize)>,
    /// The `language=` comment.
    pub comment: Option<Comment>,
}

/// A call around a literal: `object->name(…, literal, …)`.
#[derive(Debug, Clone, Copy)]
struct Call<'a> {
    name: &'a str,
    /// `$pdo`, `DB`, `$this->db`, `JSON`; empty for functions.
    object: &'a str,
    /// Index of the argument the literal is in.
    arg: usize,
}

struct Ctx<'a> {
    src: &'a str,
    b: &'a [u8],
    mask: &'a [bool],
    comments: &'a [Comment],
    host: Host,
}

impl<'a> Ctx<'a> {
    fn skip_ws_back(&self, mut i: usize) -> usize {
        while i > 0 && (self.b[i - 1].is_ascii_whitespace() || self.mask[i - 1]) {
            i -= 1;
        }
        i
    }

    fn skip_ws(&self, mut i: usize) -> usize {
        while i < self.b.len() && (self.b[i].is_ascii_whitespace() || self.is_comment_at(i)) {
            i += 1;
        }
        i
    }

    fn is_comment_at(&self, i: usize) -> bool {
        let n = self.comments.partition_point(|c| c.end <= i);
        self.comments.get(n).is_some_and(|c| c.start <= i)
    }

    /// Member operators for chains like `$this->db` or `styled.div`.
    fn member_ops(&self) -> &'static [&'static str] {
        match self.host {
            Host::Php => &["?->", "->", "::"],
            Host::Js => &["?.", "."],
        }
    }

    /// Start of the member chain ending at `end` (`$this->db`, `styled(Button)`, `Prisma.sql`).
    fn chain_start(&self, end: usize) -> usize {
        let b = self.b;
        let mut i = end;
        loop {
            let mut j = i;
            while j > 0 && b[j - 1].is_ascii_whitespace() {
                j -= 1;
            }
            if j > 0 && (b[j - 1] == b')' || b[j - 1] == b']') {
                let mut depth = 0;
                let mut k = j;
                while k > 0 {
                    k -= 1;
                    if self.mask[k] {
                        continue;
                    }
                    match b[k] {
                        b')' | b']' => depth += 1,
                        b'(' | b'[' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                j = k;
                // A call: its callee comes next.
                let mut n = j;
                while n > 0 && (is_word(b[n - 1]) || b[n - 1] == b'$' || b[n - 1] == b'\\') {
                    n -= 1;
                }
                j = n;
            } else if j > 0 && (is_word(b[j - 1]) || b[j - 1] == b'$') {
                while j > 0 && (is_word(b[j - 1]) || b[j - 1] == b'$' || b[j - 1] == b'\\') {
                    j -= 1;
                }
            } else {
                return i;
            }
            i = j;
            match self
                .member_ops()
                .iter()
                .find(|op| self.src[..i].ends_with(**op))
            {
                Some(op) => i -= op.len(),
                None => return i,
            }
        }
    }

    /// The call whose argument list the literal at `start` is in.
    fn call_of(&self, start: usize) -> Option<Call<'a>> {
        let b = self.b;
        let mut depth = 0usize;
        let mut arg = 0;
        let mut i = start;
        let open = loop {
            if i == 0 || start - i > 8000 {
                return None;
            }
            i -= 1;
            if self.mask[i] {
                continue;
            }
            match b[i] {
                b')' | b']' | b'}' => depth += 1,
                b'(' | b'[' | b'{' if depth > 0 => depth -= 1,
                b'(' => break i,
                b'[' | b'{' | b';' => return None,
                b',' if depth == 0 => arg += 1,
                _ => {}
            }
        };
        let name_end = self.skip_ws_back(open);
        let mut name_start = name_end;
        while name_start > 0 && (is_word(b[name_start - 1]) || b[name_start - 1] == b'$') {
            name_start -= 1;
        }
        if name_start == name_end {
            return None;
        }
        let name = &self.src[name_start..name_end];
        let object = match self
            .member_ops()
            .iter()
            .find(|op| self.src[..name_start].ends_with(**op))
        {
            Some(op) => {
                let end = name_start - op.len();
                self.src[self.chain_start(end)..end].trim()
            }
            None => "",
        };
        Some(Call { name, object, arg })
    }

    /// The tag of a JavaScript tagged template (`sql`, `styled.div`, `styled(Button)`).
    fn tag_of(&self, lit: &Literal) -> Option<&'a str> {
        if lit.kind != LiteralKind::JsTemplate {
            return None;
        }
        let end = self.skip_ws_back(lit.start);
        if end == 0 || !(is_word(self.b[end - 1]) || matches!(self.b[end - 1], b'$' | b')')) {
            return None;
        }
        let tag = self.src[self.chain_start(end)..end].trim();
        let keyword = [
            "return", "yield", "await", "typeof", "case", "in", "of", "else", "do",
        ];
        (!tag.is_empty() && !keyword.contains(&tag)).then_some(tag)
    }

    /// The property a literal is assigned to (`el.innerHTML = '…'`, `style.cssText += '…'`).
    fn assigned_to(&self, start: usize) -> Option<&'a str> {
        let b = self.b;
        let mut eq = self.skip_ws_back(start);
        if eq == 0 || b[eq - 1] != b'=' {
            return None;
        }
        eq -= 1;
        if eq > 0 && b[eq - 1] == b'+' {
            eq -= 1;
        } else if eq > 0 && matches!(b[eq - 1], b'=' | b'!' | b'<' | b'>') {
            return None;
        }
        let end = self.skip_ws_back(eq);
        let target = self.src[self.chain_start(end)..end].trim();
        (!target.is_empty()).then_some(target)
    }

    /// The literals concatenated to the literal `first` (PHP `.`, JavaScript `+`).
    fn concatenated(&self, lits: &[Literal], first: usize) -> Vec<usize> {
        let b = self.b;
        let op = match self.host {
            Host::Php => b'.',
            Host::Js => b'+',
        };
        let mut group = vec![first];
        let mut i = lits[first].end;
        loop {
            let j = self.skip_ws(i);
            let is_op = j < b.len()
                && b[j] == op
                && !matches!(b.get(j + 1), Some(&c) if c == b'=' || c == op || c.is_ascii_digit());
            if !is_op {
                break;
            }
            let k = self.skip_ws(j + 1);
            if let Ok(n) = lits.binary_search_by_key(&k, |l| l.start) {
                group.push(n);
                i = lits[n].end;
                continue;
            }
            // An operand that is not a literal: up to the next operator at the same depth.
            let mut depth = 0usize;
            let mut m = k;
            while m < b.len() {
                if self.mask[m] {
                    m += 1;
                    continue;
                }
                match b[m] {
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' if depth == 0 => break,
                    b')' | b']' | b'}' => depth -= 1,
                    b',' | b';' | b'?' | b':' if depth == 0 => break,
                    // `->` and `?->` never contain `.`, but numbers do.
                    c if c == op
                        && depth == 0
                        && !(op == b'.' && b.get(m + 1).is_some_and(|d| d.is_ascii_digit())) =>
                    {
                        break
                    }
                    _ => {}
                }
                m += 1;
            }
            if m >= b.len() || b[m] != op || m == k {
                break;
            }
            i = m;
        }
        group
    }
}

fn sql_start() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?is)^\s*(?:\(\s*)?(?:SELECT\s.*\bFROM\b|INSERT\s+(?:IGNORE\s+)?INTO\s|REPLACE\s+INTO\s|UPDATE\s+\S+\s+SET\s|DELETE\s+FROM\s|WITH\s+(?:RECURSIVE\s+)?\w+(?:\s*\([^)]*\))?\s+AS\s*\(|CREATE\s+(?:TEMPORARY\s+|OR\s+REPLACE\s+|UNIQUE\s+)?(?:TABLE|INDEX|VIEW|DATABASE|SCHEMA|TRIGGER|PROCEDURE|FUNCTION)\s|ALTER\s+TABLE\s|DROP\s+(?:TABLE|INDEX|VIEW|DATABASE|SCHEMA)\s|TRUNCATE\s+(?:TABLE\s+)?\w|SHOW\s+(?:FULL\s+)?(?:TABLES|COLUMNS|DATABASES|INDEX|CREATE)\b)",
        )
        .expect("SQL pattern")
    })
}

/// `SELECT` written in capitals also counts without a `FROM` (`SELECT 1`, `SELECT NOW()`).
fn upper_select() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\s*SELECT\s+\S").expect("SELECT pattern"))
}

fn looks_like_sql(text: &str) -> bool {
    sql_start().is_match(text) || upper_select().is_match(text)
}

fn looks_like_html(text: &str) -> bool {
    let t = text.trim_start();
    let mut chars = t.chars();
    chars.next() == Some('<')
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '!' || c == '/')
        && t.contains('>')
}

fn language_comment() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?:^|[\s*/#])(?:language\s*=\s*|@lang\s+)([A-Za-z][\w+#.-]*)")
            .expect("comment pattern")
    })
}

/// The language a comment names: `// language=SQL`, `/* language=JSON */`, `# language=HTML`
/// or `/** @lang SQL */`.
pub fn comment_language(text: &str) -> Option<&'static Language> {
    let caps = language_comment().captures(text)?;
    languages::language(caps.get(1)?.as_str())
}

/// The language a heredoc or nowdoc label names (`<<<SQL`, `<<<'JSON'`).
fn label_language(label: &str) -> Option<&'static Language> {
    languages::language(label)
}

/// The value of a group of literals, for content rules (interpolations kept as written).
fn group_text(src: &str, lits: &[Literal], group: &[usize]) -> String {
    group
        .iter()
        .map(|&i| &src[lits[i].content_start..lits[i].content_end])
        .collect::<Vec<_>>()
        .join("")
}

/// `(content of a PHP pattern between its delimiters)`: `/\d+/i` → `\d+`.
pub fn regex_body(src: &str, lit: &Literal) -> Option<(usize, usize)> {
    let s = &src[lit.content_start..lit.content_end];
    let d = s.chars().next()?;
    if d.is_alphanumeric() || d == '\\' || d.is_whitespace() {
        return None;
    }
    let close = match d {
        '(' => ')',
        '{' => '}',
        '[' => ']',
        '<' => '>',
        c => c,
    };
    let end = s.rfind(close).filter(|&p| p > 0)?;
    if !s[end + close.len_utf8()..]
        .chars()
        .all(|c| c.is_ascii_alphabetic())
    {
        return None;
    }
    Some((lit.content_start + d.len_utf8(), lit.content_start + end))
}

fn lower_in(name: &str, list: &[&str]) -> bool {
    list.iter().any(|n| n.eq_ignore_ascii_case(name))
}

fn last_segment(chain: &str) -> &str {
    let tail = chain
        .rsplit(['.', '>', ':', '('])
        .next()
        .unwrap_or(chain)
        .trim_end_matches(')');
    tail.trim()
}

/// A database-looking object: `$pdo`, `$this->db`, `$connection`, `pool`, `knex`.
fn is_db_object(object: &str) -> bool {
    const WORDS: &[&str] = &[
        "pdo",
        "db",
        "dbh",
        "dbal",
        "conn",
        "connection",
        "mysql",
        "mysqli",
        "database",
        "sqlite",
        "sqlite3",
        "pool",
        "knex",
        "sequelize",
        "pg",
        "tx",
        "trx",
        "sql",
        "wpdb",
    ];
    // Split `$readDbConnection` / `read_db` into words.
    let seg = last_segment(object);
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for ch in seg.chars() {
        if !ch.is_ascii_alphanumeric() {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if ch.is_ascii_uppercase() && prev_lower && !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        cur.push(ch.to_ascii_lowercase());
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words.iter().any(|w| WORDS.contains(&w.as_str()))
}

/// Rule for a literal by the call it is an argument of.
fn php_call_rule(call: &Call) -> Option<(&'static str, &'static str)> {
    let name = call.name;
    let obj = call.object;
    let a = call.arg;
    if obj.is_empty() {
        let sql = match name.to_ascii_lowercase().as_str() {
            "mysqli_query" | "mysqli_prepare" | "mysqli_real_query" | "mysqli_multi_query" => {
                a == 1
            }
            "pg_query" | "pg_send_query" => a <= 1,
            "pg_query_params" | "pg_send_query_params" => a == 1,
            "pg_prepare" | "pg_send_prepare" => a == 2,
            _ => false,
        };
        if sql {
            return Some(("php-sql-calls", "sql"));
        }
        if name.eq_ignore_ascii_case("json_decode") && a == 0 {
            return Some(("php-json", "json"));
        }
        if a == 0
            && lower_in(
                name,
                &[
                    "preg_match",
                    "preg_match_all",
                    "preg_replace",
                    "preg_replace_callback",
                    "preg_split",
                    "preg_grep",
                    "preg_filter",
                ],
            )
        {
            return Some(("php-regexp", "regexp"));
        }
        if name.eq_ignore_ascii_case("wp_add_inline_style") && a == 1 {
            return Some(("php-css", "css"));
        }
        return None;
    }
    if a != 0 {
        return None;
    }
    let facade = obj == "DB" || obj.ends_with("\\DB") || obj.starts_with("DB::");
    if facade
        && lower_in(
            name,
            &[
                "select",
                "selectOne",
                "selectResultSets",
                "insert",
                "update",
                "delete",
                "statement",
                "affectingStatement",
                "unprepared",
                "raw",
                "scalar",
                "cursor",
            ],
        )
    {
        return Some(("php-sql-laravel", "sql"));
    }
    if lower_in(
        name,
        &[
            "selectRaw",
            "whereRaw",
            "orWhereRaw",
            "havingRaw",
            "orHavingRaw",
            "orderByRaw",
            "groupByRaw",
            "fromRaw",
        ],
    ) {
        return Some(("php-sql-laravel", "sql"));
    }
    if last_segment(obj).eq_ignore_ascii_case("$wpdb") || obj.eq_ignore_ascii_case("wpdb") {
        if lower_in(
            name,
            &[
                "query",
                "prepare",
                "get_results",
                "get_row",
                "get_var",
                "get_col",
            ],
        ) {
            return Some(("php-sql-wpdb", "sql"));
        }
        return None;
    }
    let dbal = lower_in(
        name,
        &[
            "executeQuery",
            "executeStatement",
            "executeUpdate",
            "fetchAllAssociative",
            "fetchAssociative",
            "fetchAllNumeric",
            "fetchNumeric",
            "fetchOne",
            "fetchFirstColumn",
            "fetchAllKeyValue",
            "fetchAllAssociativeIndexed",
            "iterateAssociative",
            "iterateNumeric",
            "createNativeQuery",
        ],
    );
    if dbal || (lower_in(name, &["query", "prepare", "exec"]) && is_db_object(obj)) {
        return Some(("php-sql-calls", "sql"));
    }
    None
}

fn js_rule(ctx: &Ctx, lit: &Literal) -> Option<(&'static str, &'static str)> {
    if let Some(tag) = ctx.tag_of(lit) {
        let last = last_segment(tag);
        if ["sql", "SQL", "$queryRaw", "$executeRaw"].contains(&last) {
            return Some(("js-sql-calls", "sql"));
        }
        if last == "html" {
            return Some(("js-html", "html"));
        }
        if ["css", "keyframes", "createGlobalStyle", "injectGlobal"].contains(&last)
            || tag.starts_with("styled.")
            || tag.starts_with("styled(")
        {
            return Some(("js-css", "css"));
        }
        return None;
    }
    if let Some(target) = ctx.assigned_to(lit.start) {
        let last = last_segment(target);
        if last == "innerHTML" || last == "outerHTML" {
            return Some(("js-html", "html"));
        }
        if last == "cssText" {
            return Some(("js-css", "css"));
        }
    }
    let call = ctx.call_of(lit.start)?;
    match (call.name, call.object, call.arg) {
        ("parse", "JSON", 0) => Some(("js-json", "json")),
        ("RegExp", "", 0) => Some(("js-regexp", "regexp")),
        ("insertAdjacentHTML", _, 1) => Some(("js-html", "html")),
        ("write" | "writeln", "document", 0) => Some(("js-html", "html")),
        ("insertRule", _, 0) => Some(("js-css", "css")),
        ("query" | "execute" | "prepare" | "raw" | "exec" | "unsafe", obj, 0)
            if is_db_object(obj) =>
        {
            Some(("js-sql-calls", "sql"))
        }
        _ => None,
    }
}

/// Calls whose strings belong to another injection (Doctrine DQL).
fn excluded_call(call: &Call) -> bool {
    call.name == "createQuery"
}

/// All injections of a source, in source order.
pub fn detect(src: &str, host: Host, lexed: &Lexed, disabled: &[String]) -> Vec<Found> {
    let lits = &lexed.literals;
    let ctx = Ctx {
        src,
        b: src.as_bytes(),
        mask: &lexed.mask,
        comments: &lexed.comments,
        host,
    };
    let enabled = |id: &str| !disabled.iter().any(|d| d == id);

    // Group concatenated literals; nested template literals stay separate.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut taken = vec![false; lits.len()];
    let mut outer_end = 0;
    for i in 0..lits.len() {
        // Literals inside a template's `${…}` are never injection hosts themselves.
        if lits[i].start < outer_end {
            continue;
        }
        outer_end = outer_end.max(lits[i].end);
        if taken[i] {
            continue;
        }
        let g = ctx.concatenated(lits, i);
        for &n in &g {
            taken[n] = true;
        }
        groups.push(g);
    }

    // `language=` comments apply to the first literal after them in the same statement.
    let mut by_comment: Vec<Option<(Comment, &'static Language)>> = vec![None; groups.len()];
    for c in &lexed.comments {
        let Some(lang) = comment_language(&src[c.start..c.end]) else {
            continue;
        };
        let Some(g) = groups.iter().position(|g| lits[g[0]].start >= c.end) else {
            continue;
        };
        let between = &ctx.b[c.end..lits[groups[g][0]].start];
        let blocked = between
            .iter()
            .enumerate()
            .any(|(k, ch)| !ctx.mask[c.end + k] && matches!(ch, b';' | b'{' | b'}'))
            || src[c.end..lits[groups[g][0]].start].contains("?>");
        if !blocked {
            by_comment[g] = Some((*c, lang));
        }
    }

    let mut out = Vec::new();
    for (gi, group) in groups.iter().enumerate() {
        let first = &lits[group[0]];
        let found = |language: &'static Language, source, rule, comment| Found {
            language,
            source,
            rule,
            literals: group.clone(),
            ranges: Vec::new(),
            comment,
        };
        let mut hit = if let Some((c, lang)) = by_comment[gi] {
            Some(found(lang, Source::Comment, None, Some(c)))
        } else {
            None
        };
        if hit.is_none() && group.len() == 1 {
            if let Some(label) = &first.label {
                if let Some(lang) = label_language(label) {
                    if enabled("php-heredoc") {
                        hit = Some(found(lang, Source::Rule, Some("php-heredoc"), None));
                    }
                }
            }
        }
        if hit.is_none() && first.label.as_deref() != Some("DQL") {
            let call = ctx.call_of(first.start);
            let excluded = host == Host::Php && call.as_ref().is_some_and(excluded_call);
            let by_place = match host {
                Host::Php => call.as_ref().and_then(php_call_rule),
                Host::Js => js_rule(&ctx, first),
            };
            let rule = by_place.filter(|(id, _)| enabled(id)).or_else(|| {
                if excluded {
                    return None;
                }
                let text = group_text(src, lits, group);
                let (sql, html) = match host {
                    Host::Php => ("php-sql-keywords", "php-html"),
                    Host::Js => ("js-sql-keywords", "js-html"),
                };
                if enabled(sql) && looks_like_sql(&text) {
                    Some((sql, "sql"))
                } else if enabled(html) && looks_like_html(&text) {
                    Some((html, "html"))
                } else {
                    None
                }
            });
            if let Some((id, lang)) = rule {
                let rule = RULES.iter().find(|r| r.id == id).map(|r| r.id);
                if let Some(lang) = languages::by_id(lang) {
                    hit = Some(found(lang, Source::Rule, rule, None));
                }
            }
        }
        if let Some(mut f) = hit {
            f.ranges = ranges_of(src, lits, &f);
            out.push(f);
        }
    }
    out
}

/// The injected ranges: the text pieces of each literal; PHP patterns without delimiters.
fn ranges_of(src: &str, lits: &[Literal], f: &Found) -> Vec<(usize, usize)> {
    let clip = (f.language.id == "regexp" && f.literals.len() == 1)
        .then(|| {
            let lit = &lits[f.literals[0]];
            matches!(lit.kind, LiteralKind::PhpSingle | LiteralKind::PhpDouble)
                .then(|| regex_body(src, lit))
                .flatten()
        })
        .flatten();
    let mut out = Vec::new();
    for &i in &f.literals {
        for &(s, e) in &lits[i].pieces {
            let (s, e) = match clip {
                Some((cs, ce)) => (s.max(cs), e.min(ce)),
                None => (s, e),
            };
            if e > s {
                out.push((s, e));
            }
        }
    }
    out
}

/// The editable range of a literal in an injection: its content, without a PHP pattern's
/// delimiters.
pub fn fragment_range(src: &str, lit: &Literal, language: &Language) -> (usize, usize) {
    if language.id == "regexp"
        && matches!(lit.kind, LiteralKind::PhpSingle | LiteralKind::PhpDouble)
    {
        if let Some(r) = regex_body(src, lit) {
            return r;
        }
    }
    (lit.content_start, lit.content_end)
}
