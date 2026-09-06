//! Survey a folder of `FabFilter` `.ffp` presets: what the parameters are
//! called, in what order, and what range of each the factory library
//! actually uses.
//!
//! This is the first thing to run against a plugin you intend to replace,
//! because it answers the layout question for free. A text `.ffp` lists the
//! same values the binary state carries, in the same order the plugin
//! publishes its parameters, under readable names — so a preset folder hands
//! over the field table that would otherwise have to be reverse-engineered
//! one float at a time.
//!
//! Two things it is specifically looking for:
//!
//! - **Layout variants.** A preset folder is not one format. Six of Pro-C 3's
//!   122 factory presets carry 69 keys in a different order under the same
//!   `FC3p` signature — an older layout the plugin still reads. Positional
//!   decoding of a *file* is therefore unsafe; positional decoding of the
//!   *binary state* is fine, because that was written by the installed build.
//!   The variant table below is what tells you which you are looking at.
//! - **What is worth translating.** A parameter that is constant across the
//!   whole factory library is one you can carry as a default and stop
//!   thinking about. A parameter with eighty distinct values is load-bearing.
//!
//! See `spec/measuring-a-plugin.md` for where this sits in the process.
//!
//! ```sh
//! cargo run --release -p signal-import --example ffp_survey -- \
//!     ~/Documents/FabFilter/Presets/"Pro-C 2"
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use signal_import::fabfilter::parser;

fn collect(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut items: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    items.sort();
    for path in items {
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ffp"))
        {
            out.push(path);
        }
    }
}

/// One preset, reduced to what the survey needs.
struct Preset {
    name: String,
    signature: String,
    keys: Vec<String>,
    values: Vec<f64>,
}

fn main() {
    let Some(root) = std::env::args().nth(1) else {
        eprintln!("usage: ffp_survey <preset directory>");
        std::process::exit(2);
    };

    let mut files = Vec::new();
    collect(Path::new(&root), &mut files);
    if files.is_empty() {
        eprintln!("no .ffp presets under {root}");
        std::process::exit(1);
    }

    let mut presets = Vec::new();
    let mut unreadable = 0usize;
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            unreadable += 1;
            continue;
        };
        match parser::parse_ffp_text(&text) {
            Err(_) => unreadable += 1,
            Ok(p) => presets.push(Preset {
                name: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                signature: p.signature.clone(),
                keys: p.parameters.iter().map(|(k, _)| k.clone()).collect(),
                values: p.parameters.iter().map(|(_, v)| *v).collect(),
            }),
        }
    }

    println!("{} presets under {root}", presets.len());
    if unreadable > 0 {
        println!("{unreadable} could not be read");
    }

    // ── Layout variants ─────────────────────────────────────────────────
    //
    // Grouped by (signature, parameter count). More than one row here means
    // the folder holds more than one layout, and the biggest is the current
    // one — the others are older presets the plugin still opens.
    let mut variants: BTreeMap<(String, usize), Vec<&Preset>> = BTreeMap::new();
    for p in &presets {
        variants
            .entry((p.signature.clone(), p.keys.len()))
            .or_default()
            .push(p);
    }
    println!("\n── layouts ──────────────────────────────────────────");
    for ((sig, n), group) in &variants {
        println!(
            "  {sig:<6} {n:>4} parameters  {:>4} presets   e.g. {}",
            group.len(),
            group[0].name
        );
    }
    if variants.len() > 1 {
        println!(
            "\n  More than one layout. Decode the binary state by index — the\n  \
             installed build wrote it — but decode preset FILES by key name."
        );
    }

    // The dominant layout is the one worth tabulating.
    let Some(((sig, count), group)) = variants.iter().max_by_key(|(_, g)| g.len()) else {
        return;
    };
    println!(
        "\n── {sig}, {count} parameters, across {} presets ──",
        group.len()
    );
    println!(
        "  {:>3} {:<38} {:>11} {:>11} {:>9}",
        "#", "parameter", "min", "max", "distinct"
    );

    let keys = &group[0].keys;
    for (i, key) in keys.iter().enumerate() {
        // A preset in this group that disagrees about key order would make
        // the column meaningless, so say so rather than average over it.
        if group.iter().any(|p| p.keys.get(i) != Some(key)) {
            println!("  {i:>3} {key:<38} {:>33}", "key order differs");
            continue;
        }
        let vals: Vec<f64> = group
            .iter()
            .filter_map(|p| p.values.get(i).copied())
            .collect();
        if vals.is_empty() {
            continue;
        }
        let min = vals.iter().copied().fold(f64::INFINITY, f64::min);
        let max = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mut sorted: Vec<u64> = vals.iter().map(|v| v.to_bits()).collect();
        sorted.sort_unstable();
        sorted.dedup();
        let note = if sorted.len() == 1 { "  constant" } else { "" };
        println!(
            "  {i:>3} {key:<38} {min:>11.4} {max:>11.4} {:>9}{note}",
            sorted.len()
        );
    }

    println!(
        "\n  A constant parameter can be carried as a default and forgotten.\n  \
         One with many distinct values is load-bearing, and its units are the\n  \
         next thing to measure — see `plugin_params`."
    );
}
