//! Variable expressions of PHPStorm's "Edit Template Variables" dialog: string literals,
//! references to other variables and function calls such as `camelCase(NAME)`,
//! `className()` or `date("yyyy-MM-dd")`.

use std::collections::HashMap;

use crate::context::CaretInfo;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Str(String),
    Var(String),
    Call(String, Vec<Expr>),
}

/// Functions offered for expressions, with a short description (settings page help).
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "camelCase(String)",
        "Converts the string to camel case: my-text-file → myTextFile",
    ),
    ("capitalize(String)", "Capitalizes the first letter"),
    (
        "capitalizeAndUnderscore(String)",
        "Converts to upper case with underscores: myText → MY_TEXT",
    ),
    ("className()", "Name of the class at the caret"),
    ("clipboard()", "Contents of the clipboard"),
    ("complete()", "Leaves the field empty for code completion"),
    ("concat(expressions...)", "Joins the values"),
    ("date(format)", "Current date, e.g. date(\"yyyy-MM-dd\")"),
    ("decapitalize(String)", "Lower-cases the first letter"),
    ("enum(values...)", "The first of the given values"),
    ("fileName()", "Name of the current file"),
    (
        "fileNameWithoutExtension()",
        "Name of the current file without extension",
    ),
    ("functionName()", "Name of the function at the caret"),
    ("lowercaseAndDash(String)", "myTextFile → my-text-file"),
    ("methodName()", "Name of the method at the caret"),
    ("namespace()", "PHP namespace of the file"),
    ("snakeCase(String)", "myTextFile → my_text_file"),
    ("spaceSeparated(String)", "myTextFile → my text file"),
    (
        "substringBefore(String, delimiter)",
        "The part before the delimiter",
    ),
    (
        "suggestVariableName(collection)",
        "A singular name for an element of the collection",
    ),
    ("time(format)", "Current time, e.g. time(\"HH:mm\")"),
    (
        "underscoresToCamelCase(String)",
        "my_text_file → myTextFile",
    ),
    ("underscoresToSpaces(String)", "my_text_file → my text file"),
    ("user()", "Name of the current user"),
    (
        "variableOfType(type)",
        "A variable of the given type used before the caret",
    ),
];

/// Parse an expression; `None` for empty or invalid text.
pub fn parse(text: &str) -> Option<Expr> {
    let mut p = Parser {
        s: text.as_bytes(),
        src: text,
        i: 0,
    };
    let e = p.expr()?;
    p.ws();
    (p.i == p.s.len()).then_some(e)
}

struct Parser<'a> {
    s: &'a [u8],
    src: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn expr(&mut self) -> Option<Expr> {
        self.ws();
        let c = *self.s.get(self.i)?;
        if c == b'"' || c == b'\'' {
            self.i += 1;
            let mut out = String::new();
            while self.i < self.s.len() && self.s[self.i] != c {
                if self.s[self.i] == b'\\' && self.i + 1 < self.s.len() {
                    self.i += 1;
                }
                let ch = self.src[self.i..].chars().next()?;
                out.push(ch);
                self.i += ch.len_utf8();
            }
            self.i += 1;
            return (self.i <= self.s.len()).then_some(Expr::Str(out));
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = self.i;
            while self.i < self.s.len()
                && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_')
            {
                self.i += 1;
            }
            let name = self.src[start..self.i].to_string();
            self.ws();
            if self.s.get(self.i) != Some(&b'(') {
                return Some(Expr::Var(name));
            }
            self.i += 1;
            let mut args = Vec::new();
            loop {
                self.ws();
                match self.s.get(self.i) {
                    Some(b')') => {
                        self.i += 1;
                        return Some(Expr::Call(name, args));
                    }
                    Some(b',') if !args.is_empty() => self.i += 1,
                    Some(_) => args.push(self.expr()?),
                    None => return None,
                }
            }
        }
        None
    }
}

