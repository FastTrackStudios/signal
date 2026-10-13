//! A `.session` folder as one store file, measured: its Loro history as
//! a snapshot and each of its objects as a blob — the folder's size
//! against the file's.
//!
//! `cargo run -p dawfile-store --example measure -- <Song.session> <out.fts>`

fn folder_size(dir: &std::path::Path) -> u64 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| {
            let path = e.path();
            if path.is_dir() {
                folder_size(&path)
            } else {
                e.metadata().map_or(0, |m| m.len())
            }
        })
        .sum()
}

fn kind_of(bytes: &[u8]) -> &'static str {
    match bytes.get(..12) {
        Some(head) if head.starts_with(b"<REAPER_PROJ") => "rpp",
        Some(head) if head.starts_with(b"<SOURCE") => "midi",
        Some(head) if head.starts_with(b"<FXCHAIN") => "fx",
        Some(head) if head.starts_with(b"loro") => "loro",
        _ => "other",
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(session), Some(out)) = (args.next(), args.next()) else {
        eprintln!("usage: measure <Song.session> <out.fts>");
        std::process::exit(2);
    };
    let session = std::path::PathBuf::from(session);
    let out = std::path::PathBuf::from(out);
    let _ = std::fs::remove_file(&out);
    let project = dawfile_standalone::DawProject::load(&session).expect("the session");
    let store = dawfile_store::Store::open(&out).expect("the store");
    let mut by_kind: std::collections::BTreeMap<&str, (usize, usize)> = Default::default();
    for id in project.objects().ids() {
        let bytes = project.objects().get(id).expect("an object");
        let kind = kind_of(bytes);
        store.put_blob(kind, bytes).expect("stored");
        let slot = by_kind.entry(kind).or_default();
        slot.0 += 1;
        slot.1 += bytes.len();
    }
    if let Some(history) = project.history() {
        store
            .compact(dawfile_store::MAIN, history)
            .expect("the history");
    }
    let folder = folder_size(&session);
    let file = std::fs::metadata(&out).map_or(0, |m| m.len());
    let (count, raw, packed) = store.blob_totals().expect("totals");
    println!(
        "folder {:.1} MB  ->  file {:.1} MB",
        folder as f64 / 1e6,
        file as f64 / 1e6
    );
    println!(
        "blobs {count}: {:.1} MB raw, {:.1} MB stored",
        raw as f64 / 1e6,
        packed as f64 / 1e6
    );
    for (kind, (n, bytes)) in by_kind {
        println!("  {kind:6} {n:4} objects {:8.1} MB raw", bytes as f64 / 1e6);
    }
    println!(
        "history: {}",
        if project.history().is_some() {
            "snapshot stored"
        } else {
            "none"
        }
    );
}
