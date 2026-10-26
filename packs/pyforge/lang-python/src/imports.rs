//! Import statements of a Python file (`import a.b as c, d`, `from x.y import z`), found by
//! a small tokenizer that skips strings, comments and bracketed continuations. Relative
//! imports (`from . import x`) are left out: they never name a package.

use serde::{Deserialize, Serialize};

/// One imported module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Import {
    /// The dotted module name (`google.cloud.storage`).
    pub module: String,
    /// Its first segment (`google`).
    pub top: String,
    /// 0-based line and UTF-16 columns of the module name.
    pub line: u32,
    pub start: u32,
    pub end: u32,
    /// Inside a `try:` block (an optional dependency with a fallback).
    pub guarded: bool,
}

/// The imports of `text`.
pub fn scan(text: &str) -> Vec<Import> {
    let mut out = Vec::new();
    let mut in_triple: Option<&str> = None;
    // Indentation of open `try:` blocks.
    let mut tries: Vec<usize> = Vec::new();
    let mut depth = 0i32; // open brackets across lines
    for (no, line) in text.lines().enumerate() {
        // Close triple-quoted strings started on earlier lines.
        let mut rest_start = 0;
        if let Some(q) = in_triple {
            match line.find(q) {
                Some(p) => {
                    in_triple = None;
                    rest_start = p + 3;
                }
                None => continue,
            }
        }
        let code = code_part(&line[rest_start..], &mut in_triple, &mut depth);
        if rest_start > 0 || code.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        tries.retain(|&t| t < indent);
        let stmt = line.trim_start();
        if stmt.starts_with("try:") {
            tries.push(indent);
            continue;
        }
        let guarded = !tries.is_empty();
        // Statements after `;` are rare in imports; only the first one is read.
        let stmt_end = code.len().min(line.len());
        let stmt_text = &line[indent..stmt_end];
        let mut push = |name: &str, byte: usize| {
            if name.is_empty() || name.starts_with('.') {
                return;
            }
            let top = name.split('.').next().unwrap_or(name).to_string();
            let start = line[..byte].encode_utf16().count() as u32;
            out.push(Import {
                module: name.to_string(),
                top,
                line: no as u32,
                start,
                end: start + name.encode_utf16().count() as u32,
                guarded,
            });
        };
        if let Some(rest) = stmt_text.strip_prefix("import ") {
            let mut offset = indent + "import ".len();
            for part in rest.split(',') {
                let lead = part.len() - part.trim_start().len();
                let name = part.split_whitespace().next().unwrap_or("");
                let name = name.trim_matches(['(', ')']);
                push(name, offset + lead);
                offset += part.len() + 1;
            }
        } else if let Some(rest) = stmt_text.strip_prefix("from ") {
            let lead = rest.len() - rest.trim_start().len();
            let name = rest.split_whitespace().next().unwrap_or("");
            if rest.contains(" import") {
                push(name, indent + "from ".len() + lead);
            }
        }
    }
    out
}

/// The code of `line` before a comment, tracking strings and brackets. Opens `in_triple`
/// when a triple-quoted string continues on the next line.
fn code_part<'a>(line: &'a str, in_triple: &mut Option<&'static str>, depth: &mut i32) -> &'a str {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'#' => return &line[..i],
            q @ (b'"' | b'\'') => {
                let triple: &'static str = if q == b'"' { "\"\"\"" } else { "'''" };
                if line[i..].starts_with(triple) {
                    match line[i + 3..].find(triple) {
                        Some(p) => i += 3 + p + 3,
                        None => {
                            *in_triple = Some(triple);
                            return &line[..i];
                        }
                    }
                    continue;
                }
                i += 1;
                while i < b.len() && b[i] != q {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
                continue;
            }
            b'(' | b'[' | b'{' => *depth += 1,
            b')' | b']' | b'}' => *depth -= 1,
            _ => {}
        }
        i += 1;
    }
    line
}

