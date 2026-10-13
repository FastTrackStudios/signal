//! Folder items over the reference kit, from the session's real peaks.
//!
//! A folder row draws one item per take, folded from its children's
//! takes through the take-peaks RPC. The fold is the decision and the
//! picture is what you see, so both halves are committed beside the
//! golden session (`features/dynamic-template/fixtures/golden/folder-items/`),
//! following the #48 amendment:
//!
//! - **the fold** (`<slug>.fold`) — every folder's children, its
//!   revision and its mute mask, and the fold itself over the window on
//!   a 256-column reference grid. The picture's own grid is one column
//!   per pixel and would commit hundreds of kilobytes of text a zoom;
//!   256 columns of the same fold over the same window is the same
//!   decision at a resolution a diff can show. Compared **byte for
//!   byte** — it is arithmetic over a deterministic WAV and is the same
//!   on every machine.
//! - **the picture** (`<slug>.png`) — compared structurally with the
//!   repo's own threshold (24 of 255 per channel, zero pixels past it),
//!   because two GPUs disagree about antialiasing by a few levels and
//!   never about a fold.
//!
//! Three zooms, because the fold lives on a column grid: the whole
//! session, eight bars, and one bar. A fold recorded once and scaled —
//! the way the arrangement's rectangles are — would be wrong at two of
//! them, which is why the zoom is in the cache key.
//!
//! `just daw-folder-items` rewrites all of it.

use std::path::{Path, PathBuf};
use std::process::Command;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

/// The size every sheet is committed at: three folder rows, wide enough
/// that a column is a pixel.
const SIZE: &str = "1280x480";
/// A per-channel difference this large or smaller is rasterisation, not
/// a different picture.
const STRUCTURAL: i16 = 24;
/// How many pixels may differ structurally: none.
const STRUCTURAL_TOLERANCE: usize = 0;
/// Below this many colours, a render is an empty sheet rather than three
/// folder items.
const MIN_COLOURS: usize = 16;
/// The child muted and hidden in the mute/hide pass — the snare's top
/// mic, which every kit has and which is loud enough to change a
/// picture when it leaves the sum.
const SUBJECT: &str = "Top";

/// The three zooms, and the window each covers in project seconds.
///
/// The golden session is eight bars of song at 120 in 4/4 repeated
/// across its sections, so sixteen seconds is eight bars and two
/// seconds is one.
const ZOOMS: [(&str, f64, f64); 3] = [
    ("session", 0.0, 158.0),
    ("eight-bars", 0.0, 16.0),
    ("one-bar", 2.0, 4.0),
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../features/dynamic-template/fixtures/golden")
}

fn committed() -> PathBuf {
    fixtures().join("folder-items")
}

