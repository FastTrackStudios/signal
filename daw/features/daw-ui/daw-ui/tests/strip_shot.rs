//! The strip, rasterised, at REAPER's own strip height.
//!
//! The convergence loop needs a picture of the strip on every edit, and
//! booting REAPER for one takes minutes. This paints the same component
//! through blitz-dom — the renderer REAPER embeds — so the picture is
//! honest and arrives in seconds.
//!
//! `MIXER_HEIGHT=228 cargo test -p daw-ui --test strip_shot -- --nocapture`
//! writes `target/theme-shots/strip-dioxus.png`, which
//! `just` (or the comparison script) montages against a crop of the real
//! MCP. 228 is not arbitrary: it is the height of the strip in the
//! reference REAPER screenshot, measured off its coloured band.

use daw_proto::Track;
use daw_ui::components::mixer::ChannelStripPreview;
use daw_ui::controls::TrackStore;
use dioxus::prelude::*;

/// REAPER's strip in the reference shot, measured rather than assumed:
/// its index plate is 20 rows at 1x and 60 in a 3x zoom, which fixes the
/// zoom, and the strip is 684 rows in that zoom.
const REFERENCE_HEIGHT: f32 = 228.0;

fn kick() -> Track {
    Track {
        guid: "kick".into(),
        index: 0,
        name: "Kick".into(),
        // REAPER's raw track colour, not the colour of the band it paints:
        // the reference's band is #A8415C and the panel tint is a flat 0.70,
        // so the track itself is #F05D83.
        color: Some(0xf0_5d_83),
        // Unity, which is where the reference screenshot's tracks are —
        // the cap should land where REAPER's does, not at the top.
        volume: 1.0,
        ..Default::default()
    }
}

