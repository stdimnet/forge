//! File types every Forge installation knows (PHPStorm's bundled file types). Plugins add
//! more, or replace one of these by contributing a file type with the same id, through
//! `[[plugin.contributions.fileTypes]]` in their manifests.

use crate::FileTypeSpec;

/// `(id, name, extensions, file names)`.
type Row = (
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

const ROWS: &[Row] = &[
    ("php", "PHP", &["php", "phtml", "inc"], &[]),
    ("blade", "Blade", &["blade.php"], &[]),
    (
        "json",
        "JSON",
        &[
            "json",
            "jsonc",
            "json5",
            "lock",
            "webmanifest",
            "geojson",
            "har",
        ],
        &[
            ".prettierrc",
            ".babelrc",
            ".eslintrc",
            "composer.lock",
            ".php-cs-fixer.cache",
        ],
    ),
    ("yaml", "YAML", &["yml", "yaml", "neon"], &[]),
    ("sql", "SQL", &["sql", "ddl"], &[]),
    ("markdown", "Markdown", &["md", "markdown"], &[]),
    (
        "javascript",
        "JavaScript",
        &["js", "mjs", "cjs", "es6"],
        &[],
    ),
    ("jsx", "JavaScript React", &["jsx"], &[]),
    ("typescript", "TypeScript", &["ts", "mts", "cts"], &[]),
    ("tsx", "TypeScript React", &["tsx"], &[]),
    ("css", "CSS", &["css", "pcss", "postcss"], &[]),
    ("scss", "SCSS", &["scss"], &[]),
    ("sass", "Sass", &["sass"], &[]),
    ("less", "Less", &["less"], &[]),
    ("stylus", "Stylus", &["styl", "stylus"], &[]),
    (
        "html",
        "HTML",
        &["html", "htm", "xhtml", "svelte", "astro"],
        &[],
    ),
    ("vue", "Vue", &["vue"], &[]),
    ("twig", "Twig", &["twig", "html.twig"], &[]),
    ("smarty", "Smarty", &["tpl", "smarty"], &[]),
    ("latte", "Latte", &["latte"], &[]),
    (
        "gotemplate",
        "Go Template",
        &["tmpl", "gotmpl", "gohtml"],
        &[],
    ),
    (
        "jinja",
        "Jinja2",
        &["j2", "jinja", "jinja2", "njk", "nunjucks"],
        &[],
    ),
    (
        "handlebars",
        "Handlebars",
        &["hbs", "handlebars", "mustache"],
        &[],
    ),
    ("velocity", "Velocity", &["vm", "vtl"], &[]),
    ("pug", "Pug", &["pug", "jade"], &[]),
    (
        "xml",
        "XML",
        &["xml", "xsd", "xsl", "svg", "dist", "xliff", "xlf"],
        &[],
    ),
    ("dotenv", "Dotenv", &["env"], &[".env*", "*.env"]),
    (
        "properties",
        "Properties",
        &["properties", "ini", "cfg", "conf", "editorconfig"],
        &[".editorconfig", ".gitattributes"],
    ),
    (
        "shell",
        "Shell Script",
        &["sh", "bash", "zsh"],
        &[".bashrc", ".zshrc", ".profile", "artisan.sh"],
    ),
    (
        "dockerfile",
        "Dockerfile",
        &["dockerfile"],
        &["dockerfile", "containerfile", "dockerfile.*"],
    ),
    ("toml", "TOML", &["toml"], &[]),
    ("nginx", "Nginx", &["nginx"], &["nginx.conf"]),
    ("lua", "Lua", &["lua"], &[]),
    ("diff", "Diff", &["diff", "patch"], &[]),
    ("python", "Python", &["py", "pyw", "pyi"], &[]),
    ("go", "Go", &["go"], &[]),
    ("rust", "Rust", &["rs"], &[]),
    ("java", "Java", &["java"], &[]),
    (
        "cpp",
        "C++",
        &["cpp", "cc", "cxx", "hpp", "hh", "hxx", "ino"],
        &[],
    ),
    ("c", "C", &["c", "h"], &[]),
    ("csharp", "C#", &["cs", "csx"], &[]),
    ("kotlin", "Kotlin", &["kt", "kts"], &[]),
    ("scala", "Scala", &["scala", "sc"], &[]),
    ("dart", "Dart", &["dart"], &[]),
    ("objc", "Objective-C", &["m", "mm"], &[]),
    ("swift", "Swift", &["swift"], &[]),
    (
        "ruby",
        "Ruby",
        &["rb", "rake", "gemspec", "ru"],
        &["gemfile", "rakefile", "vagrantfile"],
    ),
    ("perl", "Perl", &["pl", "pm", "t"], &[]),
    ("powershell", "PowerShell", &["ps1", "psm1", "psd1"], &[]),
    ("batch", "Batch", &["bat", "cmd"], &[]),
    (
        "groovy",
        "Groovy",
        &["groovy", "gradle", "gvy"],
        &["jenkinsfile"],
    ),
    ("haskell", "Haskell", &["hs", "lhs"], &[]),
    ("erlang", "Erlang", &["erl", "hrl"], &[]),
    ("elixir", "Elixir", &["ex", "exs"], &[]),
    ("clojure", "Clojure", &["clj", "cljs", "cljc", "edn"], &[]),
    ("r", "R", &["r"], &[]),
    ("julia", "Julia", &["jl"], &[]),
    ("elm", "Elm", &["elm"], &[]),
    ("crystal", "Crystal", &["cr"], &[]),
    ("ocaml", "OCaml", &["ml", "mli"], &[]),
    ("fsharp", "F#", &["fs", "fsi", "fsx"], &[]),
    ("coffeescript", "CoffeeScript", &["coffee"], &[]),
    ("lisp", "Common Lisp", &["lisp", "lsp", "cl"], &[]),
    ("scheme", "Scheme", &["scm", "ss", "rkt"], &[]),
    ("tcl", "Tcl", &["tcl"], &[]),
    ("vb", "Visual Basic", &["vb", "vbs"], &[]),
    ("pascal", "Pascal", &["pas", "pp"], &[]),
    ("fortran", "Fortran", &["f", "f90", "f95", "for"], &[]),
    ("verilog", "Verilog", &["v", "sv", "svh"], &[]),
    ("vhdl", "VHDL", &["vhd", "vhdl"], &[]),
    ("octave", "MATLAB/Octave", &["matlab", "oct"], &[]),
    ("asm", "Assembly", &["asm", "s"], &[]),
    ("latex", "LaTeX", &["tex", "sty", "cls", "bib"], &[]),
    ("textile", "Textile", &["textile"], &[]),
    ("protobuf", "Protocol Buffers", &["proto"], &[]),
    ("cmake", "CMake", &["cmake"], &["cmakelists.txt"]),
    ("gherkin", "Gherkin", &["feature"], &[]),
    ("http", "HTTP Request", &["http", "rest"], &[]),
    ("puppet", "Puppet", &["pp.puppet"], &[]),
    (
        "makefile",
        "Makefile",
        &["mk", "make"],
        &["makefile", "gnumakefile"],
    ),
    (
        "htaccess",
        "Apache Config",
        &[],
        &[".htaccess", "httpd.conf", "apache2.conf"],
    ),
    ("log", "Log", &["log"], &[]),
    (
        "ignore",
        "Ignore File",
        &[],
        &[".gitignore", ".dockerignore", ".npmignore"],
    ),
];

/// Template languages: their text between tags is highlighted in a data language.
const TEMPLATES: &[&str] = &[
    "twig",
    "smarty",
    "latte",
    "gotemplate",
    "jinja",
    "handlebars",
];

/// The bundled file types, in registration order.
pub fn builtin_file_types() -> Vec<FileTypeSpec> {
    let strings = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect();
    ROWS.iter()
        .map(|(id, name, extensions, file_names)| FileTypeSpec {
            id: id.to_string(),
            name: name.to_string(),
            extensions: strings(extensions),
            file_names: strings(file_names),
            template: TEMPLATES.contains(id),
            ..Default::default()
        })
        .collect()
}