/// What expressions can see besides other variables.
#[derive(Debug, Clone, Default)]
pub struct EvalEnv<'a> {
    pub caret: Option<&'a CaretInfo>,
    pub file_name: &'a str,
    /// Milliseconds since the Unix epoch and the local time zone offset (UTC + minutes).
    pub now_ms: f64,
    pub tz_offset_minutes: i32,
    pub clipboard: &'a str,
    pub user: &'a str,
}

/// Evaluate an expression with the values of the variables computed so far.
pub fn eval(e: &Expr, env: &EvalEnv, vars: &HashMap<String, String>) -> String {
    match e {
        Expr::Str(s) => s.clone(),
        Expr::Var(name) => vars.get(name).cloned().unwrap_or_default(),
        Expr::Call(name, args) => {
            let arg = |i: usize| args.get(i).map(|a| eval(a, env, vars)).unwrap_or_default();
            let caret = env.caret;
            match name.as_str() {
                "className" | "phpClassName" | "jsClassName" => {
                    caret.and_then(|c| c.class_name.clone()).unwrap_or_default()
                }
                "methodName" | "functionName" | "phpFunctionName" | "jsMethodName" => caret
                    .and_then(|c| c.function_name.clone())
                    .unwrap_or_default(),
                "namespace" | "phpNamespace" => {
                    caret.and_then(|c| c.namespace.clone()).unwrap_or_default()
                }
                "fileName" => env.file_name.to_string(),
                "fileNameWithoutExtension" => match env.file_name.split_once('.') {
                    Some((stem, _)) if !stem.is_empty() => stem.to_string(),
                    _ => env.file_name.to_string(),
                },
                "clipboard" => env.clipboard.to_string(),
                "user" => env.user.to_string(),
                "date" => format_date(
                    env,
                    &if args.is_empty() {
                        "yyyy-MM-dd".into()
                    } else {
                        arg(0)
                    },
                ),
                "time" => format_date(
                    env,
                    &if args.is_empty() {
                        "HH:mm".into()
                    } else {
                        arg(0)
                    },
                ),
                "complete" | "completeSmart" => String::new(),
                "enum" => arg(0),
                "concat" => (0..args.len()).map(arg).collect(),
                "substringBefore" => {
                    let s = arg(0);
                    s.split_once(arg(1).as_str())
                        .map_or(s.clone(), |(a, _)| a.to_string())
                }
                "camelCase" | "underscoresToCamelCase" => camel_case(&arg(0)),
                "capitalize" => capitalize(&arg(0)),
                "decapitalize" => decapitalize(&arg(0)),
                "snakeCase" => words(&arg(0)).join("_"),
                "capitalizeAndUnderscore" => words(&arg(0)).join("_").to_uppercase(),
                "lowercaseAndDash" => words(&arg(0)).join("-"),
                "spaceSeparated" | "underscoresToSpaces" => words(&arg(0)).join(" "),
                "suggestVariableName" | "phpSuggestVariableName" => suggest_variable_name(&arg(0)),
                "variableOfType" | "phpVar" | "arrayVariable" => variable_of_type(caret, &arg(0)),
                _ => String::new(),
            }
        }
    }
}

/// Lower-case words of an identifier or phrase (`myTextFile`, `my-text file` → my, text, file).
fn words(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        let boundary = c.is_uppercase()
            && !cur.is_empty()
            && (chars[i - 1].is_lowercase() || chars.get(i + 1).is_some_and(|n| n.is_lowercase()));
        if boundary {
            out.push(std::mem::take(&mut cur));
        }
        cur.extend(c.to_lowercase());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn decapitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_lowercase().chain(c).collect())
        .unwrap_or_default()
}

fn camel_case(s: &str) -> String {
    let w = words(s);
    w.iter()
        .enumerate()
        .map(|(i, w)| if i == 0 { w.clone() } else { capitalize(w) })
        .collect()
}

