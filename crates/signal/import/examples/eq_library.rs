//! Build the FTS-EQ preset library from a bank of `FabFilter` Pro-Q 4 presets.
//!
//! Walks a directory of `.ffp` files, translates each to `signal_fx::NativeEq`
//! parameters, and writes one JSON per preset in the same shape the reverb
//! library uses — so `preset-browser` loads both with the same reader and the
//! EQ editor's browser needs nothing EQ-specific to show them.
//!
//! Text and binary presets both work. A text `.ffp` lists every parameter by
//! name in the same order as the binary float vector, which is what makes the
//! two paths one decoder: the text file is read back into that vector and
//! handed to the same `proq4::decode`.
//!
//! ```sh
//! cargo run -p signal-import --example eq_library -- \
//!     --presets "/path/to/Presets/Pro-Q 4" \
//!     --out /path/to/Libraries/Presets/FTS-EQ/fabfilter-proq4
//! ```

use std::path::{Path, PathBuf};

use signal_import::fabfilter::{ffbs, parser, proq4};

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

/// Every `.ffp` under `dir`, recursively.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("ffp"))
        {
            out.push(path);
        }
    }
}

/// Read a preset file into the flat parameter vector Pro-Q's decoder wants.
///
/// The tags come back too: a text preset carries the ones `FabFilter` filed it
/// under, which are better category material than the folder it sits in.
fn read_state(path: &Path) -> Result<(ffbs::FfbsState, Vec<String>, Option<String>), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;

    if parser::is_text_format(&bytes) {
        let text = String::from_utf8_lossy(&bytes);
        let preset = parser::parse_ffp_text(&text).map_err(|e| e.to_string())?;
        // The `[Parameters]` list is in binary order, so its values *are* the
        // float vector — the names are there for humans and for the field map.
        let params: Vec<f32> = preset.parameters.iter().map(|(_, v)| *v as f32).collect();
        Ok((
            ffbs::FfbsState {
                version: 1,
                params,
                metadata: Default::default(),
            },
            preset.tags,
            preset.author,
        ))
    } else if ffbs::is_ffbs(&bytes) {
        let state = ffbs::parse(&bytes).map_err(|e| format!("{e:?}"))?;
        Ok((state, Vec::new(), None))
    } else {
        Err("neither a text nor an FFBS preset".to_string())
    }
}

fn main() {
    let (Some(presets), Some(out_dir)) = (arg("--presets"), arg("--out")) else {
        eprintln!("usage: eq_library --presets <dir> --out <dir>");
        std::process::exit(2);
    };
    let root = PathBuf::from(&presets);
    let out_dir = PathBuf::from(&out_dir);
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("could not create {}: {e}", out_dir.display());
        std::process::exit(1);
    }

    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();
    println!("{} preset files under {}", files.len(), root.display());

    let (mut written, mut skipped, mut dynamic, mut spectral) = (0, 0, 0, 0);
    for path in &files {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let (state, tags, author) = match read_state(path) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("skip {stem}: {e}");
                skipped += 1;
                continue;
            }
        };
        let eq = match proq4::decode(&state) {
            Ok(eq) => eq,
            Err(e) => {
                eprintln!("skip {stem}: {e:?}");
                skipped += 1;
                continue;
            }
        };

        let params = proq4::to_native_eq_params(&eq);
        let n_dyn = eq.bands.iter().filter(|b| b.is_active() && b.is_dynamic()).count();
        let n_spec = eq
            .bands
            .iter()
            .filter(|b| b.is_active() && b.is_dynamic() && b.spectral)
            .count();
        if n_dyn > 0 {
            dynamic += 1;
        }
        if n_spec > 0 {
            spectral += 1;
        }

        // The bank's own folder is the category — FabFilter files these by
        // instrument ("Guitar", "Vocals", "Drums"), which is how someone
        // reaches for one.
        let category = path
            .parent()
            .filter(|p| *p != root)
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());

        let doc = serde_json::json!({
            "source": {
                "preset": stem,
                "file": path.to_string_lossy(),
                "plugin": "Pro-Q 4",
                "mode": category,
                "author": author,
                "tags": tags,
            },
            "target": {
                "engine": "signal_fx::NativeEq",
                "parameters": params
                    .iter()
                    .map(|(n, v)| serde_json::json!({ "name": n, "value": v }))
                    .collect::<Vec<_>>(),
            },
            // An EQ translation is a parameter mapping, not a fit: every field
            // has a counterpart and nothing is being approximated, so there is
            // no error to report. Left null rather than faked, which is what
            // the browser reads to decide whether to show a match badge.
            "measurement": serde_json::Value::Null,
            "summary": {
                "active_bands": eq.bands.iter().filter(|b| b.is_active()).count(),
                "dynamic_bands": n_dyn,
                "spectral_bands": n_spec,
            },
        });

        let out = out_dir.join(format!("{stem}.json"));
        match serde_json::to_string_pretty(&doc)
            .map_err(|e| e.to_string())
            .and_then(|t| std::fs::write(&out, t).map_err(|e| e.to_string()))
        {
            Ok(()) => written += 1,
            Err(e) => {
                eprintln!("could not write {}: {e}", out.display());
                skipped += 1;
            }
        }
    }

    println!(
        "wrote {written} presets to {} ({skipped} skipped)\n  \
         {dynamic} use dynamic bands, {spectral} use spectral bands",
        out_dir.display()
    );
}
