//! Orchestra — the orchestral-strings *definition + test* layer over the shared
//! sampler engine.
//!
//! There is no separate orchestra DSP: the sampler engine
//! ([`signal_sampler::SamplerRig`]) already implements the CSS articulation
//! behaviour (legato / shorts / sustain, see `signal-sampler`'s `engine`
//! module). This crate owns the orchestra-specific *definitions* (how to load
//! and drive a strings library), its behaviour *spec* (`spec/`), and the A/B
//! *tests* that verify the engine reproduces a real Cinematic Studio Strings
//! render (`examples/`).

use std::path::Path;

use keyflow_orchestra::{Config, PartOutput, ProfileKind, process_part, score};
use signal_sampler::SamplerRig;
use signal_sampler::document::{
    DocCc, DocNote, DocumentRenderOptions, DocumentRenderResult, TempoPoint, TrackDocument,
};

/// Default install root for Cinematic Studio Strings (per-machine; override in
/// the caller). The A/B examples use this unless a path is passed.
pub const CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";

/// The articulation-config styx that maps CSS zones/articulations onto the
/// engine — the SOUNDPACK DEFINITION: every CSS-specific number (legato
/// timing curves, retire crossfades, makeup gains, master tune, amp
/// envelopes, keyswitch map) lives in this file, not in engine code. Owned
/// by this crate; resolved relative to the source tree at compile time.
// r[impl signal.soundsource.declarative]
pub const CSS_CONFIG: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/specs/cinematic-strings.styx"
);

/// Load a CSS strings section (e.g. `"1st Violins"`) into `rig` under `id`,
/// wired with the engine settings that match a real CSS-in-Kontakt render
/// (solo mic, arco-attack bloom, release overlap). This is the orchestra
/// feature's core *definition*: everything the engine needs to sound like CSS.
///
/// `css_root` is the library install dir; `config` is the articulation styx.
// r[impl orchestra.load.css-match]
// r[impl orchestra.load.section-zones]
pub fn load_strings(
    rig: &SamplerRig,
    id: &str,
    section: &str,
    mic: &str,
    css_root: impl AsRef<Path>,
    config: impl AsRef<Path>,
) -> Result<(), String> {
    let css_root = css_root.as_ref();
    let zones = css_root.join("_patches").join(section).join("library.styx");
    rig.load_instrument_with_config(id, config.as_ref(), &zones, css_root, section, mic)
        .map_err(|e| e.to_string())?;
    rig.set_solo_mic(id, Some(mic.to_string()));
    rig.set_midi_channel(id, 0);
    // Attack/release blooms come from the spec (`performance { attack_ms,
    // release_ms }` in the config styx) — nothing engine-side to set here.
    Ok(())
}

// ── Score → document bridge (keyflow-orchestra → signal-sampler) ─────────────

/// Convert one keyflow-orchestra engine output ([`PartOutput`], score-time —
/// run `process_part` with `timing_comp: false`; the document annotation does
/// the anticipation) into a sampler [`TrackDocument`].
///
/// keyflow channels are 1-based; documents are 0-based. keyflow has already
/// assigned divisi channels, so `auto_divisi` stays off and channel N maps to
/// engine line N. `seed` pins every stochastic choice (round-robin) — persist
/// it to reproduce a render byte-identically.
pub fn part_to_document(
    po: &PartOutput,
    tempos: &[score::TempoPoint],
    seed: u64,
) -> TrackDocument {
    TrackDocument {
        version: 1,
        seed,
        auto_divisi: false,
        notes: po
            .notes
            .iter()
            .map(|n| DocNote {
                start_qn: n.start_qn,
                end_qn: n.end_qn,
                chan: n.chan.saturating_sub(1),
                pitch: n.pitch.clamp(0, 127) as u8,
                vel: n.vel,
            })
            .collect(),
        ccs: po
            .ccs
            .iter()
            .map(|e| DocCc {
                qn: e.qn,
                chan: e.chan.saturating_sub(1),
                cc: e.cc,
                val: e.val,
            })
            .collect(),
        tempo: tempos
            .iter()
            .map(|t| TempoPoint {
                qn: t.qn,
                bpm: t.bpm,
            })
            .collect(),
    }
}

/// The full score → audio path for one part: MusicXML part → keyflow-orchestra
/// engine (score-time output) → [`TrackDocument`] → annotated schedule →
/// [`SamplerRig::render_offline_document`] on an instrument already loaded
/// under `id` (see [`load_strings`]). `rig` must be a
/// [`SamplerRig::new_offline`].
pub fn render_part_offline(
    rig: &SamplerRig,
    id: &str,
    part: &score::Part,
    tempos: &[score::TempoPoint],
    profile: ProfileKind,
    seed: u64,
) -> Result<DocumentRenderResult, String> {
    let cfg = Config {
        profile,
        timing_comp: false, // the document annotation does the anticipation
        tempo_map: Some(tempos.to_vec()),
        ..Config::default()
    };
    let po = process_part(part, &cfg);
    let doc = part_to_document(&po, tempos, seed);
    rig.render_offline_document(id, &doc, &DocumentRenderOptions::default())
        .map_err(|e| e.to_string())
}
