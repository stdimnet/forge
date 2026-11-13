//! Sanitizers of a CMake profile (CLion's "Sanitizers" in the profile): AddressSanitizer,
//! UndefinedBehaviorSanitizer, ThreadSanitizer, MemorySanitizer and LeakSanitizer as
//! `-fsanitize=…` compiler and linker flags, the combinations compilers reject, and the
//! runtime options that make reports readable.

use std::collections::BTreeMap;

/// `(id, label)` of the sanitizers a profile can enable, in display order.
pub const SANITIZERS: [(&str, &str); 5] = [
    ("address", "AddressSanitizer"),
    ("undefined", "UndefinedBehaviorSanitizer"),
    ("thread", "ThreadSanitizer"),
    ("memory", "MemorySanitizer"),
    ("leak", "LeakSanitizer"),
];

/// `asan, UBSan; thread` → `["address", "undefined", "thread"]` (known ids, in display order).
pub fn parse_list(value: &str) -> Vec<String> {
    let wanted: Vec<String> = value
        .split([',', ';', ' ', '\n'])
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .map(|s| {
            match s.as_str() {
                "asan" => "address",
                "ubsan" => "undefined",
                "tsan" => "thread",
                "msan" => "memory",
                "lsan" => "leak",
                other => other,
            }
            .to_string()
        })
        .collect();
    SANITIZERS
        .iter()
        .filter(|(id, _)| wanted.iter().any(|w| w == id))
        .map(|(id, _)| id.to_string())
        .collect()
}

/// Why a combination does not build: ThreadSanitizer and MemorySanitizer each work only
/// without AddressSanitizer, LeakSanitizer and each other.
pub fn conflict(list: &[String]) -> Option<String> {
    let has = |id: &str| list.iter().any(|s| s == id);
    for exclusive in ["thread", "memory"] {
        if !has(exclusive) {
            continue;
        }
        let others: Vec<&str> = ["address", "leak", "thread", "memory"]
            .into_iter()
            .filter(|o| *o != exclusive && has(o))
            .collect();
        if let Some(o) = others.first() {
            let label = |id: &str| {
                SANITIZERS
                    .iter()
                    .find(|(i, _)| *i == id)
                    .map(|(_, l)| *l)
                    .unwrap_or("")
                    .to_string()
            };
            return Some(format!(
                "{} cannot be combined with {}",
                label(exclusive),
                label(o)
            ));
        }
    }
    None
}

/// `-fsanitize=address,undefined -fno-omit-frame-pointer` (compiler flags); `None` without
/// sanitizers.
pub fn compile_flags(list: &[String]) -> Option<String> {
    if list.is_empty() {
        return None;
    }
    let mut flags = format!("-fsanitize={} -fno-omit-frame-pointer", list.join(","));
    if list.iter().any(|s| s == "memory") {
        flags.push_str(" -fsanitize-memory-track-origins");
    }
    Some(flags)
}

/// `-fsanitize=address,undefined` (linker flags).
pub fn link_flags(list: &[String]) -> Option<String> {
    (!list.is_empty()).then(|| format!("-fsanitize={}", list.join(",")))
}

/// Runtime options for runs of a sanitized build, unless the environment sets them: UBSan
/// prints stacks, ASan reports leaks at exit.
pub fn run_env(list: &[String]) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    if list.iter().any(|s| s == "undefined") {
        env.insert("UBSAN_OPTIONS".into(), "print_stacktrace=1".into());
    }
    if list.iter().any(|s| s == "address") {
        env.insert("ASAN_OPTIONS".into(), "detect_leaks=1".into());
    }
    env
}

/// The `-D` arguments for configure: sanitizer flags appended to the compiler and linker flag
/// variables of `existing` (the profile's options / cache variables, `NAME` → value) or set.
pub fn cmake_definitions(list: &[String], existing: &BTreeMap<String, String>) -> Vec<String> {
    let (Some(compile), Some(link)) = (compile_flags(list), link_flags(list)) else {
        return Vec::new();
    };
    [
        ("CMAKE_C_FLAGS", &compile),
        ("CMAKE_CXX_FLAGS", &compile),
        ("CMAKE_EXE_LINKER_FLAGS", &link),
        ("CMAKE_SHARED_LINKER_FLAGS", &link),
    ]
    .into_iter()
    .map(
        |(name, flags)| match existing.get(name).filter(|v| !v.trim().is_empty()) {
            Some(v) => format!("-D{name}={} {flags}", v.trim()),
            None => format!("-D{name}={flags}"),
        },
    )
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_conflicts_and_flags() {
        assert_eq!(parse_list("ubsan, ASan;bogus"), ["address", "undefined"]);
        assert!(parse_list("").is_empty());
        let l = parse_list("address,undefined");
        assert_eq!(conflict(&l), None);
        assert_eq!(
            compile_flags(&l).unwrap(),
            "-fsanitize=address,undefined -fno-omit-frame-pointer"
        );
        assert_eq!(link_flags(&l).unwrap(), "-fsanitize=address,undefined");
        assert_eq!(
            conflict(&parse_list("thread,address")).unwrap(),
            "ThreadSanitizer cannot be combined with AddressSanitizer"
        );
        assert!(conflict(&parse_list("memory,thread")).is_some());
        assert_eq!(conflict(&parse_list("thread,undefined")), None);
        assert!(compile_flags(&parse_list("memory"))
            .unwrap()
            .ends_with("-fsanitize-memory-track-origins"));
        assert_eq!(run_env(&l)["UBSAN_OPTIONS"], "print_stacktrace=1");
        assert!(run_env(&parse_list("thread")).is_empty());
        let existing: BTreeMap<_, _> =
            [("CMAKE_CXX_FLAGS".to_string(), "-Wall".to_string())].into();
        let defs = cmake_definitions(&parse_list("address"), &existing);
        assert_eq!(
            defs[0],
            "-DCMAKE_C_FLAGS=-fsanitize=address -fno-omit-frame-pointer"
        );
        assert_eq!(
            defs[1],
            "-DCMAKE_CXX_FLAGS=-Wall -fsanitize=address -fno-omit-frame-pointer"
        );
        assert_eq!(defs[2], "-DCMAKE_EXE_LINKER_FLAGS=-fsanitize=address");
        assert!(cmake_definitions(&[], &existing).is_empty());
    }
}
