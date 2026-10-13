//! Hold a project file open and write to it every so often, as the app
//! does — to read it from another process meanwhile.
//!
//! `cargo run -p dawfile-store --example hold -- <Song.session> <seconds>`

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("a project file");
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);
    let store = dawfile_store::Store::open(std::path::Path::new(&path)).expect("the store");
    let doc = loro::LoroDoc::new();
    for i in 0..seconds {
        let from = doc.oplog_vv();
        doc.get_map("items")
            .insert(&format!("held-{i}"), i64::try_from(i).unwrap_or(0))
            .expect("a write");
        doc.commit();
        let update = doc
            .export(loro::ExportMode::updates(&from))
            .expect("an update");
        store
            .append_update(dawfile_store::MAIN, "Held", &update)
            .expect("appended");
        println!("wrote {i}");
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
