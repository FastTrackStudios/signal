//! `signal-doc-dump` — look inside Signal's live docs. Reads and writes
//! local files only; it never talks to Task.
//!
//! ```text
//! signal-doc-dump <file.loro>...              print what the docs hold, as JSON
//! signal-doc-dump --keys <file.loro>...       one line per entity: key, name, bytes
//! signal-doc-dump --demo <dir> [--layout L]   write the demo library as docs into <dir>
//!                                             (L: per-entity | whole-library | per-setlist;
//!                                             default per-entity)
//! ```
//!
//! A file is a Loro snapshot or update; several files are merged into one
//! doc first, the way a peer would merge them.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use signal_proto::doc::library::{Layout, build};
use signal_proto::doc::{EntityKind, SignalDoc, seed};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("signal-doc-dump: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    match args.split_first() {
        None => Err(usage()),
        Some((flag, _)) if flag == "-h" || flag == "--help" => {
            println!("{}", usage());
            Ok(())
        }
        Some((flag, rest)) if flag == "--demo" => demo(rest),
        Some((flag, files)) if flag == "--keys" => keys(&load(files)?),
        Some(_) => {
            let doc = load(args)?;
            let json = serde_json::to_string_pretty(&doc.dump()).map_err(|e| e.to_string())?;
            println!("{json}");
            Ok(())
        }
    }
}

fn usage() -> String {
    "usage: signal-doc-dump [--keys] <file.loro>... | --demo <dir> [--layout per-entity|whole-library|per-setlist]".to_owned()
}

fn load(files: &[String]) -> Result<SignalDoc, String> {
    if files.is_empty() {
        return Err(usage());
    }
    let doc = SignalDoc::new();
    for file in files {
        let bytes = std::fs::read(file).map_err(|e| format!("{file}: {e}"))?;
        doc.import(&bytes).map_err(|e| format!("{file}: {e}"))?;
    }
    Ok(doc)
}

fn keys(doc: &SignalDoc) -> Result<(), String> {
    let dump = doc.dump();
    for kind in EntityKind::ALL {
        for key in doc.keys(kind) {
            let entity = dump.get(kind.as_str()).and_then(|m| m.get(&key));
            let name = entity
                .and_then(|e| e.get("name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let bytes = entity
                .map(|e| serde_json::to_vec(e).map_or(0, |v| v.len()))
                .unwrap_or(0);
            println!("{key}\t{name}\t{bytes} B json");
        }
    }
    Ok(())
}

fn demo(args: &[String]) -> Result<(), String> {
    let (dir, rest) = args.split_first().ok_or_else(usage)?;
    let layout = match rest {
        [] => Layout::PerEntity,
        [flag, name] if flag == "--layout" => {
            Layout::parse(name).ok_or_else(|| format!("unknown layout {name}"))?
        }
        _ => return Err(usage()),
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let library = seed::demo_library();
    let docs = build(layout, &library, "signal-doc-dump").map_err(|e| e.to_string())?;
    let mut total = 0usize;
    for (name, doc) in &docs {
        let bytes = doc.snapshot().map_err(|e| e.to_string())?;
        let path = dir.join(file_name(name));
        write(&path, &bytes)?;
        total = total.saturating_add(bytes.len());
        println!("{}\t{} B", path.display(), bytes.len());
    }
    println!(
        "{} docs, {total} B, layout {} ({} entities)",
        docs.len(),
        layout.as_str(),
        library.len()
    );
    Ok(())
}

/// A doc name as a file name: `patch:worship-lead` → `patch_worship-lead.loro`.
fn file_name(doc: &str) -> String {
    format!("{}.loro", doc.replace([':', '/'], "_"))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}
