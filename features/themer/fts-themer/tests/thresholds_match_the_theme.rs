//! The panel and the theme cannot drift apart on collapse heights.
//!
//! This is the seam #148 exists to close. The Dioxus strip reads a Rust
//! constant; REAPER reads `rtconfig.txt`; both encode the same question —
//! at what height does this part of the strip disappear. Nothing else in
//! the two implementations overlaps, and nothing would have noticed a
//! mismatch: the app would collapse at one height and the host at another,
//! and both would look deliberate.
//!
//! So the shipped theme is parsed here and compared against the constant.
//!
//! # Why these are all `#[ignore]`d
//!
//! The theme itself is not in this repo. The UI migration brought the
//! theme *crates* across — `daw-theme`, `daw-theme-art`,
//! `daw-theme-reaper`, `fts-themer` — and left the asset directory they
//! read behind: `fts-theme/FastTrackStudio/`, holding `rtconfig.txt`
//! and `.source-art`. Every test here opens that file, so all five fail
//! on a missing path rather than on a mismatch, and they have failed
//! since the migration landed.
//!
//! Ignored rather than deleted because the seam they cover is real and
//! unguarded in the meantime: the Dioxus strip reads a Rust constant and
//! REAPER reads `rtconfig.txt`, and nothing else would notice them
//! drifting apart. Ignored rather than left red because a permanently
//! failing gate stops telling anyone anything — a build that is always
//! broken is the same as no build check at all.
//!
//! Tracked as session#13. Restoring the assets is what un-ignores
//! these; the `#[ignore]`s come
//! straight back off, and they should fail loudly if the assets
//! arrive in a different shape.

use fts_themer::thresholds;

/// The theme REAPER actually loads.
fn theme() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fts-theme/FastTrackStudio/rtconfig.txt"
    );
    std::fs::read_to_string(path).expect("the shipped layout file")
}

/// Every generated threshold, read back out of the file and compared with
/// the constant it came from.
#[test]
#[ignore = "the shipped theme is not in this repo — session#13"]
fn the_layout_file_states_the_rust_thresholds() {
    let text = theme();
    let wanted = thresholds::generated_lines();
    let names: Vec<&str> = wanted.iter().map(|t| t.name).collect();

    let found = thresholds::parse(&text, &names).expect("parse the thresholds back");
    for (t, (name, value)) in wanted.iter().zip(found) {
        assert_eq!(name, t.name);
        assert_eq!(
            value, t.value,
            "`{}` is {value} in the theme and {} in the Rust constant — run \
             `fts-themer thresholds`",
            t.name, t.value
        );
    }
}

/// The three section heights are not generated — this theme states them
/// inside expressions — so they are checked instead.
#[test]
#[ignore = "the shipped theme is not in this repo — session#13"]
fn the_section_heights_agree_with_the_constant() {
    let wrong = thresholds::section_heights_agree(&theme());
    assert!(wrong.is_empty(), "section heights drifted: {wrong:#?}");
}

/// The stated offsets — the column chain, the arm's cell, the input
/// field — sit inside expressions too, so they get the same treatment:
/// the shipped file must still state the literal each geometry constant
/// was read from. #239's other half.
#[test]
#[ignore = "the shipped theme is not in this repo — session#13"]
fn the_stated_offsets_agree_with_the_geometry() {
    let wrong = thresholds::offsets_agree(&theme());
    assert!(wrong.is_empty(), "stated offsets drifted: {wrong:#?}");
}

/// Splicing the shipped file must be a no-op: the theme is already at the
/// constant's values, so the generator has nothing to write. This is what
/// makes running it safe on a file a themer edits by hand.
#[test]
#[ignore = "the shipped theme is not in this repo — session#13"]
fn splicing_the_shipped_theme_changes_nothing() {
    let text = theme();
    let (out, changed) = thresholds::splice(&text, &thresholds::generated_lines()).unwrap();
    assert_eq!(
        changed, 0,
        "the shipped theme is not at the constant's values"
    );
    assert_eq!(out, text, "a no-op splice still rewrote the file");
}

/// And running it twice is the same as running it once.
#[test]
#[ignore = "the shipped theme is not in this repo — session#13"]
fn the_splice_is_idempotent_on_the_real_file() {
    let text = theme();
    let mut moved = thresholds::generated_lines();
    moved[0].value += 5.0;

    let (once, first) = thresholds::splice(&text, &moved).unwrap();
    assert_eq!(first, 1);
    let (twice, second) = thresholds::splice(&once, &moved).unwrap();
    assert_eq!(second, 0);
    assert_eq!(once, twice);

    // Only the one line moved — every other line of 3000-odd is identical.
    let differing = text
        .lines()
        .zip(once.lines())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        differing, 1,
        "the splice touched more than the line it owns"
    );
    assert_eq!(
        text.lines().count(),
        once.lines().count(),
        "line count changed"
    );
}
