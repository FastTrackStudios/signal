//! Regenerating the faces with no source change writes the same bytes:
//! node ids are stable, so `examples/plugins` doesn't churn and a live
//! reload keeps its state.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).expect("read dir") {
            let p = e.expect("entry").path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.insert(p.strip_prefix(root).expect("under root").to_path_buf(), std::fs::read(&p).expect("read"));
            }
        }
    }
    out
}

#[test]
fn a_second_regeneration_is_byte_identical() {
    let tmp = std::env::temp_dir().join(format!("plugin-faces-stable-{}", std::process::id()));
    let (a, b) = (tmp.join("a"), tmp.join("b"));
    let n = plugin_faces::write_all(&a);
    assert_eq!(plugin_faces::write_all(&b), n);
    let (fa, fb) = (files(&a), files(&b));
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(fa.keys().filter(|p| p.extension().is_some_and(|e| e == "frame")).count() == n, "one .frame per face");
    assert_eq!(fa.keys().collect::<Vec<_>>(), fb.keys().collect::<Vec<_>>(), "same files");
    let differ: Vec<_> = fa.iter().filter(|(p, bytes)| fb.get(*p) != Some(*bytes)).map(|(p, _)| p.display().to_string()).collect();
    assert!(differ.is_empty(), "files differ between two regenerations: {differ:?}");
}

#[test]
fn ids_are_unique_and_follow_the_tree() {
    for f in plugin_faces::all() {
        let s = &f.scene;
        for (id, n) in &s.nodes {
            assert_eq!(*id, n.id, "{}: a node keyed by another id", f.file);
            for c in &n.children {
                assert_eq!(s.node(*c).and_then(|c| c.parent), Some(*id), "{}: child's parent", f.file);
            }
        }
    }
}
