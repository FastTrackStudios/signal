//! Every visualiser in the rig, in one window, with the design rig's
//! synthetic guitar running through them.
//!
//!     just viz-gallery
//!
//! # Why
//!
//! The rig shows one delay family, one reverb family and one modulation
//! engine at a time — whichever happens to be loaded. Comparing two of them
//! means loading one, looking, loading the other, and remembering. That is
//! not a comparison, and it is how a panel that looks like another panel
//! ships: nobody ever sees them side by side.
//!
//! The contact sheets each effect ships (`cargo run -p delay-ui --example
//! family_sheet`) solve half of it — they are stills, rendered offline, one
//! effect at a time. This is the other half: everything at once, moving, on
//! the same clock, fed the same signal. Half of what separates these
//! pictures only exists in motion, and all of what separates the compressor
//! and the EQ needs audio going through them.
//!
//! The audio is `signal_guitar::design`'s — the same synthetic guitar the
//! `--design` rig plays, so what is on screen here is what the rig would
//! show if you could load every algorithm at once.

use dioxus::prelude::*;
use signal_guitar::design;

use comp_ui::viz::CompViz;
use delay_ui::viz::{DelayViz, Family as DelayFamily};
use eq_ui::eq_graph::{AnalyzerSnapshot, EqGraph};
use eq_ui::eq_graph_model::EqBand;
use modulation_ui::viz::{Engine, ModViz};
use reverb_ui::viz::{Family as VerbFamily, ReverbViz};

/// Cyan leads modulation, pink leads motion, blue delay, purple reverb — the
/// rig's own grouping, so the gallery reads the way the rack does.
const CYAN: [u8; 3] = [34, 211, 238];
const PINK: [u8; 3] = [244, 114, 182];
const BLUE: [u8; 3] = [59, 130, 246];
const VIOLET: [u8; 3] = [167, 139, 250];
const GREY: [u8; 3] = [228, 228, 231];

const BPM: f32 = 120.0;

fn main() {
    use dioxus_native::{Config, LogicalSize, WindowAttributes, launch_cfg};
    // The same launcher the app uses: Blitz → Vello → winit. A WebView
    // cannot host a custom widget at all, and every panel here is one.
    let window = WindowAttributes::default()
        .with_title("FTS — visualiser gallery")
        .with_surface_size(LogicalSize::new(2560.0, 1440.0))
        .with_min_surface_size(LogicalSize::new(900.0, 600.0));
    launch_cfg(
        Gallery,
        vec![],
        vec![Box::new(Config::new().with_window_attributes(window))],
    );
}

#[component]
fn Gallery() -> Element {
    // One clock for the whole window, so every panel is showing the same
    // instant of the same performance. Panels keep their own animation
    // clocks; this is the AUDIO's clock, which is a different thing.
    let mut now = use_signal(|| 0.0_f32);
    use_hook(|| {
        let start = std::time::Instant::now();
        let updater = dioxus_core::schedule_update();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(33));
                let _ = start;
                updater();
            }
        });
    });
    // Advance on every render rather than from the thread: writing a signal
    // off the runtime is the thing this whole tree is careful not to do.
    let t = use_hook(std::time::Instant::now);
    now.set(t.elapsed().as_secs_f32());
    let secs = now();

    let beat_ms = 60_000.0 / BPM;
    let spectrum = design::spectrum(secs, 96);
    let (wave_in, wave_gr) = design::traces(secs, 120);
    let env = design::envelope(secs);
    // The design rig reports peaks 0..1; the compressor panel draws its own
    // display range, the same conversion `comp_surface` does.
    let in_db = 20.0 * env.max(1e-5).log10();
    let bands = use_signal(demo_bands);

    // The EQ's band popups expect a drag context. Without one the graph
    // panics on its first pointer event and takes the window with it — the
    // same thing that broke the rig when the real EQ first landed there.
    rsx! {
        fts_audio_ui::drag::DragProvider {
        div {
            style: "width:100vw; height:100vh; background:#050506; \
                    display:flex; flex-direction:column; overflow:hidden; \
                    font-family: ui-sans-serif, system-ui, sans-serif;",

            // The two slots hold DIFFERENT machines — the rig's MOD picker
            // offers chorus, phaser and flanger, and MOTION offers tremolo,
            // vibrato and rotary. Listing all six in both rows drew each
            // engine twice in two colours and said the slots were
            // interchangeable, which is the one thing they are not.
            Band { title: "MODULATION — the three that colour a signal",
                for engine in Engine::COLOURING {
                    Cell { label: engine_name(engine),
                        ModViz { engine, rate: 0.9, depth: 0.75, mix: 0.55, on: true, color: CYAN }
                    }
                }
            }
            Band { title: "MOTION — the three that move it",
                for engine in Engine::MOVING {
                    Cell { label: engine_name(engine),
                        ModViz { engine, rate: 1.4, depth: 0.65, mix: 0.6, on: true, color: PINK }
                    }
                }
            }
            Band { title: "DELAY — deep blue",
                for family in DELAY_FAMILIES {
                    Cell { label: delay_name(family),
                        DelayViz {
                            taps: taps(beat_ms),
                            win_ms: beat_ms * 7.0,
                            on: true,
                            beat_ms,
                            division: "1/4".to_string(),
                            family,
                            color: BLUE,
                        }
                    }
                }
            }
            Band { title: "REVERB — purple",
                for family in VerbFamily::ALL {
                    Cell { label: verb_name(family),
                        ReverbViz {
                            decay: 2.6,
                            density: 0.55,
                            predelay: 0.035,
                            mix: 0.5,
                            damp: 0.35,
                            family,
                            on: true,
                            beat_ms,
                            color: VIOLET,
                        }
                    }
                }
            }
            Band { title: "DYNAMICS — the synthetic guitar, live", grow: 1.4,
                Cell { label: "compressor".to_string(),
                    CompViz {
                        threshold: -18.0,
                        ratio: 4.0,
                        knee: 6.0,
                        in_db,
                        gr_db: (in_db + 18.0).max(0.0) * 0.75,
                        wave: (scaled(&wave_in), scaled(&wave_gr)),
                        on: true,
                        color: GREY,
                    }
                }
                Cell { label: "eq — the real graph, glowing".to_string(),
                    EqGraph {
                        bands,
                        db_range: 12.0,
                        // The same two axes the rig feeds it: the glow reads
                        // raw dBFS, the vector pass reads the level axis.
                        spectrum_db: spectrum.clone(),
                        analyzer_snapshot: analyzer_of(&spectrum),
                        show_hints: false,
                    }
                }
            }
        }
        }
    }
}