/// Python's standard library modules (`sys.stdlib_module_names` of 3.8–3.13, top level).
pub const STDLIB: &[&str] = &[
    "__future__",
    "_thread",
    "abc",
    "aifc",
    "argparse",
    "array",
    "ast",
    "asynchat",
    "asyncio",
    "asyncore",
    "atexit",
    "audioop",
    "base64",
    "bdb",
    "binascii",
    "bisect",
    "builtins",
    "bz2",
    "calendar",
    "cgi",
    "cgitb",
    "chunk",
    "cmath",
    "cmd",
    "code",
    "codecs",
    "codeop",
    "collections",
    "colorsys",
    "compileall",
    "concurrent",
    "configparser",
    "contextlib",
    "contextvars",
    "copy",
    "copyreg",
    "cProfile",
    "crypt",
    "csv",
    "ctypes",
    "curses",
    "dataclasses",
    "datetime",
    "dbm",
    "decimal",
    "difflib",
    "dis",
    "distutils",
    "doctest",
    "email",
    "encodings",
    "ensurepip",
    "enum",
    "errno",
    "faulthandler",
    "fcntl",
    "filecmp",
    "fileinput",
    "fnmatch",
    "fractions",
    "ftplib",
    "functools",
    "gc",
    "getopt",
    "getpass",
    "gettext",
    "glob",
    "graphlib",
    "grp",
    "gzip",
    "hashlib",
    "heapq",
    "hmac",
    "html",
    "http",
    "idlelib",
    "imaplib",
    "imghdr",
    "imp",
    "importlib",
    "inspect",
    "io",
    "ipaddress",
    "itertools",
    "json",
    "keyword",
    "lib2to3",
    "linecache",
    "locale",
    "logging",
    "lzma",
    "mailbox",
    "mailcap",
    "marshal",
    "math",
    "mimetypes",
    "mmap",
    "modulefinder",
    "msilib",
    "msvcrt",
    "multiprocessing",
    "netrc",
    "nis",
    "nntplib",
    "ntpath",
    "numbers",
    "opcode",
    "operator",
    "optparse",
    "os",
    "ossaudiodev",
    "pathlib",
    "pdb",
    "pickle",
    "pickletools",
    "pipes",
    "pkgutil",
    "platform",
    "plistlib",
    "poplib",
    "posix",
    "posixpath",
    "pprint",
    "profile",
    "pstats",
    "pty",
    "pwd",
    "py_compile",
    "pyclbr",
    "pydoc",
    "queue",
    "quopri",
    "random",
    "re",
    "readline",
    "reprlib",
    "resource",
    "rlcompleter",
    "runpy",
    "sched",
    "secrets",
    "select",
    "selectors",
    "shelve",
    "shlex",
    "shutil",
    "signal",
    "site",
    "smtpd",
    "smtplib",
    "sndhdr",
    "socket",
    "socketserver",
    "spwd",
    "sqlite3",
    "sre_compile",
    "sre_constants",
    "sre_parse",
    "ssl",
    "stat",
    "statistics",
    "string",
    "stringprep",
    "struct",
    "subprocess",
    "sunau",
    "symtable",
    "sys",
    "sysconfig",
    "syslog",
    "tabnanny",
    "tarfile",
    "telnetlib",
    "tempfile",
    "termios",
    "textwrap",
    "threading",
    "time",
    "timeit",
    "tkinter",
    "token",
    "tokenize",
    "tomllib",
    "trace",
    "traceback",
    "tracemalloc",
    "tty",
    "turtle",
    "turtledemo",
    "types",
    "typing",
    "unicodedata",
    "unittest",
    "urllib",
    "uu",
    "uuid",
    "venv",
    "warnings",
    "wave",
    "weakref",
    "webbrowser",
    "winreg",
    "winsound",
    "wsgiref",
    "xdrlib",
    "xml",
    "xmlrpc",
    "zipapp",
    "zipfile",
    "zipimport",
    "zlib",
    "zoneinfo",
];

pub fn is_stdlib(top: &str) -> bool {
    STDLIB.contains(&top)
}

