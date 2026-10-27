//! Run with Coverage for C and C++ tests, as CLion does it: the project is built with
//! coverage instrumentation (GCC `--coverage`, or Clang's source-based
//! `-fprofile-instr-generate -fcoverage-mapping`), the tests run, and the raw data becomes
//! an LCOV report for the shared Coverage tool window:
//!
//! - GCC: `.gcda` files next to the objects → `gcov --json-format --stdout` → LCOV
//!   ([`gcov_json_to_lcov`]),
//! - Clang: `LLVM_PROFILE_FILE` `.profraw` files → `llvm-profdata merge` → `llvm-cov export
//!   -format=lcov`.

use std::collections::BTreeMap;

use serde_json::Value;

use cforge_cmake::Command;

/// The raw-profile environment variable of Clang's instrumentation (`%p`: one file per
/// process, so CTest's parallel tests do not overwrite each other).
pub fn llvm_profile_env(dir: &str) -> (String, String) {
    (
        "LLVM_PROFILE_FILE".into(),
        format!("{}/forge-%p.profraw", dir.trim_end_matches(['/', '\\'])),
    )
}

/// `gcov --json-format --stdout <gcda files>` in the build folder.
pub fn gcov_command(gcov: &str, gcda: &[String], build_dir: &str) -> Command {
    let mut args = vec!["--json-format".to_string(), "--stdout".to_string()];
    args.extend(gcda.iter().cloned());
    Command {
        program: gcov.to_string(),
        args,
        cwd: build_dir.to_string(),
        env: BTreeMap::new(),
    }
}

/// `llvm-profdata merge -sparse <profraw…> -o <profdata>`.
pub fn llvm_merge_command(profdata_tool: &str, profraw: &[String], out: &str) -> Command {
    let mut args = vec!["merge".to_string(), "-sparse".to_string()];
    args.extend(profraw.iter().cloned());
    args.push("-o".into());
    args.push(out.to_string());
    Command {
        program: profdata_tool.to_string(),
        args,
        cwd: String::new(),
        env: BTreeMap::new(),
    }
}

/// `llvm-cov export -format=lcov -instr-profile=<profdata> <binary> -object <binary>…`;
/// the report is its standard output.
pub fn llvm_export_command(llvm_cov: &str, profdata: &str, binaries: &[String]) -> Option<Command> {
    let (first, rest) = binaries.split_first()?;
    let mut args = vec![
        "export".to_string(),
        "-format=lcov".to_string(),
        format!("-instr-profile={profdata}"),
        first.clone(),
    ];
    for b in rest {
        args.push("-object".into());
        args.push(b.clone());
    }
    Some(Command {
        program: llvm_cov.to_string(),
        args,
        cwd: String::new(),
        env: BTreeMap::new(),
    })
}

/// The `gcov` that belongs to a GCC (`/usr/bin/g++-14` → `/usr/bin/gcov-14`,
/// `arm-none-eabi-g++` → `arm-none-eabi-gcov`), and the LLVM tools next to a Clang.
pub fn tool_next_to(compiler: &str, tool: &str) -> String {
    let (dir, file) = match compiler.rfind(['/', '\\']) {
        Some(i) => (&compiler[..=i], &compiler[i + 1..]),
        None => ("", compiler),
    };
    let (stem, exe) = match file.strip_suffix(".exe") {
        Some(s) => (s, ".exe"),
        None => (file, ""),
    };
    for base in ["g++", "gcc", "clang++", "clang", "c++", "cc"] {
        if let Some(i) = stem.rfind(base) {
            let (prefix, suffix) = (&stem[..i], &stem[i + base.len()..]);
            let suffix_ok = suffix.is_empty() || suffix.starts_with('-');
            if suffix_ok && (prefix.is_empty() || prefix.ends_with('-')) {
                return format!("{dir}{prefix}{tool}{suffix}{exe}");
            }
        }
    }
    format!("{dir}{tool}{exe}")
}

#[derive(Default)]
struct FileData {
    lines: BTreeMap<u32, u64>,
    /// line → branch counts in order.
    branches: BTreeMap<u32, Vec<u64>>,
    functions: BTreeMap<String, (u32, u64)>,
}

fn under(root: &str, file: &str) -> bool {
    let root = root.replace('\\', "/");
    let root = root.trim_end_matches('/');
    let f = file.replace('\\', "/");
    root.is_empty() || f.starts_with(&format!("{root}/"))
}

