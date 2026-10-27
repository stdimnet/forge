//! Smoke test against a real Valgrind (ignored: needs a C compiler and valgrind). Run with
//! `cargo test -p cforge-analyzers --test smoke -- --ignored`. Everything is built in a
//! temporary folder.

use std::process::Command;

#[test]
#[ignore]
fn memcheck_xml_of_a_real_program() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("leak.c");
    std::fs::write(
        &src,
        "#include <stdlib.h>\nint main(void) {\n  int *p = malloc(4 * sizeof(int));\n  p[0] = 1;\n  int x = p[4];\n  return x == 7;\n}\n",
    )
    .unwrap();
    let exe = dir.path().join("leak");
    let ok = Command::new("cc")
        .args(["-g", "-O0"])
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    let xml = dir.path().join("out.xml");
    let args =
        cforge_analyzers::valgrind_args(&xml.to_string_lossy(), "", &exe.to_string_lossy(), &[]);
    Command::new("valgrind").args(&args).status().unwrap();
    let mut report =
        cforge_analyzers::valgrind::parse(&std::fs::read_to_string(&xml).unwrap()).unwrap();
    let root = dir.path().to_string_lossy().to_string();
    cforge_analyzers::valgrind::resolve(&mut report, &|f| {
        cforge_analyzers::project_path(f, &root, "", &|_| false)
    });
    assert_eq!(report.state, "FINISHED");
    let read = report
        .errors
        .iter()
        .find(|e| e.kind == "InvalidRead")
        .unwrap();
    assert_eq!(
        read.location.as_ref().unwrap().path.as_deref(),
        Some("leak.c")
    );
    assert_eq!(read.location.as_ref().unwrap().line, 5);
    assert_eq!(report.leak_summary.definitely_lost.bytes, 16);
}
