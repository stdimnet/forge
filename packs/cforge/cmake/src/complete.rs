//! Completion in `CMakeLists.txt` and `*.cmake` files: command names at the start of a
//! command, the keywords of the command the caret is in, and `${VARIABLE}` references (CMake's
//! variables plus the ones the file sets).

use serde::{Deserialize, Serialize};

/// `(command, signature, keywords)`.
pub const COMMANDS: &[(&str, &str, &[&str])] = &[
    ("add_compile_definitions", "add_compile_definitions(<definition>...)", &[]),
    ("add_compile_options", "add_compile_options(<option>...)", &[]),
    ("add_custom_command", "add_custom_command(OUTPUT output... COMMAND command...)", &["OUTPUT", "COMMAND", "DEPENDS", "BYPRODUCTS", "WORKING_DIRECTORY", "COMMENT", "VERBATIM", "TARGET", "PRE_BUILD", "PRE_LINK", "POST_BUILD", "USES_TERMINAL"]),
    ("add_custom_target", "add_custom_target(Name [ALL] [command...])", &["ALL", "COMMAND", "DEPENDS", "BYPRODUCTS", "WORKING_DIRECTORY", "COMMENT", "VERBATIM", "SOURCES", "USES_TERMINAL"]),
    ("add_definitions", "add_definitions(-DFOO -DBAR ...)", &[]),
    ("add_dependencies", "add_dependencies(<target> <target-dependency>...)", &[]),
    ("add_executable", "add_executable(<name> [WIN32] [MACOSX_BUNDLE] [EXCLUDE_FROM_ALL] [source...])", &["WIN32", "MACOSX_BUNDLE", "EXCLUDE_FROM_ALL", "IMPORTED", "ALIAS", "GLOBAL"]),
    ("add_library", "add_library(<name> [STATIC | SHARED | MODULE] [EXCLUDE_FROM_ALL] [<source>...])", &["STATIC", "SHARED", "MODULE", "OBJECT", "INTERFACE", "EXCLUDE_FROM_ALL", "IMPORTED", "ALIAS", "GLOBAL", "UNKNOWN"]),
    ("add_link_options", "add_link_options(<option>...)", &[]),
    ("add_subdirectory", "add_subdirectory(source_dir [binary_dir] [EXCLUDE_FROM_ALL] [SYSTEM])", &["EXCLUDE_FROM_ALL", "SYSTEM"]),
    ("add_test", "add_test(NAME <name> COMMAND <command> [<arg>...])", &["NAME", "COMMAND", "CONFIGURATIONS", "WORKING_DIRECTORY", "COMMAND_EXPAND_LISTS"]),
    ("cmake_minimum_required", "cmake_minimum_required(VERSION <min>[...<policy_max>] [FATAL_ERROR])", &["VERSION", "FATAL_ERROR"]),
    ("cmake_parse_arguments", "cmake_parse_arguments(<prefix> <options> <one_value_keywords> <multi_value_keywords> <args>...)", &["PARSE_ARGV"]),
    ("cmake_path", "cmake_path(GET <path-var> <component> <out-var>)", &["GET", "SET", "APPEND", "REMOVE_FILENAME", "REPLACE_EXTENSION", "NORMAL_PATH", "RELATIVE_PATH", "ABSOLUTE_PATH", "COMPARE", "HAS_ROOT_PATH", "IS_ABSOLUTE"]),
    ("cmake_policy", "cmake_policy(SET CMP<NNNN> NEW|OLD)", &["SET", "GET", "VERSION", "PUSH", "POP", "NEW", "OLD"]),
    ("configure_file", "configure_file(<input> <output> [COPYONLY] [ESCAPE_QUOTES] [@ONLY])", &["COPYONLY", "ESCAPE_QUOTES", "@ONLY", "NEWLINE_STYLE", "NO_SOURCE_PERMISSIONS"]),
    ("else", "else()", &[]),
    ("elseif", "elseif(<condition>)", &["AND", "OR", "NOT", "DEFINED", "EXISTS", "STREQUAL", "EQUAL", "LESS", "GREATER", "VERSION_LESS", "VERSION_GREATER_EQUAL", "MATCHES", "TARGET", "IN_LIST"]),
    ("enable_language", "enable_language(<lang>... [OPTIONAL])", &["C", "CXX", "CUDA", "ASM", "OPTIONAL"]),
    ("enable_testing", "enable_testing()", &[]),
    ("endforeach", "endforeach()", &[]),
    ("endfunction", "endfunction()", &[]),
    ("endif", "endif()", &[]),
    ("endmacro", "endmacro()", &[]),
    ("endwhile", "endwhile()", &[]),
    ("execute_process", "execute_process(COMMAND <cmd1> [<args>...] ...)", &["COMMAND", "WORKING_DIRECTORY", "RESULT_VARIABLE", "OUTPUT_VARIABLE", "ERROR_VARIABLE", "OUTPUT_STRIP_TRAILING_WHITESPACE", "COMMAND_ERROR_IS_FATAL", "TIMEOUT"]),
    ("file", "file(GLOB <variable> [LIST_DIRECTORIES true|false] [<globbing-expressions>...])", &["GLOB", "GLOB_RECURSE", "READ", "WRITE", "APPEND", "MAKE_DIRECTORY", "COPY", "REMOVE", "REMOVE_RECURSE", "DOWNLOAD", "CONFIGURE_DEPENDS", "RELATIVE", "TO_CMAKE_PATH", "REAL_PATH"]),
    ("find_library", "find_library(<VAR> name1 [path1 path2 ...])", &["NAMES", "HINTS", "PATHS", "PATH_SUFFIXES", "REQUIRED", "NO_DEFAULT_PATH"]),
    ("find_package", "find_package(<PackageName> [version] [EXACT] [QUIET] [REQUIRED] [COMPONENTS <components>...])", &["REQUIRED", "QUIET", "EXACT", "CONFIG", "MODULE", "COMPONENTS", "OPTIONAL_COMPONENTS", "NO_MODULE", "HINTS", "PATHS", "GLOBAL"]),
    ("find_path", "find_path(<VAR> name1 [path1 path2 ...])", &["NAMES", "HINTS", "PATHS", "PATH_SUFFIXES", "REQUIRED"]),
    ("find_program", "find_program(<VAR> name1 [path1 path2 ...])", &["NAMES", "HINTS", "PATHS", "PATH_SUFFIXES", "REQUIRED"]),
    ("foreach", "foreach(<loop_var> <items>)", &["IN", "LISTS", "ITEMS", "ZIP_LISTS", "RANGE"]),
    ("function", "function(<name> [<arg1> ...])", &[]),
    ("get_filename_component", "get_filename_component(<var> <FileName> <mode> [CACHE])", &["DIRECTORY", "NAME", "EXT", "NAME_WE", "LAST_EXT", "NAME_WLE", "PATH", "ABSOLUTE", "REALPATH", "BASE_DIR", "CACHE"]),
    ("get_property", "get_property(<variable> <GLOBAL | DIRECTORY | TARGET | SOURCE | TEST | CACHE> PROPERTY <name>)", &["GLOBAL", "DIRECTORY", "TARGET", "SOURCE", "TEST", "CACHE", "PROPERTY", "SET", "DEFINED"]),
    ("get_target_property", "get_target_property(<VAR> target property)", &[]),
    ("if", "if(<condition>)", &["AND", "OR", "NOT", "DEFINED", "EXISTS", "STREQUAL", "EQUAL", "LESS", "GREATER", "VERSION_LESS", "VERSION_GREATER_EQUAL", "MATCHES", "TARGET", "IN_LIST", "COMMAND", "POLICY"]),
    ("include", "include(<file|module> [OPTIONAL] [RESULT_VARIABLE <var>])", &["OPTIONAL", "RESULT_VARIABLE", "NO_POLICY_SCOPE", "CTest", "GoogleTest", "FetchContent", "GNUInstallDirs", "CMakePackageConfigHelpers", "CheckCXXCompilerFlag"]),
    ("include_directories", "include_directories([AFTER|BEFORE] [SYSTEM] dir1 [dir2 ...])", &["AFTER", "BEFORE", "SYSTEM"]),
    ("install", "install(TARGETS <target>... [...])", &["TARGETS", "FILES", "PROGRAMS", "DIRECTORY", "EXPORT", "DESTINATION", "RUNTIME", "LIBRARY", "ARCHIVE", "INCLUDES", "COMPONENT", "NAMESPACE"]),
    ("list", "list(APPEND <list> [<element>...])", &["LENGTH", "GET", "JOIN", "SUBLIST", "FIND", "APPEND", "FILTER", "INSERT", "PREPEND", "POP_BACK", "POP_FRONT", "REMOVE_ITEM", "REMOVE_DUPLICATES", "TRANSFORM", "REVERSE", "SORT"]),
    ("macro", "macro(<name> [<arg1> ...])", &[]),
    ("math", "math(EXPR <variable> \"<expression>\")", &["EXPR", "OUTPUT_FORMAT", "HEXADECIMAL", "DECIMAL"]),
    ("message", "message([<mode>] \"message text\" ...)", &["STATUS", "WARNING", "AUTHOR_WARNING", "SEND_ERROR", "FATAL_ERROR", "DEPRECATION", "NOTICE", "VERBOSE", "DEBUG", "TRACE", "CHECK_START", "CHECK_PASS", "CHECK_FAIL"]),
    ("option", "option(<variable> \"<help_text>\" [value])", &["ON", "OFF"]),
    ("project", "project(<PROJECT-NAME> [VERSION <major>[.<minor>]] [DESCRIPTION <desc>] [LANGUAGES <language-name>...])", &["VERSION", "DESCRIPTION", "HOMEPAGE_URL", "LANGUAGES", "C", "CXX", "CUDA", "ASM"]),
    ("return", "return()", &["PROPAGATE"]),
    ("set", "set(<variable> <value>... [PARENT_SCOPE])", &["CACHE", "PARENT_SCOPE", "FORCE", "BOOL", "FILEPATH", "PATH", "STRING", "INTERNAL"]),
    ("set_property", "set_property(<GLOBAL | DIRECTORY | TARGET | SOURCE | TEST | CACHE> [APPEND] PROPERTY <name> [<value>...])", &["GLOBAL", "DIRECTORY", "TARGET", "SOURCE", "TEST", "CACHE", "APPEND", "APPEND_STRING", "PROPERTY"]),
    ("set_target_properties", "set_target_properties(target1 target2 ... PROPERTIES prop1 value1 ...)", &["PROPERTIES", "OUTPUT_NAME", "CXX_STANDARD", "CXX_STANDARD_REQUIRED", "CXX_EXTENSIONS", "POSITION_INDEPENDENT_CODE", "RUNTIME_OUTPUT_DIRECTORY", "VERSION", "SOVERSION"]),
    ("set_tests_properties", "set_tests_properties(test1 [test2...] PROPERTIES prop1 value1 ...)", &["PROPERTIES", "LABELS", "TIMEOUT", "WILL_FAIL", "ENVIRONMENT", "DEPENDS", "WORKING_DIRECTORY"]),
    ("source_group", "source_group(<name> [FILES <src>...] [REGULAR_EXPRESSION <regex>])", &["FILES", "REGULAR_EXPRESSION", "TREE", "PREFIX"]),
    ("string", "string(REPLACE <match_string> <replace_string> <output_variable> <input> [<input>...])", &["REPLACE", "REGEX", "MATCH", "MATCHALL", "APPEND", "PREPEND", "CONCAT", "JOIN", "TOLOWER", "TOUPPER", "LENGTH", "SUBSTRING", "STRIP", "FIND", "COMPARE", "TIMESTAMP", "MAKE_C_IDENTIFIER"]),
    ("target_compile_definitions", "target_compile_definitions(<target> <INTERFACE|PUBLIC|PRIVATE> [items1...])", &["INTERFACE", "PUBLIC", "PRIVATE"]),
    ("target_compile_features", "target_compile_features(<target> <PRIVATE|PUBLIC|INTERFACE> <feature> [...])", &["INTERFACE", "PUBLIC", "PRIVATE", "cxx_std_11", "cxx_std_14", "cxx_std_17", "cxx_std_20", "cxx_std_23", "c_std_11", "c_std_17"]),
    ("target_compile_options", "target_compile_options(<target> [BEFORE] <INTERFACE|PUBLIC|PRIVATE> [items1...])", &["BEFORE", "INTERFACE", "PUBLIC", "PRIVATE"]),
    ("target_include_directories", "target_include_directories(<target> [SYSTEM] [AFTER|BEFORE] <INTERFACE|PUBLIC|PRIVATE> [items1...])", &["SYSTEM", "AFTER", "BEFORE", "INTERFACE", "PUBLIC", "PRIVATE"]),
    ("target_link_directories", "target_link_directories(<target> [BEFORE] <INTERFACE|PUBLIC|PRIVATE> [items1...])", &["BEFORE", "INTERFACE", "PUBLIC", "PRIVATE"]),
    ("target_link_libraries", "target_link_libraries(<target> <PRIVATE|PUBLIC|INTERFACE> <item>...)", &["PRIVATE", "PUBLIC", "INTERFACE", "LINK_PUBLIC", "LINK_PRIVATE", "LINK_INTERFACE_LIBRARIES", "debug", "optimized", "general"]),
    ("target_link_options", "target_link_options(<target> [BEFORE] <INTERFACE|PUBLIC|PRIVATE> [items1...])", &["BEFORE", "INTERFACE", "PUBLIC", "PRIVATE"]),
    ("target_precompile_headers", "target_precompile_headers(<target> <INTERFACE|PUBLIC|PRIVATE> [header1...])", &["INTERFACE", "PUBLIC", "PRIVATE", "REUSE_FROM"]),
    ("target_sources", "target_sources(<target> <INTERFACE|PUBLIC|PRIVATE> [items1...])", &["INTERFACE", "PUBLIC", "PRIVATE", "FILE_SET", "TYPE", "HEADERS", "CXX_MODULES", "BASE_DIRS", "FILES"]),
    ("unset", "unset(<variable> [CACHE | PARENT_SCOPE])", &["CACHE", "PARENT_SCOPE"]),
    ("while", "while(<condition>)", &["AND", "OR", "NOT", "DEFINED", "LESS", "GREATER", "EQUAL"]),
    // Modules projects use most.
    ("FetchContent_Declare", "FetchContent_Declare(<name> <contentOptions>...)", &["GIT_REPOSITORY", "GIT_TAG", "GIT_SHALLOW", "URL", "URL_HASH", "SOURCE_SUBDIR", "FIND_PACKAGE_ARGS", "OVERRIDE_FIND_PACKAGE", "DOWNLOAD_EXTRACT_TIMESTAMP"]),
    ("FetchContent_MakeAvailable", "FetchContent_MakeAvailable(<name1> [<name2>...])", &[]),
    ("gtest_discover_tests", "gtest_discover_tests(target [EXTRA_ARGS arg1...] [WORKING_DIRECTORY dir] ...)", &["EXTRA_ARGS", "WORKING_DIRECTORY", "TEST_PREFIX", "TEST_SUFFIX", "TEST_FILTER", "PROPERTIES", "TEST_LIST", "DISCOVERY_TIMEOUT", "DISCOVERY_MODE", "POST_BUILD", "PRE_TEST"]),
    ("catch_discover_tests", "catch_discover_tests(target [TEST_SPEC arg1...] [EXTRA_ARGS arg1...] ...)", &["TEST_SPEC", "EXTRA_ARGS", "WORKING_DIRECTORY", "TEST_PREFIX", "TEST_SUFFIX", "PROPERTIES", "REPORTER", "OUTPUT_DIR", "DISCOVERY_MODE"]),
];