/// Turn gcov's JSON (one document per line, one per `.gcda` file) into an LCOV tracefile.
/// Files outside `root` (system headers) are left out; a line reported by several objects
/// (headers, inline functions) adds up.
pub fn gcov_json_to_lcov(text: &str, root: &str) -> Result<String, String> {
    let mut files: BTreeMap<String, FileData> = BTreeMap::new();
    let mut any = false;
    for doc in text.lines().filter(|l| l.trim_start().starts_with('{')) {
        let v: Value = serde_json::from_str(doc).map_err(|e| format!("bad gcov output: {e}"))?;
        any = true;
        let cwd = v
            .get("current_working_directory")
            .and_then(Value::as_str)
            .unwrap_or("");
        for f in v.get("files").and_then(Value::as_array).into_iter().flatten() {
            let Some(name) = f.get("file").and_then(Value::as_str) else {
                continue;
            };
            let abs = if name.starts_with('/') || name.get(1..2) == Some(":") {
                name.to_string()
            } else {
                format!("{}/{name}", cwd.trim_end_matches('/'))
            };
            if !under(root, &abs) {
                continue;
            }
            let data = files.entry(abs).or_default();
            for l in f.get("lines").and_then(Value::as_array).into_iter().flatten() {
                let Some(n) = l.get("line_number").and_then(Value::as_u64) else {
                    continue;
                };
                let count = l.get("count").and_then(Value::as_u64).unwrap_or(0);
                *data.lines.entry(n as u32).or_default() += count;
                let br: Vec<u64> = l
                    .get("branches")
                    .and_then(Value::as_array)
                    .map(|b| {
                        b.iter()
                            .filter(|b| !b.get("throw").and_then(Value::as_bool).unwrap_or(false))
                            .map(|b| b.get("count").and_then(Value::as_u64).unwrap_or(0))
                            .collect()
                    })
                    .unwrap_or_default();
                if !br.is_empty() {
                    let slot = data.branches.entry(n as u32).or_default();
                    if slot.len() < br.len() {
                        slot.resize(br.len(), 0);
                    }
                    for (i, c) in br.into_iter().enumerate() {
                        slot[i] += c;
                    }
                }
            }
            for func in f.get("functions").and_then(Value::as_array).into_iter().flatten() {
                let name = func
                    .get("demangled_name")
                    .or_else(|| func.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let line = func.get("start_line").and_then(Value::as_u64).unwrap_or(0) as u32;
                let count = func.get("execution_count").and_then(Value::as_u64).unwrap_or(0);
                let e = data.functions.entry(name).or_insert((line, 0));
                e.1 += count;
            }
        }
    }
    if !any {
        return Err("gcov reported no coverage data (was the project built with --coverage?)".into());
    }
    let mut out = String::new();
    for (file, d) in files {
        out.push_str("TN:\n");
        out.push_str(&format!("SF:{file}\n"));
        for (name, (line, count)) in &d.functions {
            out.push_str(&format!("FN:{line},{name}\n"));
            out.push_str(&format!("FNDA:{count},{name}\n"));
        }
        out.push_str(&format!("FNF:{}\n", d.functions.len()));
        out.push_str(&format!(
            "FNH:{}\n",
            d.functions.values().filter(|(_, c)| *c > 0).count()
        ));
        let mut brf = 0;
        let mut brh = 0;
        for (line, counts) in &d.branches {
            let executed = d.lines.get(line).copied().unwrap_or(0) > 0;
            for (i, c) in counts.iter().enumerate() {
                brf += 1;
                if *c > 0 {
                    brh += 1;
                }
                let taken = if executed { c.to_string() } else { "-".into() };
                out.push_str(&format!("BRDA:{line},0,{i},{taken}\n"));
            }
        }
        out.push_str(&format!("BRF:{brf}\nBRH:{brh}\n"));
        for (line, count) in &d.lines {
            out.push_str(&format!("DA:{line},{count}\n"));
        }
        out.push_str(&format!(
            "LF:{}\nLH:{}\n",
            d.lines.len(),
            d.lines.values().filter(|c| **c > 0).count()
        ));
        out.push_str("end_of_record\n");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_gcov_json_becomes_lcov() {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gcov.json"),
        )
        .unwrap();
        let lcov = gcov_json_to_lcov(&text, "/home/dev/shapes").unwrap();
        let files: Vec<&str> = lcov.lines().filter_map(|l| l.strip_prefix("SF:")).collect();
        assert_eq!(files, ["/home/dev/shapes/src/shapes.cpp", "/home/dev/shapes/tests/shapes_test.cpp"]);
        // `area` ran (the test ran with the `area` argument), `perimeter` did not.
        assert!(lcov.contains("FNDA:1,shapes::area(std::vector<shapes::Point, std::allocator<shapes::Point> > const&)\n"));
        assert!(lcov.contains("FNDA:0,shapes::perimeter(std::vector<shapes::Point, std::allocator<shapes::Point> > const&)\n"));
        let report = forge_testing::CoverageReport::parse(&lcov, forge_testing::CoverageFormat::Lcov).unwrap();
        assert_eq!(report.files.len(), 2);
        assert!(gcov_json_to_lcov("", "/x").is_err());
    }

    #[test]
    fn tools_and_commands() {
        assert_eq!(tool_next_to("/usr/bin/g++-14", "gcov"), "/usr/bin/gcov-14");
        assert_eq!(tool_next_to("/usr/bin/c++", "gcov"), "/usr/bin/gcov");
        assert_eq!(tool_next_to("/opt/arm/bin/arm-none-eabi-g++", "gcov"), "/opt/arm/bin/arm-none-eabi-gcov");
        assert_eq!(tool_next_to("/usr/bin/clang++-18", "llvm-cov"), "/usr/bin/llvm-cov-18");
        assert_eq!(tool_next_to("C:\\msys64\\ucrt64\\bin\\g++.exe", "gcov"), "C:\\msys64\\ucrt64\\bin\\gcov.exe");
        assert_eq!(tool_next_to("cl.exe", "gcov"), "gcov.exe");
        let c = gcov_command("gcov", &["a.gcda".into()], "/b");
        assert_eq!(c.args, ["--json-format", "--stdout", "a.gcda"]);
        assert_eq!(llvm_profile_env("/tmp/cov/").1, "/tmp/cov/forge-%p.profraw");
        let m = llvm_merge_command("llvm-profdata", &["a.profraw".into()], "m.profdata");
        assert_eq!(m.args, ["merge", "-sparse", "a.profraw", "-o", "m.profdata"]);
        let e = llvm_export_command("llvm-cov", "m.profdata", &["t1".into(), "t2".into()]).unwrap();
        assert_eq!(e.args, ["export", "-format=lcov", "-instr-profile=m.profdata", "t1", "-object", "t2"]);
        assert!(llvm_export_command("llvm-cov", "m", &[]).is_none());
    }
}