/// Render one zoom through the bench, writing the sheet to `png` and the
/// fold to `fold`, plus `<png>.hidden.png` and `<png>.muted.png` from
/// the same process.
///
/// r[impl flow.drums.comping.folder-items]
fn render(window: (f64, f64), png: &Path, fold: &Path) -> Result<String> {
    let bench = env!("CARGO_BIN_EXE_bench");
    let output = Command::new(bench)
        .arg(fixtures().join("template.rpp"))
        .env("FTS_BENCH_FOLDER_ITEMS", png)
        .env("FTS_BENCH_FOLD", fold)
        .env("FTS_BENCH_SIZE", SIZE)
        .env("FTS_BENCH_WINDOW", format!("{},{}", window.0, window.1))
        .env("FTS_BENCH_FOLDER_HIDE", SUBJECT)
        .env("FTS_BENCH_FOLDER_MUTE", SUBJECT)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "bench failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    if !png.is_file() || !fold.is_file() {
        return Err(format!(
            "bench wrote nothing: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The pixels of a PNG as RGBA8, with its size.
fn pixels(path: &Path) -> Result<(u32, u32, Vec<u8>)> {
    let image = image::open(path)?.into_rgba8();
    let (w, h) = image.dimensions();
    Ok((w, h, image.into_raw()))
}

/// The count of pixels past the structural threshold, and the largest
/// per-channel difference among all of them.
fn compare(a: &[u8], b: &[u8]) -> (usize, i16) {
    let mut structural = 0_usize;
    let mut worst = 0_i16;
    for (pa, pb) in a.chunks(4).zip(b.chunks(4)) {
        let delta = pa
            .iter()
            .zip(pb)
            .map(|(x, y)| i16::from(*x).saturating_sub(i16::from(*y)).saturating_abs())
            .max()
            .unwrap_or(0);
        if delta > STRUCTURAL {
            structural = structural.saturating_add(1);
        }
        worst = worst.max(delta);
    }
    (structural, worst)
}

/// How many distinct colours a render uses — the cheap way to tell a
/// sheet of folder items from a picture of the background.
fn colours(pixels: &[u8]) -> usize {
    pixels
        .chunks(4)
        .map(<[u8]>::to_vec)
        .collect::<std::collections::HashSet<_>>()
        .len()
}

fn beside(png: &Path, suffix: &str) -> PathBuf {
    png.with_extension(format!("{suffix}.png"))
}

/// The reference kit's folder item, at three zooms, against its
/// committed fold and its committed picture.
///
/// r[verify flow.drums.comping.folder-items]
/// r[verify flow.drums.comping.folder-item-colours]
/// r[verify flow.guitars.folder-items]
#[test]
fn every_zoom_renders_to_its_committed_fixture() -> Result<()> {
    let update = std::env::var_os("FTS_UPDATE_GOLDEN").is_some();
    // The media the project references is generated, not committed.
    dynamic_template::golden_session::write_media(&fixtures())?;
    let dir = committed();
    if update {
        std::fs::create_dir_all(&dir)?;
    }
    let scratch = tempfile::tempdir()?;
    let mut failures = Vec::new();
    for (slug, from, to) in ZOOMS {
        let fresh_png = scratch.path().join(format!("{slug}.png"));
        let fresh_fold = scratch.path().join(format!("{slug}.fold"));
        render((from, to), &fresh_png, &fresh_fold)?;
        let committed_png = dir.join(format!("{slug}.png"));
        let committed_fold = dir.join(format!("{slug}.fold"));
        if update {
            std::fs::copy(&fresh_png, &committed_png)?;
            std::fs::copy(&fresh_fold, &committed_fold)?;
            continue;
        }
        if !committed_png.is_file() || !committed_fold.is_file() {
            failures.push(format!(
                "{slug}: no committed fixture beside {}",
                committed_png.display()
            ));
            continue;
        }

        // The fold, byte for byte.
        let fresh = std::fs::read_to_string(&fresh_fold)?;
        let old = std::fs::read_to_string(&committed_fold)?;
        if fresh != old {
            let (n, a, b) = fresh
                .lines()
                .zip(old.lines())
                .enumerate()
                .find(|(_, (a, b))| a != b)
                .map_or((0, "", ""), |(n, (a, b))| (n.saturating_add(1), a, b));
            failures.push(format!(
                "{slug}: the folders fold differently ({} lines, committed {}); \
                 first difference at line {n}:\n    folded:    {a}\n    committed: {b}",
                fresh.lines().count(),
                old.lines().count()
            ));
        }

        // And the picture, structurally.
        let (w, h, a) = pixels(&fresh_png)?;
        let (cw, ch, b) = pixels(&committed_png)?;
        if (w, h) != (cw, ch) {
            failures.push(format!("{slug}: rendered {w}x{h}, committed {cw}x{ch}"));
            continue;
        }
        let used = colours(&a);
        if used < MIN_COLOURS {
            failures.push(format!(
                "{slug}: rendered {used} colours — a sheet with no folder item on it"
            ));
            continue;
        }
        let (structural, worst) = compare(&a, &b);
        if structural > STRUCTURAL_TOLERANCE {
            // Kept where the fixtures are NOT, so a failing run does not
            // leave untracked PNGs beside the committed ones.
            let kept = std::env::temp_dir().join(format!("fts-folder-items-{slug}.fresh.png"));
            std::fs::copy(&fresh_png, &kept)?;
            failures.push(format!(
                "{slug}: {structural} pixels differ structurally (worst channel by {worst} of \
                 255); fresh render kept at {}",
                kept.display()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "folder-item fixtures drifted — if the change is intended, run \
         `just daw-folder-items` and commit the result:\n{}",
        failures.join("\n")
    );
    Ok(())
}

/// A muted child changes the picture; a hidden one replays the held one
/// byte for byte.
///
/// Both passes happen inside one bench process, against the same
/// rasteriser, which is what makes "byte-identical" a claim worth
/// making: it is the cached picture being replayed, not two renders that
/// happen to agree.
///
/// r[verify flow.drums.comping.folder-items]
#[test]
fn a_hidden_child_replays_the_picture_and_a_muted_one_changes_it() -> Result<()> {
    dynamic_template::golden_session::write_media(&fixtures())?;
    let scratch = tempfile::tempdir()?;
    let png = scratch.path().join("sheet.png");
    let fold = scratch.path().join("sheet.fold");
    let log = render((0.0, 16.0), &png, &fold)?;

    let base = std::fs::read(&png)?;
    let hidden = std::fs::read(beside(&png, "hidden"))?;
    let muted = std::fs::read(beside(&png, "muted"))?;
    assert_eq!(
        base, hidden,
        "hiding {SUBJECT} must replay the cached picture byte for byte\n{log}"
    );
    assert_ne!(
        base, muted,
        "muting {SUBJECT} must leave the sum and change the picture\n{log}"
    );

    // And structurally, not by a byte in a PNG header.
    let (_, _, a) = pixels(&png)?;
    let (_, _, c) = pixels(&beside(&png, "muted"))?;
    let (structural, worst) = compare(&a, &c);
    assert!(
        structural > 0 && worst > STRUCTURAL,
        "muting moved {structural} pixels, worst {worst}\n{log}"
    );

    // The cache says why: the hidden pass replayed every picture, the
    // muted pass rebuilt the folders the mic is in.
    assert!(
        log.contains("hidden.png — picture cache: 3 hits, 3 misses"),
        "the hidden pass should be all hits:\n{log}"
    );
    assert!(
        log.contains("muted.png — picture cache: 4 hits, 5 misses"),
        "the muted pass should rebuild the two folders holding {SUBJECT}:\n{log}"
    );
    Ok(())
}

/// The negative control for the fixture comparison: two of the three
/// zooms are different pictures and different folds by far more than the
/// tolerances, so neither half could pass by being lenient.
#[test]
fn two_zooms_differ_by_more_than_the_tolerances() -> Result<()> {
    let dir = committed();
    let (_, _, a) = pixels(&dir.join("eight-bars.png"))?;
    let (_, _, b) = pixels(&dir.join("one-bar.png"))?;
    let (structural, worst) = compare(&a, &b);
    assert!(
        structural > 10_000,
        "{structural} pixels differ structurally"
    );
    assert!(worst > STRUCTURAL, "worst {worst}");
    let fold_a = std::fs::read_to_string(dir.join("eight-bars.fold"))?;
    let fold_b = std::fs::read_to_string(dir.join("one-bar.fold"))?;
    assert_ne!(fold_a, fold_b);
    Ok(())
}

/// Every zoom has a committed fixture with three folders in it, one of
/// them folded by side — a double, drawn as two halves of one stereo
/// waveform.
///
/// r[verify flow.guitars.folder-items]
#[test]
fn every_zoom_has_a_fixture_with_a_double_in_it() -> Result<()> {
    for (slug, _, _) in ZOOMS {
        let dir = committed();
        let (w, h, committed_px) = pixels(&dir.join(format!("{slug}.png")))?;
        assert_eq!((w, h), (1280, 480), "{slug}");
        assert!(colours(&committed_px) >= MIN_COLOURS, "{slug} is blank");
        let fold = std::fs::read_to_string(dir.join(format!("{slug}.fold")))?;
        assert_eq!(
            fold.matches("\nfolder ").count(),
            3,
            "{slug} should fold three folders"
        );
        assert_eq!(
            fold.matches("group-by side").count(),
            1,
            "{slug} should fold exactly one of them by side"
        );
        assert_eq!(
            fold.matches("group-by role").count(),
            2,
            "{slug} should fold two of them by role"
        );
    }
    Ok(())
}
