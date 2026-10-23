//! Lists the shipped library's files for embedding: every capture and IR in
//! `default-config/models/`, every frozen Core in `default-config/frozen/`,
//! every profile in `default-config/profiles/` (and the same for the bass's
//! `default-config-bass/`) — so what
//! `default-config/ship-library.sh` puts there ships, with no list to keep.
//! The `models/` directories are links into the rig-models repo (`just
//! assets`); a release build refuses to run without them.

use std::fmt::Write as _;
use std::path::Path;

fn files(dir: &Path, exts: &[&str]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| exts.contains(&e))
                })
                .map(|p| {
                    (
                        p.file_name().unwrap().to_string_lossy().into_owned(),
                        p.to_string_lossy().into_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// One shipped library's tables, named `<prefix>_MODELS`, `_FROZEN`,
/// `_PROFILES` (empty when the directory is not there).
fn tables(src: &mut String, root: &Path, prefix: &str) {
    println!("cargo:rerun-if-changed={}", root.display());
    // Only the ones that exist: cargo counts a missing rerun-if-changed path
    // as changed, so the bass's absent `frozen/` re-ran this script — and
    // recompiled guitar and everything above it — on every cargo call. The
    // root above still sees one appear.
    for sub in ["models", "frozen", "profiles"] {
        let dir = root.join(sub);
        if dir.exists() {
            println!("cargo:rerun-if-changed={}", dir.display());
        }
    }
    let bytes = |src: &mut String, name: &str, list: &[(String, String)]| {
        writeln!(src, "const {name}: &[(&str, &[u8])] = &[").unwrap();
        for (file, path) in list {
            writeln!(src, "    ({file:?}, include_bytes!({path:?})),").unwrap();
        }
        writeln!(src, "];").unwrap();
    };
    bytes(
        src,
        &format!("{prefix}_MODELS"),
        &files(&root.join("models"), &["nam", "wav"]),
    );
    bytes(
        src,
        &format!("{prefix}_FROZEN"),
        &files(&root.join("frozen"), &["nam"]),
    );
    writeln!(src, "const {prefix}_PROFILES: &[(&str, &str)] = &[").unwrap();
    for (file, path) in files(&root.join("profiles"), &["styx"]) {
        writeln!(src, "    ({file:?}, include_str!({path:?})),").unwrap();
    }
    writeln!(src, "];").unwrap();
}

fn main() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    // The captures live in the rig-models repo, linked in by `just assets`.
    // A debug build without them is fine (a rig with no captures, tests
    // that need them fail); a release would ship that rig to people.
    // Except where no rig ships: a browser build (the engine is native-only)
    // and a build that says so (`SIGNAL_RIG_NO_CAPTURES`: signal-web's
    // pre-render server, which only renders pages).
    println!("cargo:rerun-if-env-changed=SIGNAL_RIG_NO_CAPTURES");
    let ships_a_rig = std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("wasm32")
        && std::env::var_os("SIGNAL_RIG_NO_CAPTURES").is_none();
    if ships_a_rig && std::env::var("PROFILE").as_deref() == Ok("release") {
        for dir in ["default-config/models", "default-config-bass/models"] {
            assert!(
                !files(&here.join(dir), &["nam", "wav"]).is_empty(),
                "{dir} is empty: run `just assets` before a release build — the rig's captures are in the rig-models repo",
            );
        }
    }
    let mut src = String::new();
    // The guitar's (`DEFAULT_*`, as they always were) and the bass's.
    tables(&mut src, &here.join("default-config"), "DEFAULT");
    tables(&mut src, &here.join("default-config-bass"), "BASS");
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("default_files.rs");
    std::fs::write(out, src).unwrap();
}
