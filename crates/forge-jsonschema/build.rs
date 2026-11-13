//! Compresses the bundled schemas (`schemas/*.json`) into `OUT_DIR`; `bundled.rs` includes
//! them and inflates each one on first use. The files in the repository stay as published,
//! so Go to Declaration shows them unchanged.

use std::io::Write;
use std::path::Path;

fn main() {
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    let dir = Path::new("schemas");
    println!("cargo:rerun-if-changed=schemas");
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .expect("schemas dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".json"))
        .collect();
    names.sort();
    for name in names {
        let text = std::fs::read(dir.join(&name)).expect("read schema");
        let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
        enc.write_all(&text).expect("compress");
        let data = enc.finish().expect("compress");
        std::fs::write(Path::new(&out).join(format!("{name}.deflate")), data).expect("write");
    }
}