/// The analyser, on the level axis the EQ's snapshot expects.
fn analyzer_of(spectrum: &[f32]) -> Option<AnalyzerSnapshot> {
    if spectrum.len() < 2 {
        return None;
    }
    let last = (spectrum.len() - 1) as f32;
    let freq_hz = (0..spectrum.len())
        .map(|i| 20.0 * 1000f32.powf(i as f32 / last))
        .collect();
    Some(AnalyzerSnapshot {
        freq_hz,
        pre_db: spectrum.to_vec(),
        range_db: 90.0,
        ..AnalyzerSnapshot::default()
    })
}

/// A curve with something happening in it, so the glow has bands to light.
fn demo_bands() -> Vec<EqBand> {
    let mut bands = vec![EqBand::default(); 24];
    for (i, (freq, gain, q)) in [
        (80.0, -4.0, 0.9),
        (220.0, 2.5, 1.1),
        (900.0, 4.0, 1.4),
        (3200.0, -3.0, 2.0),
        (8000.0, 3.5, 0.8),
    ]
    .iter()
    .enumerate()
    {
        bands[i].used = true;
        bands[i].enabled = true;
        bands[i].index = i;
        bands[i].frequency = *freq;
        bands[i].gain = *gain;
        bands[i].q = *q;
    }
    bands
}

const DELAY_FAMILIES: [DelayFamily; 6] = [
    DelayFamily::Digital,
    DelayFamily::Tape,
    DelayFamily::Analog,
    DelayFamily::Pitch,
    DelayFamily::Rhythmic,
    DelayFamily::Special,
];

/// A labelled row of panels.
#[component]
fn Band(title: String, #[props(default = 1.0)] grow: f64, children: Element) -> Element {
    rsx! {
        div { style: "display:flex; flex-direction:column; flex:{grow} 1 0%; min-height:0;",
            div {
                style: "flex:0 0 auto; padding:3px 10px; color:#8a8a92; \
                        font-size:10px; letter-spacing:0.09em; text-transform:uppercase; \
                        border-bottom:1px solid #1b1b20;",
                "{title}"
            }
            div { style: "display:flex; flex:1 1 0%; min-height:0; gap:4px; padding:4px;",
                {children}
            }
        }
    }
}

/// One panel, with its name over it.
#[component]
fn Cell(label: String, children: Element) -> Element {
    rsx! {
        div { style: "position:relative; flex:1 1 0%; min-width:0; min-height:0; \
                      border:1px solid #1e1e24; overflow:hidden;",
            {children}
            div {
                style: "position:absolute; top:2px; left:5px; color:#c8c8d0; \
                        font-size:10px; letter-spacing:0.05em; pointer-events:none; \
                        text-transform:uppercase;",
                "{label}"
            }
        }
    }
}

/// A quarter-note tail, the same shape every delay family is shown making.
fn taps(beat_ms: f32) -> Vec<(f32, f32, bool)> {
    (1..=6)
        .map(|i| (beat_ms * i as f32, 0.82_f32.powi(i - 1), i % 2 == 1))
        .collect()
}

/// Linear peaks to the compressor's display range.
fn scaled(peaks: &[f32]) -> Vec<f32> {
    peaks
        .iter()
        .map(|&p| {
            if p <= 0.0 {
                0.0
            } else {
                (1.0 + 20.0 * p.log10() / 60.0).clamp(0.0, 1.0)
            }
        })
        .collect()
}

fn engine_name(e: Engine) -> String {
    match e {
        Engine::Chorus => "chorus",
        Engine::Flanger => "flanger",
        Engine::Phaser => "phaser",
        Engine::Tremolo => "tremolo",
        Engine::Vibrato => "vibrato",
        Engine::Rotary => "rotary",
    }
    .to_string()
}

fn delay_name(f: DelayFamily) -> String {
    match f {
        DelayFamily::Digital => "digital",
        DelayFamily::Tape => "tape",
        DelayFamily::Analog => "analog",
        DelayFamily::Pitch => "pitch",
        DelayFamily::Rhythmic => "rhythmic",
        DelayFamily::Special => "special",
    }
    .to_string()
}

fn verb_name(f: VerbFamily) -> String {
    match f {
        VerbFamily::Room => "room",
        VerbFamily::Hall => "hall",
        VerbFamily::Plate => "plate",
        VerbFamily::Spring => "spring",
        VerbFamily::Ambient => "ambient",
        VerbFamily::Random => "random",
        VerbFamily::Special => "special",
        VerbFamily::Convolution => "convolution",
    }
    .to_string()
}
