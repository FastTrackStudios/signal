//! What a project file's branch holds: its steps since it was last
//! folded, its items, and named tracks' fields, read straight from the
//! doc.
//!
//! `cargo run -p dawfile-store --example dump -- <Song.session> [branch] [track name…]`

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("a project file");
    let branch = args
        .next()
        .unwrap_or_else(|| formats::store::MAIN.to_owned());
    let names: Vec<String> = args.collect();
    let store = formats::store::Store::open(std::path::Path::new(&path)).expect("the store");
    for b in store.branches().expect("branches") {
        println!("branch {} (from {:?})", b.name, b.parent);
    }
    for step in store.steps(&branch).expect("steps") {
        println!("step {:?} ({} bytes)", step.label, step.bytes);
    }
    let doc = store.load(&branch).expect("the branch");
    println!("items: {}", doc.get_map("items").len());
    let tree = doc.get_tree("tracks");
    for node in tree.get_nodes(false) {
        let Ok(meta) = tree.get_meta(node.id) else {
            continue;
        };
        let value = meta.get_deep_value();
        let loro::LoroValue::Map(map) = &value else {
            continue;
        };
        let name = map
            .get("name")
            .map(|v| format!("{v:?}"))
            .unwrap_or_default();
        if names.iter().any(|n| name.contains(n.as_str())) {
            let pick: Vec<String> = [
                "guid",
                "muted",
                "lanes.count",
                "lanes.play",
                "lanes.names",
                "ext",
            ]
            .iter()
            .map(|k| format!("{k}={:?}", map.get(*k)))
            .collect();
            println!("{name}: {}", pick.join(" "));
        }
    }
}
