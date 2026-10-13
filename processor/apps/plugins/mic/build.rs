//! Builds the default mic (LD-47K) into the plugin, so FTS Mic works with
//! no models installed. The model files are private (measured from a
//! commercial plugin) and live outside this repository: the build takes
//! them from the first of `$FTS_MIC_MODELS`, `~/.local/share/fts/mic-models`
//! and a `mic-models` checkout beside this one that has them. Without any,
//! the plugin builds with nothing embedded and needs the models installed.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The default mic's file name (Mic 1 Type's default).
const DEFAULT: &str = "LD-47K.micm";
/// The sets to embed: each sample rate, the LX capsule pair, the 180's backward mic.
const SETS: [&str; 6] = ["44100", "48000", "88200", "96000", "48000-LX", "48000-180"];

fn candidates() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os("FTS_MIC_MODELS") {
        dirs.push(PathBuf::from(dir));
    }
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/fts/mic-models"));
    }
    if let Some(manifest) = std::env::var_os("CARGO_MANIFEST_DIR") {
        // processor/apps/plugins/mic -> ../../../../mic-models
        dirs.push(PathBuf::from(manifest).join("../../../../mic-models"));
    }
    dirs
}

fn main() {
    println!("cargo:rerun-if-env-changed=FTS_MIC_MODELS");
    let root = candidates().into_iter().find(|d| d.join("48000").join(DEFAULT).is_file());
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default());
    let mut table = String::from("/// The default mic's model per set (folder name), built in.\npub static EMBEDDED: &[(&str, &[u8])] = &[\n");
    match &root {
        Some(root) => {
            for set in SETS {
                let src = root.join(set).join(DEFAULT);
                if !src.is_file() {
                    continue;
                }
                println!("cargo:rerun-if-changed={}", src.display());
                let dst = out.join(format!("{set}-{DEFAULT}"));
                if std::fs::copy(&src, &dst).is_err() {
                    continue;
                }
                let _ = writeln!(table, "    ({set:?}, include_bytes!({:?})),", dst.display().to_string());
            }
        }
        None => println!("cargo:warning=FTS Mic: no mic models found at build time; the default mic is not built in"),
    }
    table.push_str("];\n");
    let _ = std::fs::write(Path::new(&out).join("embedded.rs"), table);
}
