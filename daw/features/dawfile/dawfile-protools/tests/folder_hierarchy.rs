//! Folder/nesting hierarchy parsing tests.
//!
//! Ground truth was AUTHORED with `dawfile_reaper::builder` (known nesting),
//! converted RPP→PTX by the official "PT Reaper Converter" (which supports
//! folders), then parsed back here. The expected `(is_folder, folder_depth)`
//! pairs encode the exact authored shape. See
//! `crates/daw-reaper/examples/gen_folders*.rs`.

use dawfile_protools::read_session;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn fixture(name: &str) -> String {
    format!("{FIXTURES}/{name}")
}

/// folder-nesting.ptx authored shape:
///   FolderA            folder, depth 0
///     ChildA1          leaf,   depth 1
///     ChildA2          folder, depth 1
///       GrandA2a       leaf,   depth 2
///       GrandA2b       leaf,   depth 2
///   SiblingB           leaf,   depth 0
#[test]
fn nested_folders_reconstruct_depth() {
    let s = read_session(fixture("folder-nesting.ptx"), 48000).expect("parse");
    let mut by_order: Vec<(&str, bool, u32)> = s
        .all_tracks()
        .map(|t| (t.name.as_str(), t.is_folder, t.folder_depth))
        .collect();
    by_order.sort_by_key(|_| 0); // keep parse order; collect already in audio+midi order
    let got: std::collections::HashMap<&str, (bool, u32)> =
        by_order.iter().map(|(n, f, d)| (*n, (*f, *d))).collect();

    assert_eq!(got.get("FolderA"), Some(&(true, 0)), "FolderA");
    assert_eq!(got.get("ChildA1"), Some(&(false, 1)), "ChildA1");
    assert_eq!(got.get("ChildA2"), Some(&(true, 1)), "ChildA2");
    assert_eq!(got.get("GrandA2a"), Some(&(false, 2)), "GrandA2a");
    assert_eq!(got.get("GrandA2b"), Some(&(false, 2)), "GrandA2b");
    assert_eq!(got.get("SiblingB"), Some(&(false, 0)), "SiblingB");
}

/// folder-siblings.ptx authored shape: two independent top-level folders.
///   F1 (folder d0) { leafA d1, leafB d1 }
///   F2 (folder d0) { leafC d1, leafD d1 }
///   leafTop (leaf d0)
#[test]
fn sibling_folders_reconstruct_depth() {
    let s = read_session(fixture("folder-siblings.ptx"), 48000).expect("parse");
    let got: std::collections::HashMap<String, (bool, u32)> = s
        .all_tracks()
        .map(|t| (t.name.clone(), (t.is_folder, t.folder_depth)))
        .collect();

    assert_eq!(got.get("F1"), Some(&(true, 0)));
    assert_eq!(got.get("leafA"), Some(&(false, 1)));
    assert_eq!(got.get("leafB"), Some(&(false, 1)));
    assert_eq!(got.get("F2"), Some(&(true, 0)));
    assert_eq!(got.get("leafC"), Some(&(false, 1)));
    assert_eq!(got.get("leafD"), Some(&(false, 1)));
    assert_eq!(got.get("leafTop"), Some(&(false, 0)));
}
