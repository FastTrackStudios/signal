//! Guitar rig demo — the **real** `signal-guitar-ui` control surface.
//!
//! This is not a picture of the interface. It is [`ControlView`], the same
//! component `apps/desktop`'s browser remote mounts (see that crate's
//! `mobile_view.rs`), rendering the same EQ surface, drive board and
//! meters it renders on stage.
//!
//! # How it runs with no engine
//!
//! `ControlView` reads its rig through `try_consume_context::<RigClient>`,
//! and the whole guitar UI is built that way — no client in context means
//! no writes go anywhere and the surface is read-only. So the demo does
//! not stub the component; it supplies the *state* the component would
//! have been given by a live rig, and leaves the client out.
//!
//! That state is [`RigViewState`], a plain struct of signals. Seeded with
//! a patch built here, the view renders exactly as it does when the rig
//! filled those signals over the wire. Nothing about the rendering path is
//! special-cased for the demo, which is the point: if this looks wrong,
//! the shipped UI is wrong.
//!
//! # The feed
//!
//! A live rig fills those signals from its event stream ~15-30 times a
//! second. [`play_feed`] does the same thing from a closed-form model of
//! someone playing: pick attacks that decay, a compressor that pulls back
//! on the transients, and a spectrum that moves with the note rather than
//! wobbling at random. Nothing is sampled and nothing is random — it is a
//! function of elapsed frames, so it is identical on every machine and in
//! every screenshot.
//!
//! The blocks below are a real chain — a boost, three drives, two amps, a
//! compressor and a 24-band EQ — with the parameter names the wire
//! protocol actually uses (`b{i}_{used,on,freq,gain,q,shape}` for the EQ,
//! decoded by `eq_surface::bands_of`). Get one of those names wrong and
//! the EQ renders flat, which is a useful canary.

use std::time::Duration;

use dioxus::prelude::*;
use signal_guitar_ui::proto::{BlockParam, LiveBlock, PerfStack, PerformanceModel};
use signal_guitar_ui::{ControlView, PerformGrid, RigViewState};
use signal_proto::block::BlockType;

/// A band in the demo EQ curve: (frequency, gain dB, Q, shape index).
///
/// Shape indices follow `eq-ui`'s `shape_from_index`: 0 is a bell, and the
/// low/high shelves and cuts sit above it. A guitar amp EQ that is all
/// bells looks synthetic, so the curve opens with a high-pass and closes
/// with a shelf, the way a real one is dialled.
const EQ_BANDS: &[(f32, f32, f32, usize)] = &[
    (82.0, 0.0, 0.71, 3),    // high-pass — get the flub out
    (240.0, -3.4, 1.10, 0),  // scoop the mud
    (520.0, 2.1, 0.90, 0),   // body back in
    (1150.0, -2.8, 1.60, 0), // the honk
    (3200.0, 4.2, 0.80, 0),  // presence / pick attack
    (6400.0, 1.6, 0.70, 1),  // air, as a shelf
];

/// The drive board and amps, in signal order.
/// (id, type, name, preset, bypassed, primary param, value, min, max)
type BoardEntry = (
    &'static str,
    BlockType,
    &'static str,
    &'static str,
    bool,
    &'static str,
    f32,
    f32,
    f32,
);

