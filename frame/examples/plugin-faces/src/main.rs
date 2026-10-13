//! `cargo run -p plugin-faces -- OUT_DIR [--only PREFIX…]` — write every
//! face (or only those whose file starts with a PREFIX) + catalogs.
//!
//! Each face is `OUT_DIR/<plugin>/<file>.frame` plus, when it carries
//! textures or fonts, its `<file>.frame.assets/` sidecar (see `document-io`).
//! Node ids are stable (see `face::stable_ids`), so regenerating without a
//! source change rewrites every file byte for byte.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (only, rest): (Vec<String>, Vec<String>) = match args.iter().position(|a| a == "--only") {
        Some(i) => (args[i + 1..].to_vec(), args[..i].to_vec()),
        None => (Vec::new(), args),
    };
    let out = std::path::PathBuf::from(rest.first().cloned().unwrap_or_else(|| "examples/plugins".into()));
    let n = plugin_faces::write_only(&out, &only);
    println!("wrote {n} faces to {}", out.display());
}