#[test]
fn paint_the_strip_for_comparison() {
    let height: f32 = std::env::var("MIXER_HEIGHT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(REFERENCE_HEIGHT);

    fn app() -> Element {
        let mut store = use_hook(TrackStore::new);
        use_hook(|| {
            store.seed([kick()]);
            provide_context(store);
        });
        let height: f32 = std::env::var("MIXER_HEIGHT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(REFERENCE_HEIGHT);
        rsx! {
            // Pinned to the origin rather than reset through a stylesheet:
            // the UA's 8px body margin would inset the whole strip and read
            // as a left gutter the strip does not have.
            div {
                style: "position:absolute; left:0; top:0; \
                        background:#1e1e1e; padding:0; margin:0; width:86px;",
                ChannelStripPreview { track: kick(), index: 0, height }
            }
        }
    }

    // Relative to the manifest, not the CWD: `cargo test` runs a test
    // binary from its package directory, so "target/theme-shots" lands
    // three levels below the one the tools read.
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/theme-shots");
    let out = out.as_path();
    std::fs::create_dir_all(out).unwrap();
    let path = out.join("strip-dioxus.png");

    // The window is the strip plus the UA's 8px body margin, which nothing
    // in a headless document resets — neither a stylesheet nor an absolute
    // position escapes it, so the margin is budgeted for and cropped off.
    dioxus_test::render(app)
        .with_window_size(86 + BODY_MARGIN * 2, height.ceil() as u32 + BODY_MARGIN * 2)
        .build()
        .render_png(&path);

    crop_to_strip(&path, height.ceil() as u32);
    println!("wrote {} at height {height}", path.display());
}

/// The UA body margin, which the headless document applies and no reset
/// this test could write has removed.
const BODY_MARGIN: u32 = 8;

/// Trim the margin off, so the PNG is the strip and nothing else and a
/// column of it can be measured against a crop of REAPER's own.
fn crop_to_strip(path: &std::path::Path, height: u32) {
    crop_to_strip_sized(path, 86, height)
}

fn crop_to_strip_sized(path: &std::path::Path, width: u32, height: u32) {
    let status = std::process::Command::new("magick")
        .arg(path)
        .arg("-crop")
        .arg(format!("{width}x{height}+{BODY_MARGIN}+{BODY_MARGIN}"))
        .arg("+repage")
        .arg(path)
        .status();
    match status {
        Ok(s) if s.success() => {}
        // No ImageMagick is not a test failure: the PNG is still written,
        // just with its margin on.
        _ => eprintln!(
            "note: `magick` unavailable, {} keeps its margin",
            path.display()
        ),
    }
}

/// The FX pill on its own, for the same loop.
#[test]
fn paint_the_fx_pill() {
    fn app() -> Element {
        let mut store = use_hook(TrackStore::new);
        use_hook(|| {
            store.seed([kick()]);
            provide_context(store);
        });
        rsx! {
            div {
                style: "position:absolute; left:0; top:0; background:#1e1e1e; width:86px; height:33px;",
                div { style: "position:absolute; left:7px; top:7px;",
                    daw_ui::controls::FxButton { track: "kick", width: 43.0 }
                }
            }
        }
    }

    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/theme-shots");
    let path = out.join("fx-pill.png");
    dioxus_test::render(app)
        .with_window_size(86 + BODY_MARGIN * 2, 33 + BODY_MARGIN * 2)
        .build()
        .render_png(&path);
    crop_to_strip_sized(&path, 86, 33);
    println!("wrote {}", path.display());
}

/// The label half alone, to isolate the pill's own geometry from the
/// button's placement of it.
#[test]
fn paint_the_fx_label_half() {
    fn app() -> Element {
        rsx! {
            div { style: "position:absolute; left:0; top:0; background:#1e1e1e; width:60px; height:30px;",
                div { style: "position:absolute; left:5px; top:4px;",
                    daw_theme_art::vector_controls::FxControl {
                        pane: None,
                        part: daw_theme_art::vector_controls::FxPart::Label,
                        chain: daw_theme_art::vector_controls::FxChain::Empty,
                        bypass: daw_theme_art::vector_controls::FxBypass::Empty,
                        family: daw_theme_art::vector_controls::FxFamily::Mixer,
                        width: Some(43),
                        height: Some(22),
                        at: daw_theme_art::vector_controls::Interaction::Normal,
                    }
                }
            }
        }
    }
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/theme-shots");
    let path = out.join("fx-label.png");
    dioxus_test::render(app)
        .with_window_size(60 + BODY_MARGIN * 2, 30 + BODY_MARGIN * 2)
        .build()
        .render_png(&path);
    crop_to_strip_sized(&path, 60, 30);
    println!("wrote {}", path.display());
}

/// The track-panel reference was shot separately and its Kick is a
/// different colour from the mixer reference's: #9D3C55 on screen, so
/// #E0567A before the panel tint. The fixture carries the reference's
/// colour or every pixel of the comparison is a colour difference.
fn tcp_kick() -> Track {
    Track {
        color: Some(0xe0_56_7a),
        // The reference's tracks are on input 1; without it the combo
        // reads "No input" and the comparison is about the fixture.
        record_input: daw_proto::track::RecordInput::Audio { channel: 0 },
        ..kick()
    }
}

/// The TCP row, painted for the same loop.
#[test]
fn paint_the_track_row() {
    fn app() -> Element {
        let mut store = use_hook(TrackStore::new);
        use_hook(|| {
            store.seed([tcp_kick()]);
            provide_context(store);
        });
        rsx! {
            div {
                style: "position:absolute; left:0; top:0; background:#1e1e1e; \
                        width:343px; height:71px;",
                daw_ui::components::tcp::TrackRow { track: tcp_kick(), index: 0 }
            }
        }
    }

    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/theme-shots");
    std::fs::create_dir_all(&out).unwrap();
    let path = out.join("track-row.png");
    dioxus_test::render(app)
        .with_window_size(343 + BODY_MARGIN * 2, 71 + BODY_MARGIN * 2)
        .build()
        .render_png(&path);
    crop_to_strip_sized(&path, 343, 71);
    println!("wrote {}", path.display());
}

/// Both panels, several tracks, one picture.
///
/// The strips and the rows share every control, so the thing worth
/// looking at is whether they still read as one instrument when a handful
/// of tracks in different states are put side by side — a soloed track
/// next to a muted one next to an armed one.
#[test]
fn paint_the_panels() {
    fn demo() -> Vec<Track> {
        // Built on the reference fixtures above rather than a third
        // hand-rolled Track: `tcp_kick` already carries the reference's
        // volume and record input, so the montage's tracks differ from it
        // only where the montage means them to.
        let t = |guid: &str, name: &str, colour: u32, index: u32| Track {
            guid: guid.into(),
            name: name.into(),
            color: Some(colour),
            index,
            record_input: daw_proto::track::RecordInput::Audio { channel: index },
            ..tcp_kick()
        };
        vec![
            Track {
                armed: true,
                ..t("kick", "Kick", 0xe0_56_7a, 0)
            },
            Track {
                armed: true,
                soloed: true,
                ..t("snare", "Snare", 0xe0_56_7a, 1)
            },
            t("oh", "OH", 0xe0_56_7a, 2),
            Track {
                muted: true,
                ..t("bass", "Bass", 0x55_88_e0, 3)
            },
            t("gtr", "Gtr", 0x42_c8_e0, 4),
            Track {
                fx_count: 2,
                ..t("keys", "Keys", 0xe0_a8_42, 5)
            },
        ]
    }

    fn app() -> Element {
        let mut store = use_hook(TrackStore::new);
        use_hook(|| {
            store.seed(demo());
            provide_context(store);
        });
        rsx! {
            div {
                style: "position:absolute; left:0; top:0; background:#1e1e1e; \
                        width:900px; height:800px;",
                // The track panel, stacked.
                div { style: "position:absolute; left:0; top:0;",
                    // The last row is tall, because two of REAPER's
                    // controls — phase and the fixed-lanes button — only
                    // exist above a height and a picture of the panel
                    // without them is a picture of half of it.
                    for (i, track) in demo().into_iter().enumerate() {
                        div {
                            key: "{track.guid}",
                            style: "position:absolute; left:0; top:{i as f32 * 71.0}px;",
                            daw_ui::components::tcp::TrackRow {
                                track,
                                index: i as u32,
                                height: if i == 5 { 120.0 } else { 70.0 },
                            }
                        }
                    }
                }
                // The mixer, beside it.
                div { style: "position:absolute; left:360px; top:0; display:flex;",
                    for (i, track) in demo().into_iter().enumerate() {
                        div { key: "{track.guid}", style: "flex:0 0 auto;",
                            daw_ui::components::mixer::ChannelStripPreview {
                                track, index: i as u32, height: 430.0,
                            }
                        }
                    }
                }
            }
        }
    }

    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/theme-shots");
    let path = out.join("panels.png");
    dioxus_test::render(app)
        .with_window_size(900 + BODY_MARGIN * 2, 800 + BODY_MARGIN * 2)
        .build()
        .render_png(&path);
    crop_to_strip_sized(&path, 900, 460);
    println!("wrote {}", path.display());
}