/// A singular element name for a collection: `$users` → `user`, `$this->categories` → `category`.
fn suggest_variable_name(collection: &str) -> String {
    let base: String = collection
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_string();
    let singular = if let Some(stem) = base.strip_suffix("ies") {
        format!("{stem}y")
    } else if base.ends_with("sses")
        || base.ends_with("xes")
        || base.ends_with("ches")
        || base.ends_with("shes")
    {
        base[..base.len() - 2].to_string()
    } else if base.ends_with('s') && !base.ends_with("ss") {
        base[..base.len() - 1].to_string()
    } else if base.to_ascii_lowercase().ends_with("list") && base.len() > 4 {
        base[..base.len() - 4].to_string()
    } else {
        String::new()
    };
    // Nothing singular to offer: the template's default value applies.
    if singular.is_empty() || singular == base {
        String::new()
    } else {
        decapitalize(&singular)
    }
}

/// The most recent variable that looks like the type (`array`: a plural name).
fn variable_of_type(caret: Option<&CaretInfo>, ty: &str) -> String {
    let Some(vars) = caret.map(|c| &c.variables) else {
        return String::new();
    };
    let plural = |v: &&String| v.ends_with('s') && !v.ends_with("ss");
    let wants_collection = matches!(
        ty.to_ascii_lowercase().as_str(),
        "array" | "iterable" | "list" | "collection"
    );
    let pick = if wants_collection {
        vars.iter().find(plural)
    } else {
        None
    };
    pick.or(vars.first()).cloned().unwrap_or_default()
}

/// Civil date (year, month 1-12, day 1-31) of days since 1970-01-01.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = [
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
];

