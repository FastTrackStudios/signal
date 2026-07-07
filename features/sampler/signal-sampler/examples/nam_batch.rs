//! Batch-load every `.nam` in a directory through the engine and report a
//! pass/fail summary (load + non-silence), tagged by architecture.
//!
//! ```text
//! cargo run --release -p signal-sampler --example nam_batch -- "/path/to/dir"
//! ```

use std::path::Path;

use signal_sampler::nam::NamProcessor;

fn arch_of(path: &Path) -> String {
    // Cheap JSON peek for the "architecture" field without a full parse.
    let Ok(txt) = std::fs::read_to_string(path) else {
        return "?".into();
    };
    for key in ["\"architecture\""] {
        if let Some(i) = txt.find(key) {
            let after = &txt[i + key.len()..];
            if let Some(c) = after.find(':') {
                let rest = after[c + 1..].trim_start();
                if let Some(stripped) = rest.strip_prefix('"') {
                    if let Some(end) = stripped.find('"') {
                        return stripped[..end].to_string();
                    }
                }
            }
        }
    }
    "?".into()
}

fn main() -> Result<(), String> {
    let dir = std::env::args().nth(1).ok_or("usage: nam_batch <dir>")?;
    let sr = 48_000.0;
    let block = 256usize;

    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("nam"))
        .collect();
    files.sort();

    // Guitar-ish test signal.
    let n = sr as usize / 4;
    let base: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / sr as f32;
            0.25 * (-t * 3.0).exp() * (2.0 * std::f32::consts::PI * 110.0 * t).sin()
        })
        .collect();

    let (mut ok, mut fail, mut silent) = (0, 0, 0);
    let mut by_arch = std::collections::BTreeMap::<String, usize>::new();
    let mut failures = Vec::new();
    println!("scanning {} models in {dir}\n", files.len());

    for f in &files {
        let name = f.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
        let arch = arch_of(f);
        match NamProcessor::load(f, sr, block) {
            Ok(mut nam) => {
                let mut buf: Vec<f32> = base.iter().flat_map(|&s| [s, s]).collect();
                for c in buf.chunks_mut(block * 2) {
                    nam.process_interleaved(c);
                }
                let rms = (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt();
                *by_arch.entry(arch.clone()).or_default() += 1;
                if rms > 1e-5 {
                    ok += 1;
                } else {
                    silent += 1;
                    println!("  ⚠ SILENT  [{arch}]  {name}");
                }
            }
            Err(e) => {
                fail += 1;
                failures.push((name.to_string(), arch.clone(), e.clone()));
                println!("  ✗ FAIL    [{arch}]  {name}  — {e}");
            }
        }
    }

    println!("\n── summary ──");
    println!("total: {}   loaded+sound: {ok}   silent: {silent}   failed: {fail}", files.len());
    println!("by architecture:");
    for (a, c) in &by_arch {
        println!("  {a}: {c}");
    }
    if !failures.is_empty() {
        println!("\nfailures:");
        for (n, a, e) in &failures {
            println!("  [{a}] {n}: {e}");
        }
    }
    Ok(())
}
