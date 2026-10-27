//! Smoke test against the real tools: detects the toolchains of this machine, configures a
//! small CMake project (in a temporary folder) with the file API query, reads the reply,
//! builds it, runs CTest with a JUnit report, parses a failing build and loads libstdc++'s
//! pretty printers into GDB the way debug sessions do. Needs CMake 3.21+, a C++ compiler,
//! Ninja or Make, and GDB; run with `cargo test -p cforge-lang-cpp -- --ignored`.

use std::path::Path;
use std::process::Command;

use cforge_cmake::{build_command, configure_command, BuildAction, ConfigureContext, Profile};

fn run(cmd: &cforge_cmake::Command) -> (bool, String) {
    let out = Command::new(&cmd.program)
        .args(&cmd.args)
        .current_dir(&cmd.cwd)
        .envs(&cmd.env)
        .output()
        .expect("tool runs");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

#[test]
#[ignore = "needs cmake, a C++ compiler, ninja or make and gdb"]
fn configure_build_test_and_debug_setup() {
    let toolchains = cforge_toolchains::detect(&cforge_toolchains::SystemProbe);
    let tc = toolchains.first().expect("a toolchain");
    assert!(tc.cmake.is_some() && tc.cxx_compiler.is_some(), "{tc:?}");

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "CMakeLists.txt",
        "cmake_minimum_required(VERSION 3.21)\nproject(smoke CXX)\nadd_executable(app main.cpp)\nenable_testing()\nadd_test(NAME runs COMMAND app)\n",
    );
    write(root, "main.cpp", "#include <vector>\nint main() { std::vector<int> v{1, 2, 3}; return v.size() == 3 ? 0 : 1; }\n");

    let profile = Profile::debug();
    let source = root.to_string_lossy().into_owned();
    let build_dir = root.join(profile.build_dir());
    let query = build_dir
        .join(cforge_cmake::fileapi::QUERY_DIR)
        .join("query.json");
    std::fs::create_dir_all(query.parent().unwrap()).unwrap();
    std::fs::write(&query, cforge_cmake::fileapi::query_json()).unwrap();
    let generator = tc.default_generator().to_string();
    let ctx = ConfigureContext {
        cmake: tc.cmake.as_ref().unwrap().path.clone(),
        source_dir: source.clone(),
        default_generator: generator.clone(),
        toolchain_variables: cforge_toolchains::cmake_variables(tc, &generator),
    };
    let (ok, log) = run(&configure_command(&profile, &ctx));
    assert!(ok, "{log}");
    assert!(cforge_cmake::parse_log(&log).is_empty(), "{log}");

    let reply = build_dir.join(cforge_cmake::fileapi::REPLY_DIR);
    let files: Vec<String> = std::fs::read_dir(&reply)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let model = cforge_cmake::fileapi::parse_reply(&files, &|n| {
        std::fs::read_to_string(reply.join(n)).ok()
    })
    .unwrap();
    let app = model.target("Debug", "app").expect("the app target");
    assert_eq!(app.defined_at.as_ref().unwrap().line, 3);
    let exe = app.executable().unwrap().to_string();

    let (ok, out) = run(&build_command(
        &profile,
        &ctx.cmake,
        &source,
        &["app".into()],
        BuildAction::Build,
        model.multi_config,
    ));
    assert!(ok, "{out}");
    assert!(Path::new(&exe).is_file(), "{exe}");
    assert!(build_dir.join("compile_commands.json").is_file());

    // CTest through the provider's command line, with its JUnit report.
    let report = root.join("report");
    std::fs::create_dir_all(&report).unwrap();
    let cpp = cforge_lang_cpp::CppContext {
        build_dir: build_dir.to_string_lossy().into_owned(),
        build_type: "Debug".into(),
        multi_config: model.multi_config,
        ctest: cforge_cmake::profile::ctest_next_to(&ctx.cmake),
        targets: Vec::new(),
        coverage_tools: Default::default(),
    };
    let mut cctx = forge_testing::CommandContext {
        report_dir: report.to_string_lossy().into_owned(),
        ..Default::default()
    };
    cctx.settings.insert(
        cforge_lang_cpp::CONTEXT_KEY.into(),
        serde_json::to_string(&cpp).unwrap(),
    );
    let cfg = forge_runtools::Configuration::new("All", "ctest");
    let cmd = forge_testing::TestFrameworkProvider::command(
        &cforge_lang_cpp::CppTests::ctest(),
        &cfg,
        &cctx,
    )
    .unwrap()
    .unwrap();
    let status = Command::new(&cmd.program)
        .args(&cmd.args)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success());
    let xml = std::fs::read_to_string(report.join("ctest-report.xml")).unwrap();
    let events =
        forge_output::formats::document_events(&xml, forge_output::JUnitFlavor::Generic).unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, forge_output::TestEvent::TestStarted { name, .. } if name == "runs")));

    // A failing build becomes problems with project paths.
    write(root, "main.cpp", "int main() { return missing; }\n");
    let (ok, out) = run(&build_command(
        &profile,
        &ctx.cmake,
        &source,
        &[],
        BuildAction::Build,
        model.multi_config,
    ));
    assert!(!ok);
    let problems = cforge_lang_cpp::parse_build_output(&out);
    let p = problems
        .iter()
        .find(|p| p.message.contains("missing"))
        .expect("the error");
    let rel =
        cforge_lang_cpp::diagnostics::project_path(&p.file, &source, "cmake-build-debug", &|f| {
            root.join(f).is_file()
        });
    assert_eq!(rel.as_deref(), Some("main.cpp"));
    assert_eq!(p.line, 1);

    // GDB loads libstdc++'s printers with the debug session's -iex commands.
    if let Some(gdb) = toolchains
        .iter()
        .find_map(|t| t.debugger.clone())
        .filter(|d| d.kind == cforge_toolchains::DebuggerKind::Gdb)
    {
        let cxx = tc.cxx_compiler.clone().unwrap();
        let version = tc
            .compiler
            .as_ref()
            .map(|c| c.version.clone())
            .unwrap_or_default();
        let printers = cforge_lang_cpp::tools::printer_dir_candidates(&cxx, &version)
            .into_iter()
            .find(|d| Path::new(d).join("libstdcxx").is_dir());
        let setup = cforge_lang_cpp::tools::debug_setup("gdb", printers.as_deref(), true);
        let out = Command::new(&gdb.path)
            .args(["-nx", "-batch"])
            .args(&setup.adapter_args)
            .args(["-ex", "info pretty-printer"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        if printers.is_some() {
            assert!(text.contains("libstdc++"), "{text}");
        }
    }
}
