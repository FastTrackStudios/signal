//! Compile `docs/guides/signal/*.md` into the site.
//!
//! The guide is a **vault**: the files under `docs/guides/signal/` are
//! notes with frontmatter, and the site ships their markdown rather than
//! HTML generated at build time. Keeping the source means the same text
//! can later feed a knowledge graph or render through the editor, the way
//! keyflow-web's guide does, without the pages being rewritten first.
//!
//! This reads *outside the crate*, which CLAUDE.md otherwise forbids. The
//! rule exists because `include_str!` across a boundary is invisible to
//! cargo and fails at compile time rather than resolution time. A build
//! script is the sanctioned way out: the dependency is explicit, and
//! `cargo:rerun-if-changed` makes cargo aware of it, so editing a guide
//! page rebuilds the site.

// A build script's only way to fail is to panic — cargo has no other error
// channel for one, and a guide that silently compiles to nothing would ship
// an empty `/guide` rather than failing the build. So the panic lints are
// expected here specifically, and nowhere else in this crate.
#![expect(
    clippy::panic,
    clippy::expect_used,
    reason = "a build script reports failure by panicking; there is no other channel"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let guides = guides_dir();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", guides.display());

    // Keyed by (order, slug) so the table of contents is deterministic and
    // a page without an explicit order sorts last rather than randomly.
    let mut pages: BTreeMap<(u32, String), String> = BTreeMap::new();

    let entries = std::fs::read_dir(&guides)
        .unwrap_or_else(|e| panic!("cannot read the guide directory {}: {e}", guides.display()));

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        println!("cargo:rerun-if-changed={}", path.display());

        let slug = path
            .file_stem()
            .expect("a .md path has a stem")
            .to_string_lossy()
            .into_owned();
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

        let front = frontmatter(&raw);
        let title = fm_scalar(front, "title").unwrap_or_else(|| slug.replace('-', " "));
        let order: u32 = fm_scalar(front, "order")
            .and_then(|o| o.parse().ok())
            .unwrap_or(u32::MAX);
        let summary = fm_scalar(front, "summary").unwrap_or_default();
        let body = strip_frontmatter(&raw);

        let mut entry_src = String::new();
        write!(
            entry_src,
            "GuidePage {{ slug: {slug:?}, title: {title:?}, order: {order}, summary: {summary:?}, body: {body:?} }}"
        )
        .expect("writing to a String cannot fail");
        pages.insert((order, slug), entry_src);
    }

    assert!(
        !pages.is_empty(),
        "no guide pages found under {} — the site would ship an empty guide",
        guides.display()
    );

    let mut out = String::from("pub static GUIDE_PAGES: &[GuidePage] = &[\n");
    for entry in pages.values() {
        writeln!(out, "    {entry},").expect("writing to a String cannot fail");
    }
    out.push_str("];\n");

    let dest = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"))
        .join("guide_generated.rs");
    std::fs::write(&dest, out).unwrap_or_else(|e| panic!("cannot write {}: {e}", dest.display()));
}

/// `<repo>/docs/guides/signal`, from this crate's manifest directory.
fn guides_dir() -> PathBuf {
    let manifest = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"),
    );
    manifest
        .parent()
        .and_then(Path::parent)
        .expect("apps/web has a grandparent")
        .join("docs/guides/signal")
}

/// The YAML block between the leading `---` fences, if there is one.
fn frontmatter(raw: &str) -> &str {
    let Some(rest) = raw.strip_prefix("---\n") else {
        return "";
    };
    rest.split_once("\n---").map_or("", |(front, _)| front)
}

/// The note without its frontmatter — what actually gets rendered.
fn strip_frontmatter(raw: &str) -> &str {
    let Some(rest) = raw.strip_prefix("---\n") else {
        return raw;
    };
    rest.split_once("\n---")
        .map_or(raw, |(_, body)| body.trim_start_matches('\n').trim_start())
}

/// One `key: value` scalar out of the frontmatter. Quotes are optional and
/// stripped; anything structured is out of scope, because nothing here
/// needs it yet.
fn fm_scalar(front: &str, key: &str) -> Option<String> {
    front.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        if k.trim() != key {
            return None;
        }
        let v = v.trim().trim_matches('"').trim_matches('\'');
        (!v.is_empty()).then(|| v.to_owned())
    })
}