/// Variables CMake defines.
pub const VARIABLES: &[&str] = &[
    "CMAKE_BINARY_DIR",
    "CMAKE_BUILD_TYPE",
    "CMAKE_C_COMPILER",
    "CMAKE_C_FLAGS",
    "CMAKE_C_STANDARD",
    "CMAKE_CONFIGURATION_TYPES",
    "CMAKE_CURRENT_BINARY_DIR",
    "CMAKE_CURRENT_LIST_DIR",
    "CMAKE_CURRENT_LIST_FILE",
    "CMAKE_CURRENT_SOURCE_DIR",
    "CMAKE_CXX_COMPILER",
    "CMAKE_CXX_COMPILER_ID",
    "CMAKE_CXX_COMPILER_VERSION",
    "CMAKE_CXX_EXTENSIONS",
    "CMAKE_CXX_FLAGS",
    "CMAKE_CXX_FLAGS_DEBUG",
    "CMAKE_CXX_FLAGS_RELEASE",
    "CMAKE_CXX_STANDARD",
    "CMAKE_CXX_STANDARD_REQUIRED",
    "CMAKE_EXPORT_COMPILE_COMMANDS",
    "CMAKE_GENERATOR",
    "CMAKE_HOST_SYSTEM_NAME",
    "CMAKE_INSTALL_PREFIX",
    "CMAKE_MODULE_PATH",
    "CMAKE_PREFIX_PATH",
    "CMAKE_PROJECT_NAME",
    "CMAKE_RUNTIME_OUTPUT_DIRECTORY",
    "CMAKE_SOURCE_DIR",
    "CMAKE_SYSTEM_NAME",
    "CMAKE_SYSTEM_PROCESSOR",
    "CMAKE_TOOLCHAIN_FILE",
    "CMAKE_VERSION",
    "MSVC",
    "PROJECT_BINARY_DIR",
    "PROJECT_NAME",
    "PROJECT_SOURCE_DIR",
    "PROJECT_VERSION",
    "UNIX",
    "WIN32",
    "APPLE",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Command,
    Keyword,
    Variable,
    /// A vcpkg port (`vcpkg.json`).
    Package,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub label: String,
    pub kind: ItemKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// Text to insert (`add_executable()` places the caret inside the parentheses).
    pub apply: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    /// Byte offset where the completed word starts.
    pub from: usize,
    pub items: Vec<Item>,
}

