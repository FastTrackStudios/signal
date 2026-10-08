//! Lists the shipped library's files for embedding: every capture and IR in
//! `default-config/models/`, every frozen Core in `default-config/frozen/`,
//! every profile in `default-config/profiles/` — so what
//! `default-config/ship-library.sh` puts there ships, with no list to keep.

use std::fmt::Write as _;
use std::path::Path;

fn files(dir: &Path, exts: &[&str]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.contains(&e)))
                .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), p.to_string_lossy().into_owned()))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("default-config");
    println!("cargo:rerun-if-changed={}", root.display());
    for sub in ["models", "frozen", "profiles"] {
        println!("cargo:rerun-if-changed={}", root.join(sub).display());
    }
    let mut src = String::new();
    let bytes = |src: &mut String, name: &str, list: &[(String, String)]| {
        writeln!(src, "const {name}: &[(&str, &[u8])] = &[").unwrap();
        for (file, path) in list {
            writeln!(src, "    ({file:?}, include_bytes!({path:?})),").unwrap();
        }
        writeln!(src, "];").unwrap();
    };
    bytes(&mut src, "DEFAULT_MODELS", &files(&root.join("models"), &["nam", "wav"]));
    bytes(&mut src, "DEFAULT_FROZEN", &files(&root.join("frozen"), &["nam"]));
    writeln!(src, "const DEFAULT_PROFILES: &[(&str, &str)] = &[").unwrap();
    for (file, path) in files(&root.join("profiles"), &["styx"]) {
        writeln!(src, "    ({file:?}, include_str!({path:?})),").unwrap();
    }
    writeln!(src, "];").unwrap();
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("default_files.rs");
    std::fs::write(out, src).unwrap();
}