const BOARD: &[BoardEntry] = &[
    // The two drive slots the profile actually defines (DriveSlotDef):
    // "Drive 1" = King of Tone, "Drive 2" = JHS Morning Glory.
    (
        "boost",
        BlockType::Boost,
        "Boost",
        "Clean Boost",
        false,
        "gain",
        4.5,
        0.0,
        10.0,
    ),
    (
        "drive1",
        BlockType::Drive,
        "Drive 1",
        "King of Tone",
        false,
        "drive",
        6.2,
        0.0,
        10.0,
    ),
    (
        "drive2",
        BlockType::Drive,
        "Drive 2",
        "JHS Morning Glory",
        true,
        "drive",
        3.8,
        0.0,
        10.0,
    ),
    // The amp is the profile's NAM preset pool. "Crunch Edge" — the active
    // patch below — points at "AA Crunch".
    (
        "amp",
        BlockType::Amp,
        "Amp",
        "AA Crunch",
        false,
        "master",
        6.0,
        0.0,
        10.0,
    ),
    // The time and modulation slots, named as the profile names them: its
    // overrides address "DLY 1", "DLY 2", "VERB 1" and "VERB 2" directly.
    // ControlView picks these by TYPE, not by name — Mod takes
    // Chorus/Phaser/Flanger, Motion takes Trem/Vibrato/Rotary, and the Delay
    // and Reverb panels filter on their own types. Leave a type out and that
    // panel renders as an empty slot, which is what the first version did.
    (
        "mod1",
        BlockType::Chorus,
        "MOD 1",
        "Chorus",
        true,
        "depth",
        3.4,
        0.0,
        10.0,
    ),
    (
        "trem1",
        BlockType::Trem,
        "TREM 1",
        "Tremolo",
        true,
        "depth",
        5.5,
        0.0,
        10.0,
    ),
    // "Crunch Edge" sets DLY 1 to a dotted-8th tape delay, mix 0.45,
    // feedback 0.4 — the values in the profile's own override list.
    (
        "dly1",
        BlockType::Delay,
        "DLY 1",
        "Tape",
        false,
        "mix",
        4.5,
        0.0,
        10.0,
    ),
    (
        "dly2",
        BlockType::Delay,
        "DLY 2",
        "Digital",
        true,
        "mix",
        2.4,
        0.0,
        10.0,
    ),
    (
        "verb1",
        BlockType::Reverb,
        "VERB 1",
        "Hall",
        false,
        "mix",
        2.2,
        0.0,
        10.0,
    ),
    (
        "verb2",
        BlockType::Reverb,
        "VERB 2",
        "Plate",
        true,
        "mix",
        1.8,
        0.0,
        10.0,
    ),
];

/// The Worship profile's five footswitch stacks and their real patch
/// rotations, from `features/rigs/guitar/src/profiles.rs::worship_def`:
///
///   Clean   -> Clean, Clean Dry, Clean Verb
///   Crunch  -> Crunch, Crunch Edge
///   Drive   -> Drive, Drive Edge
///   Lead    -> Lead, Lead POG
///   Ambient -> Ambient, Ambient Swells, Ambient Delay Craze
///
/// (name, current patch, position, count, active)
const STACKS: &[(&str, &str, u32, u32, bool)] = &[
    ("Clean", "Clean", 0, 3, false),
    // Crunch Edge is the active patch — it is the one the profile's comment
    // describes as "gate off so every rattle rings; dotted-8th tape delay".
    ("Crunch", "Crunch Edge", 1, 2, true),
    ("Drive", "Drive", 0, 2, false),
    ("Lead", "Lead", 0, 2, false),
    ("Ambient", "Ambient", 0, 3, false),
];

/// Build the 24-band EQ block the way the rig sends it.
fn eq_block() -> LiveBlock {
    let mut params = Vec::with_capacity(EQ_BANDS.len().saturating_mul(6));
    for (i, (freq, gain, q, shape)) in EQ_BANDS.iter().copied().enumerate() {
        let b = i.saturating_add(1);
        // `used` marks the band as existing at all; `on` is its bypass.
        // A band that is used-but-off draws greyed rather than vanishing,
        // so both are set here.
        for (suffix, value) in [
            ("used", 1.0),
            ("on", 1.0),
            ("freq", freq),
            ("gain", gain),
            ("q", q),
            // A shape index is 0..8, so the conversion is exact and
            // needs no cast.
            ("shape", f32::from(u8::try_from(shape).unwrap_or(0))),
        ] {
            params.push(BlockParam {
                name: format!("b{b}_{suffix}"),
                value,
                min: 0.0,
                max: 20_000.0,
            });
        }
    }
    LiveBlock {
        id: "eq".into(),
        block_type: BlockType::Eq,
        // ControlView looks this block up by name — see `find_block`.
        name: "Amp EQ".into(),
        bypassed: false,
        param_name: None,
        param_value: 0.0,
        param_min: 0.0,
        param_max: 1.0,
        params,
        preset: "Amp EQ".into(),
        options: Vec::new(),
        option: 0,
    }
}