/// Variables the text sets (`set(NAME`, `option(NAME`) and its project's `<name>_*` ones.
fn own_variables(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim_start();
        for cmd in ["set(", "option(", "set (", "option ("] {
            if let Some(rest) = l.strip_prefix(cmd) {
                let name: String = rest
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() && !out.contains(&name) {
                    out.push(name);
                }
            }
        }
    }
    out
}

/// The command whose argument list contains `offset`, if any (`None` at the top level).
fn enclosing_command(text: &str, offset: usize) -> Option<String> {
    let mut depth = 0i32;
    let mut command_start: Option<String> = None;
    let mut word = String::new();
    let mut in_string = false;
    let mut in_comment = false;
    for (i, c) in text.char_indices() {
        if i >= offset {
            break;
        }
        if in_comment {
            if c == '\n' {
                in_comment = false;
            }
            continue;
        }
        if in_string {
            if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '#' => in_comment = true,
            '"' => in_string = true,
            '(' => {
                if depth == 0 {
                    command_start = Some(word.clone());
                }
                depth += 1;
                word.clear();
            }
            ')' => {
                depth = (depth - 1).max(0);
                if depth == 0 {
                    command_start = None;
                }
                word.clear();
            }
            c if c.is_ascii_alphanumeric() || c == '_' => word.push(c),
            _ => word.clear(),
        }
    }
    if depth > 0 {
        command_start
    } else {
        None
    }
}