/// Java `SimpleDateFormat` patterns, as PHPStorm's `date()` and `time()` use them.
fn format_date(env: &EvalEnv, pattern: &str) -> String {
    let local_ms = env.now_ms as i64 + i64::from(env.tz_offset_minutes) * 60_000;
    let days = local_ms.div_euclid(86_400_000);
    let ms_of_day = local_ms.rem_euclid(86_400_000);
    let (year, month, day) = civil(days);
    let (hour, minute, second) = (
        ms_of_day / 3_600_000,
        ms_of_day / 60_000 % 60,
        ms_of_day / 1000 % 60,
    );
    let weekday = DAYS[days.rem_euclid(7) as usize];
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            let end = chars[i + 1..]
                .iter()
                .position(|&q| q == '\'')
                .map_or(chars.len(), |p| i + 1 + p);
            if end == i + 1 {
                out.push('\'');
            }
            out.extend(&chars[i + 1..end]);
            i = end + 1;
            continue;
        }
        let run = chars[i..].iter().take_while(|&&x| x == c).count();
        i += run;
        let num = |v: i64| format!("{v:0run$}");
        match c {
            'y' if run == 2 => out.push_str(&format!("{:02}", year.rem_euclid(100))),
            'y' => out.push_str(&num(year)),
            'M' if run >= 4 => out.push_str(MONTHS[month as usize - 1]),
            'M' if run == 3 => out.push_str(&MONTHS[month as usize - 1][..3]),
            'M' => out.push_str(&num(i64::from(month))),
            'd' => out.push_str(&num(i64::from(day))),
            'H' => out.push_str(&num(hour)),
            'h' => out.push_str(&num(if hour % 12 == 0 { 12 } else { hour % 12 })),
            'm' => out.push_str(&num(minute)),
            's' => out.push_str(&num(second)),
            'a' => out.push_str(if hour < 12 { "AM" } else { "PM" }),
            'E' if run >= 4 => out.push_str(weekday),
            'E' => out.push_str(&weekday[..3]),
            c if c.is_ascii_alphabetic() => {}
            c => out.extend(std::iter::repeat_n(c, run)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str, env: &EvalEnv) -> String {
        let vars = HashMap::from([("NAME".to_string(), "user-profile".to_string())]);
        eval(&parse(src).unwrap_or_else(|| panic!("{src}")), env, &vars)
    }

    #[test]
    fn parses_expressions() {
        assert_eq!(parse("\"a\\\"b\""), Some(Expr::Str("a\"b".into())));
        assert_eq!(
            parse(" camelCase( NAME ) "),
            Some(Expr::Call(
                "camelCase".into(),
                vec![Expr::Var("NAME".into())]
            ))
        );
        assert_eq!(
            parse("concat(\"a\", date())"),
            Some(Expr::Call(
                "concat".into(),
                vec![Expr::Str("a".into()), Expr::Call("date".into(), vec![])]
            ))
        );
        assert_eq!(parse(""), None);
        assert_eq!(parse("f(,)"), None);
        assert_eq!(parse("a b"), None);
    }

    #[test]
    fn converts_case() {
        let env = EvalEnv::default();
        assert_eq!(run("camelCase(NAME)", &env), "userProfile");
        assert_eq!(run("capitalize(camelCase(NAME))", &env), "UserProfile");
        assert_eq!(run("snakeCase(\"myTextFile\")", &env), "my_text_file");
        assert_eq!(
            run("capitalizeAndUnderscore(\"myTextFile\")", &env),
            "MY_TEXT_FILE"
        );
        assert_eq!(run("lowercaseAndDash(\"HTMLParser\")", &env), "html-parser");
        assert_eq!(run("spaceSeparated(\"my_text\")", &env), "my text");
        assert_eq!(run("decapitalize(\"Abc\")", &env), "abc");
        assert_eq!(run("substringBefore(\"a.b.c\", \".\")", &env), "a");
        assert_eq!(run("enum(\"x\", \"y\")", &env), "x");
        assert_eq!(run("concat(NAME, \"!\")", &env), "user-profile!");
        assert_eq!(run("unknownFunction()", &env), "");
    }

    #[test]
    fn reads_the_environment() {
        let caret = CaretInfo {
            class_name: Some("User".into()),
            function_name: Some("save".into()),
            namespace: Some("App\\Models".into()),
            variables: vec!["$total".into(), "$orders".into()],
            ..CaretInfo::default()
        };
        let env = EvalEnv {
            caret: Some(&caret),
            file_name: "User.php",
            // 2026-10-22 13:05:09 UTC, shown in UTC+2.
            now_ms: 1_792_674_309_000.0,
            tz_offset_minutes: 120,
            clipboard: "copied",
            user: "marlon",
        };
        assert_eq!(run("className()", &env), "User");
        assert_eq!(run("methodName()", &env), "save");
        assert_eq!(run("namespace()", &env), "App\\Models");
        assert_eq!(run("fileNameWithoutExtension()", &env), "User");
        assert_eq!(run("clipboard()", &env), "copied");
        assert_eq!(run("user()", &env), "marlon");
        assert_eq!(
            run("date(\"yyyy-MM-dd HH:mm:ss\")", &env),
            "2026-10-22 15:05:09"
        );
        assert_eq!(
            run("date(\"EEE, d MMM yy 'at' h a\")", &env),
            "Thu, 22 Oct 26 at 3 PM"
        );
        assert_eq!(run("time()", &env), "15:05");
        assert_eq!(run("variableOfType(\"array\")", &env), "$orders");
        assert_eq!(run("variableOfType(\"\")", &env), "$total");
        assert_eq!(
            run("suggestVariableName(variableOfType(\"array\"))", &env),
            "order"
        );
    }

    #[test]
    fn suggests_singular_names() {
        assert_eq!(suggest_variable_name("$categories"), "category");
        assert_eq!(suggest_variable_name("$this->boxes"), "box");
        assert_eq!(suggest_variable_name("$userList"), "user");
        assert_eq!(suggest_variable_name("$data"), "");
        assert_eq!(suggest_variable_name(""), "");
    }
}