/// The demo patch: EQ, compressor, and the drive board.
fn demo_blocks() -> Vec<LiveBlock> {
    let mut blocks = vec![
        eq_block(),
        LiveBlock {
            id: "comp".into(),
            block_type: BlockType::Compressor,
            // Also looked up by name by ControlView.
            name: "Compressor".into(),
            bypassed: false,
            param_name: Some("ratio".into()),
            param_value: 4.0,
            param_min: 1.0,
            param_max: 20.0,
            params: vec![
                BlockParam {
                    name: "threshold".into(),
                    value: -18.0,
                    min: -60.0,
                    max: 0.0,
                },
                BlockParam {
                    name: "ratio".into(),
                    value: 4.0,
                    min: 1.0,
                    max: 20.0,
                },
                BlockParam {
                    name: "attack".into(),
                    value: 12.0,
                    min: 0.1,
                    max: 100.0,
                },
                BlockParam {
                    name: "release".into(),
                    value: 180.0,
                    min: 10.0,
                    max: 1000.0,
                },
            ],
            preset: "Studio Comp".into(),
            options: Vec::new(),
            option: 0,
        },
    ];
    blocks.extend(BOARD.iter().copied().map(
        |(id, block_type, name, preset, bypassed, param, value, min, max)| LiveBlock {
            id: id.into(),
            block_type,
            name: name.into(),
            bypassed,
            param_name: Some(param.into()),
            param_value: value,
            param_min: min,
            param_max: max,
            params: vec![BlockParam {
                name: param.into(),
                value,
                min,
                max,
            }],
            preset: preset.into(),
            options: Vec::new(),
            option: 0,
        },
    ));
    blocks
}

/// One frame of the played-guitar model, at `t` seconds.
///
/// Returns `(in_db, out_db, gain_reduction_db, spectrum_tilt)`.
///
/// The shape is a repeating pick pattern: eight notes to the bar at 96 BPM,
/// each an instant attack into an exponential decay, with alternate notes
/// dug in harder. Output sits above input because the amp and boost have
/// gain; the compressor pulls the loudest transients back, which is why
/// `gr` tracks the attack rather than the average.
fn play_frame(t: f32) -> (f32, f32, f32, f32) {
    const BPM: f32 = 96.0;
    let eighth = 30.0 / BPM; // seconds per eighth note
    let n = (t / eighth).floor();
    let phase = (t / eighth).fract();

    // Alternate notes harder, and every fourth harder still — a strum
    // pattern rather than a metronome.
    // `n` is a whole number of eighth notes, wrapped into one bar. Compared
    // as a float so nothing has to cross into an integer type at all.
    let idx = (n % 8.0).max(0.0);
    let accent = if idx < 0.5 || (3.5..4.5).contains(&idx) {
        1.0
    } else if (1.5..2.5).contains(&idx) || (5.5..6.5).contains(&idx) {
        0.72
    } else {
        0.5
    };

    // Instant attack, exponential decay. Never fully silent: strings ring.
    let env = accent * (-phase * 4.2).exp() + 0.06;

    // Levels a sound engineer would actually set: peaks around -9 dBFS on
    // the accents, well clear of clipping. Driving these to -4 made the
    // meters sit yellow, which reads as a rig on the edge rather than a
    // rig working.
    let in_db = 24.0f32.mul_add(env.min(1.0), -38.0);
    let out_db = 23.0f32.mul_add(env.min(1.0), -32.0);
    // The compressor works on the transient, so gain reduction follows the
    // attack and releases over the note.
    let gr = (env * 9.0 - 1.5).clamp(0.0, 8.0);
    (in_db, out_db, gr, env)
}

/// Drive the view-state the way a rig's event stream would.
///
/// 30 Hz: fast enough that meters look continuous, slow enough that a
/// landing page with three of these is not fighting the compositor. The
/// loop owns no state beyond a frame counter — see the module docs on why
/// it is deterministic.
fn play_feed(mut state: RigViewState) {
    spawn(async move {
        let mut frame: u32 = 0;
        loop {
            architect::platform::sleep(Duration::from_millis(33)).await;
            frame = frame.wrapping_add(1);
            // The model is periodic over 8 eighth notes, so the counter is
            // wrapped into one bar before conversion — it never grows large
            // enough to lose precision, and no cast is needed.
            let t = f32::from(u16::try_from(frame % 6_000).unwrap_or(0)) * 0.033;

            let (in_db, out_db, gr, env) = play_frame(t);
            state.in_peak_db.set(in_db);
            state.out_peak_db.set(out_db);
            // A guitar into a stereo rig is not perfectly correlated; the
            // right side trails slightly, which is what a real meter shows.
            state
                .stereo_db
                .set((in_db, in_db - 0.9, out_db, out_db - 0.6));
            state.comp_gr_db.set(gr);
            // Perceptual meters are the sqrt-curved 0..1 pair.
            state
                .in_level
                .set(f64::from((env * 0.9).clamp(0.0, 1.0)).sqrt());
            state
                .out_level
                .set(f64::from((env * 1.05).clamp(0.0, 1.0)).sqrt());
            state.spectrum.set(spectrum_at(env, t));
        }
    });
}