/// Completion at `offset` (byte offset) of a CMake file.
pub fn complete(text: &str, offset: usize) -> Option<Completion> {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let word_start = before
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map(|i| i + 1)
        .unwrap_or(0);
    let prefix = &before[word_start..];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    if before[line_start..].trim_start().starts_with('#') {
        return None;
    }
    let matches = |label: &str| {
        prefix.is_empty()
            || label
                .to_ascii_lowercase()
                .starts_with(&prefix.to_ascii_lowercase())
    };
    // `${NAME` or `$ENV{NAME`.
    if before[..word_start].ends_with("${") || before[..word_start].ends_with("ENV{") {
        let mut names: Vec<String> = own_variables(text);
        names.extend(VARIABLES.iter().map(|v| v.to_string()));
        names.dedup();
        let items = names
            .into_iter()
            .filter(|n| matches(n))
            .map(|n| Item {
                label: n.clone(),
                kind: ItemKind::Variable,
                detail: String::new(),
                apply: n,
            })
            .collect();
        return Some(Completion {
            from: word_start,
            items,
        });
    }
    match enclosing_command(text, word_start) {
        Some(cmd) => {
            let keywords = COMMANDS
                .iter()
                .find(|(c, ..)| c.eq_ignore_ascii_case(&cmd))
                .map(|(_, _, k)| *k)
                .unwrap_or(&[]);
            if keywords.is_empty()
                || (prefix.is_empty()
                    && !before.ends_with(char::is_whitespace)
                    && !before.ends_with('('))
            {
                return None;
            }
            Some(Completion {
                from: word_start,
                items: keywords
                    .iter()
                    .filter(|k| matches(k))
                    .map(|k| Item {
                        label: k.to_string(),
                        kind: ItemKind::Keyword,
                        detail: cmd.to_ascii_lowercase(),
                        apply: k.to_string(),
                    })
                    .collect(),
            })
        }
        None => {
            if prefix.is_empty() {
                return None;
            }
            Some(Completion {
                from: word_start,
                items: COMMANDS
                    .iter()
                    .filter(|(c, ..)| matches(c))
                    .map(|(c, sig, _)| Item {
                        label: c.to_string(),
                        kind: ItemKind::Command,
                        detail: sig.to_string(),
                        apply: format!("{c}()"),
                    })
                    .collect(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(c: &Option<Completion>) -> Vec<String> {
        c.as_ref()
            .map(|c| c.items.iter().map(|i| i.label.clone()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn commands_at_the_top_level() {
        let text = "project(shapes)\ntarget_li";
        let c = complete(text, text.len());
        assert_eq!(
            labels(&c),
            [
                "target_link_directories",
                "target_link_libraries",
                "target_link_options"
            ]
        );
        let c = c.unwrap();
        assert_eq!(c.from, 16);
        assert_eq!(c.items[1].apply, "target_link_libraries()");
        assert!(c.items[1]
            .detail
            .starts_with("target_link_libraries(<target>"));
        assert!(complete("# add_", 6).is_none());
    }

    #[test]
    fn keywords_inside_a_command() {
        let text = "target_link_libraries(shapes_app PR";
        assert_eq!(labels(&complete(text, text.len())), ["PRIVATE"]);
        let text = "add_library(shapes ";
        let all = labels(&complete(text, text.len()));
        assert!(all.contains(&"STATIC".to_string()) && all.contains(&"SHARED".to_string()));
        let text = "find_package(GTest \"x(\" REQ";
        assert_eq!(labels(&complete(text, text.len())), ["REQUIRED"]);
        // Closed commands are the top level again.
        let text = "add_library(a STATIC a.cpp)\nadd_ex";
        assert_eq!(labels(&complete(text, text.len())), ["add_executable"]);
    }

    #[test]
    fn variables() {
        let text = "set(SHAPES_SOURCES a.cpp)\noption(SHAPES_TESTS \"tests\" ON)\nadd_library(shapes ${SHA";
        assert_eq!(
            labels(&complete(text, text.len())),
            ["SHAPES_SOURCES", "SHAPES_TESTS"]
        );
        let text = "message(${CMAKE_CXX_COMPILER_";
        assert_eq!(
            labels(&complete(text, text.len())),
            ["CMAKE_CXX_COMPILER_ID", "CMAKE_CXX_COMPILER_VERSION"]
        );
    }
}
