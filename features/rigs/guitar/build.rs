//! Lists the shipped library's files for embedding: every capture and IR in
//! `models/`, every frozen Core in `frozen/`, every profile in
//! `default-config/profiles/` (and the same for the bass's
//! `default-config-bass/`) — so what `default-config/ship-library.sh` puts
//! there ships, with no list to keep.
//!
//! The captures and frozen Cores are not in this repository (they are
//! private). They come from `$SIGNAL_RIG_LIBRARY/guitar/{models,frozen}`
//! (and `…/bass/…`) when the app building this crate says where its copy
//! is — Studio does, from its `.cargo/config.toml`, since a crate taken by
//! git has no links of its own — else from `default-config/models` and
//! `default-config/frozen` here. A release build refuses to run without
//! the captures.
//!
//! The table names each file by its path relative to `OUT_DIR` (an
//! `include_bytes!` in an `include!`d file resolves against that file), so
//! it holds no absolute path: with the target dir in the checkout it is the
//! same text in every worktree, and the compiler cache (kache, which hashes
//! it) serves one worktree's `guitar` — and everything above it — to the
//! next.

use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

/// `to` relative to the directory `from` (both canonical), or `to` itself
/// when they share no root (another drive).
fn relative(from: &Path, to: &Path) -> PathBuf {
    let from: Vec<Component> = from.components().collect();
    let to_parts: Vec<Component> = to.components().collect();
    let common = from
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    if common == 0 {
        return to.to_path_buf();
    }
    let ups = std::iter::repeat_n(Component::ParentDir, from.len().saturating_sub(common));
    ups.chain(to_parts.into_iter().skip(common)).collect()
}

/// `(file name, path relative to out_dir)` of every file in `dir` with one
/// of `exts`, by name.
fn files(dir: &Path, out_dir: &Path, exts: &[&str]) -> Vec<(String, String)> {
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
                    // Embedded by its real path: `models/` is a link into
                    // .assets (`just assets`), and a path through it is one
                    // git cannot answer for, so cargo-rail could record no
                    // evidence for any build of this crate.
                    let real = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
                    (
                        p.file_name().unwrap().to_string_lossy().into_owned(),
                        relative(out_dir, &real).to_string_lossy().into_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// One shipped library's tables, named `<prefix>_MODELS`, `_FROZEN`,
/// `_PROFILES` (empty when the directory is not there): the profiles from
/// `root`, the captures and frozen Cores from `captures`.
fn tables(src: &mut String, root: &Path, captures: &Path, out_dir: &Path, prefix: &str) {
    // Only the ones that exist: cargo counts a missing rerun-if-changed path
    // as changed, so the bass's absent `frozen/` re-ran this script — and
    // recompiled guitar and everything above it — on every cargo call. The
    // roots still see one appear.
    for dir in [root, captures] {
        if dir.exists() {
            println!("cargo:rerun-if-changed={}", dir.display());
        }
    }
    for dir in [
        captures.join("models"),
        captures.join("frozen"),
        root.join("profiles"),
    ] {
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
        &files(&captures.join("models"), out_dir, &["nam", "wav"]),
    );
    bytes(
        src,
        &format!("{prefix}_FROZEN"),
        &files(&captures.join("frozen"), out_dir, &["nam"]),
    );
    writeln!(src, "const {prefix}_PROFILES: &[(&str, &str)] = &[").unwrap();
    for (file, path) in files(&root.join("profiles"), out_dir, &["styx"]) {
        writeln!(src, "    ({file:?}, include_str!({path:?})),").unwrap();
    }
    writeln!(src, "];").unwrap();
}

fn main() {
    // Read at run time, not `env!`: that would put this checkout's path in
    // the build script's own cache key.
    let here = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let out_real = std::fs::canonicalize(&out_dir).unwrap_or_else(|_| out_dir.clone());
    // Where the captures and frozen Cores are: the building app's copy
    // (`SIGNAL_RIG_LIBRARY`), else this crate's own directories.
    println!("cargo:rerun-if-env-changed=SIGNAL_RIG_LIBRARY");
    let library = std::env::var_os("SIGNAL_RIG_LIBRARY").map(PathBuf::from);
    let captures = |own: &str, name: &str| {
        library
            .as_ref()
            .map_or_else(|| here.join(own), |dir| dir.join(name))
    };
    let (guitar, bass) = (
        captures("default-config", "guitar"),
        captures("default-config-bass", "bass"),
    );
    // The captures live in the rig-models repo, which is private.
    // A debug build without them is fine (a rig with no captures, tests
    // that need them fail); a release would ship that rig to people.
    // Except where no rig ships: a browser build (the engine is native-only)
    // and a build that says so (`SIGNAL_RIG_NO_CAPTURES`: signal-web's
    // pre-render server, which only renders pages).
    println!("cargo:rerun-if-env-changed=SIGNAL_RIG_NO_CAPTURES");
    let ships_a_rig = std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("wasm32")
        && std::env::var_os("SIGNAL_RIG_NO_CAPTURES").is_none();
    if ships_a_rig && std::env::var("PROFILE").as_deref() == Ok("release") {
        for dir in [guitar.join("models"), bass.join("models")] {
            assert!(
                !files(&dir, &out_real, &["nam", "wav"]).is_empty(),
                "{} is empty: the rig's captures are in the rig-models repo (in Studio, `just assets` links them in) — a release build ships them",
                dir.display(),
            );
        }
    }
    let mut src = String::new();
    // The guitar's (`DEFAULT_*`, as they always were) and the bass's.
    tables(
        &mut src,
        &here.join("default-config"),
        &guitar,
        &out_real,
        "DEFAULT",
    );
    tables(
        &mut src,
        &here.join("default-config-bass"),
        &bass,
        &out_real,
        "BASS",
    );
    std::fs::write(out_dir.join("default_files.rs"), src).unwrap();
}