/// A spectrum that looks like a guitar rather than noise: strong low-mids,
/// a presence bump, and the top rolled off the way a 4x12 does.
fn spectrum_at(env: f32, t: f32) -> Vec<f32> {
    const BINS: usize = 96;
    const BINS_F: f32 = 96.0;
    (0..BINS)
        .map(|i| {
            let f = f32::from(u8::try_from(i).unwrap_or(0)) / BINS_F;
            // Two humps (low-mid body, presence) minus a cabinet roll-off.
            let body = (-((f - 0.22) / 0.16).powi(2)).exp() * 26.0;
            let presence = (-((f - 0.62) / 0.12).powi(2)).exp() * 14.0;
            let rolloff = (f - 0.72).max(0.0) * 60.0;
            // The whole curve rises and falls with the note, and the
            // presence region shimmers a little as the pick attack decays —
            // the top moves faster than the body, as it does on a real one.
            let shimmer = f.mul_add(22.0, t * 9.0).sin() * 1.6 * env;
            (body + presence).mul_add(0.65f32.mul_add(env, 0.35), shimmer - rolloff) - 78.0
        })
        .collect()
}

/// The performance model behind the footswitch grid: the Worship profile's
/// stacks, its tempo, and a short setlist.
fn worship_performance() -> PerformanceModel {
    PerformanceModel {
        profile_name: "Worship".into(),
        stacks: STACKS
            .iter()
            .map(|(name, patch, position, count, active)| PerfStack {
                name: (*name).into(),
                current_patch: (*patch).into(),
                position: *position,
                patch_count: *count,
                // Every stack is preloaded on a rig that has been set up,
                // which is what the grid's "ready" state means.
                available: true,
                is_active: *active,
                preset: (*patch).into(),
                override_modules: Vec::new(),
            })
            .collect(),
        tempo_bpm: 96,
        // Perform mode 1 is Profile — the stacks view, which is the one the
        // footswitch grid is for.
        perform_mode: 1,
        ..PerformanceModel::default()
    }
}

#[component]
pub fn GuitarDemo() -> Element {
    // The signals a live rig would have filled. Built once — this is a
    // still frame of a rig, not a simulation of one.
    let state = use_hook(|| RigViewState {
        running: Signal::new(true),
        in_level: Signal::new(0.62),
        out_level: Signal::new(0.71),
        in_peak_db: Signal::new(-8.4),
        out_peak_db: Signal::new(-5.2),
        stereo_db: Signal::new((-8.4, -9.1, -5.2, -5.6)),
        comp_gr_db: Signal::new(3.6),
        spectrum: Signal::new(spectrum_at(0.5, 0.0)),
        comp_wave: Signal::new((Vec::new(), Vec::new())),
        perf: Signal::new(worship_performance()),
        blocks: Signal::new(demo_blocks()),
        active_patch: Signal::new(Some("Crunch Edge".to_string())),
    });

    // Feed it as a rig would. `use_hook` so the loop is started once per
    // mount rather than on every render.
    use_hook(|| play_feed(state));

    rsx! {
        // `inert` so the surface cannot be dialled from the landing page:
        // with no RigClient in context a drag would move a knob locally and
        // silently fail to reach anything, which reads as a broken control
        // rather than a demo. The real one is at /rigs/guitar.
        div { class: "sg-demo sg-demo-live sg-demo-guitar", inert: true, aria_hidden: "true",
            div { class: "sg-rig-stack",
                div { class: "sg-rig-control",
                    ControlView { model: state.perf.cloned(), state }
                }
                // The footswitch grid — the surface a player actually looks
                // at on stage, showing the Worship profile's five stacks.
                // Every callback is a no-op: the grid is `inert` and there is
                // no rig behind it, so a press has nothing to send to.
                div { class: "sg-rig-perform",
                    PerformGrid {
                        model: state.perf.cloned(),
                        on_press: move |_| {},
                        on_toggle_fx: move |()| {},
                        on_toggle_boost: move |()| {},
                        on_cycle_boost: move |()| {},
                        on_tap_tempo: move |()| {},
                        on_prev_song: move |()| {},
                        on_next_song: move |()| {},
                        on_select_song: move |_| {},
                    }
                }
            }
        }
    }
}