/// Modules whose distribution has another name: (import name, distribution).
pub const KNOWN_DISTRIBUTIONS: &[(&str, &str)] = &[
    ("yaml", "PyYAML"),
    ("PIL", "Pillow"),
    ("cv2", "opencv-python"),
    ("sklearn", "scikit-learn"),
    ("skimage", "scikit-image"),
    ("bs4", "beautifulsoup4"),
    ("dateutil", "python-dateutil"),
    ("dotenv", "python-dotenv"),
    ("jose", "python-jose"),
    ("jwt", "PyJWT"),
    ("magic", "python-magic"),
    ("multipart", "python-multipart"),
    ("OpenSSL", "pyOpenSSL"),
    ("Crypto", "pycryptodome"),
    ("serial", "pyserial"),
    ("usb", "pyusb"),
    ("zmq", "pyzmq"),
    ("git", "GitPython"),
    ("github", "PyGithub"),
    ("google.protobuf", "protobuf"),
    ("psycopg2", "psycopg2-binary"),
    ("MySQLdb", "mysqlclient"),
    ("ldap", "python-ldap"),
    ("docx", "python-docx"),
    ("pptx", "python-pptx"),
    ("slugify", "python-slugify"),
    ("telegram", "python-telegram-bot"),
    ("attr", "attrs"),
    ("pkg_resources", "setuptools"),
    ("_pytest", "pytest"),
    ("win32api", "pywin32"),
    ("wx", "wxPython"),
    ("fitz", "PyMuPDF"),
    ("Levenshtein", "python-Levenshtein"),
    ("socks", "PySocks"),
    ("rest_framework", "djangorestframework"),
    ("corsheaders", "django-cors-headers"),
    ("debug_toolbar", "django-debug-toolbar"),
    ("environ", "django-environ"),
    ("flask_sqlalchemy", "Flask-SQLAlchemy"),
    ("flask_login", "Flask-Login"),
    ("flask_cors", "Flask-Cors"),
    ("sqlalchemy", "SQLAlchemy"),
    ("markdown", "Markdown"),
    ("jinja2", "Jinja2"),
    ("markupsafe", "MarkupSafe"),
];

/// The distribution that usually provides import `module` (by its dotted name, then its top
/// level), or the top-level name itself.
pub fn distribution_for(module: &str) -> String {
    let top = module.split('.').next().unwrap_or(module);
    KNOWN_DISTRIBUTIONS
        .iter()
        .find(|(m, _)| {
            *m == module || module.starts_with(&format!("{m}.")) || (!m.contains('.') && *m == top)
        })
        .map(|(_, d)| d.to_string())
        .unwrap_or_else(|| top.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_imports() {
        let src = r#"import os, sys
import numpy as np, requests.adapters
from django.db import models  # comment with import x
from . import local
from .models import User
text = """
import not_an_import
"""
s = "from fake import y"
try:
    import ujson as json
except ImportError:
    import json
def f():
    from yaml import safe_load
"#;
        let got: Vec<(String, u32, u32, bool)> = scan(src)
            .into_iter()
            .map(|i| (i.module, i.line, i.start, i.guarded))
            .collect();
        assert_eq!(
            got,
            vec![
                ("os".into(), 0, 7, false),
                ("sys".into(), 0, 11, false),
                ("numpy".into(), 1, 7, false),
                ("requests.adapters".into(), 1, 20, false),
                ("django.db".into(), 2, 5, false),
                ("ujson".into(), 10, 11, true),
                ("json".into(), 12, 11, false),
                ("yaml".into(), 14, 9, false),
            ]
        );
    }

    #[test]
    fn maps_modules_to_distributions() {
        assert_eq!(distribution_for("yaml"), "PyYAML");
        assert_eq!(distribution_for("PIL.Image"), "Pillow");
        assert_eq!(distribution_for("google.protobuf.message"), "protobuf");
        assert_eq!(distribution_for("requests.adapters"), "requests");
        assert!(is_stdlib("asyncio") && !is_stdlib("requests"));
    }
}
