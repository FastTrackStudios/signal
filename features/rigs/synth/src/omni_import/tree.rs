//! **Patch → composition tree** — map an [`OmniPatch`] onto the Omnisphere
//! routing tree, realizing soundsources and emitting block params + routes.

use std::path::Path;

use signal_proto::block::BlockType;

use super::model::{
    FilterModel, OmniModRoute, OmniPatch, classify_effect, classify_filter_full, filter_model,
    omni_cutoff_hz,
};

/// Omnisphere's normalized cutoff → OUR normalized cutoff param, via the
/// calibrated Hz curve.
fn omni_cutoff_norm(v: f32) -> f32 {
    signal_sampler::native::NativeFilter::norm_from_cutoff(omni_cutoff_hz(v))
}
use super::{SoundsourceIndex, parse_patch};
use signal_sampler::rig::RigBlock;
use signal_sampler::rig_node::Container;

/// Configure a filter block as an Omnisphere filter model at a knob setting.
fn modelled_filter(
    block: RigBlock,
    type_n: Option<f32>,
    name: &str,
    setting: f32,
    res: f32,
    weight: f32,
) -> RigBlock {
    let model = type_n.and_then(filter_model).unwrap_or_else(|| {
        // No measured algorithm: the name's family, generically.
        let (mode, poles, character) = classify_filter_full(name);
        FilterModel {
            name: "by name",
            mode,
            poles,
            ladder: character == "ladder",
            taper: [1.0; 5],
            res: if character == "ladder" {
                (0.0, 3.8, 1.0)
            } else {
                (0.5, 12.0, 1.0)
            },
            res_shift: 1.0,
            res_shift_curve: 1.0,
            gain_db: 0.0,
            comp: 0.5,
        }
    });
    let hz = model.corner_hz(setting);
    // The knob itself, so modulation moves it the way Omnisphere's does.
    let taper = (0..=30)
        .map(|i| {
            let s = signal_sampler::native::NativeFilter::KNOB_MIN + 0.05 * i as f32;
            format!("{s:.2}:{:.2}", model.corner_hz(s))
        })
        .collect::<Vec<_>>()
        .join(";");
    block
        .with_param("taper", taper)
        .with_param("knob_setting", format!("{setting:.4}"))
        .with_param(
            "cutoff",
            format!(
                "{:.4}",
                signal_sampler::native::NativeFilter::norm_from_cutoff(hz)
            ),
        )
        .with_param("resonance", format!("{:.4}", res.clamp(0.0, 1.0)))
        .with_param("mode", model.mode)
        .with_param("poles", model.poles.to_string())
        .with_param("character", if model.ladder { "ladder" } else { "clean" })
        .with_param("res_lo", format!("{:.3}", model.res.0))
        .with_param("res_hi", format!("{:.3}", model.res.1))
        .with_param("res_curve", format!("{:.3}", model.res.2))
        .with_param("res_shift", format!("{:.3}", model.res_shift))
        .with_param("res_shift_curve", format!("{:.3}", model.res_shift_curve))
        .with_param(
            "gain_db",
            format!("{:.1}", model.gain_db + 20.0 * weight.max(1e-6).log10()),
        )
        .with_param("ladder_comp", format!("{:.2}", model.comp))
}

/// Breakpoints as the block param "time:level:curve:step;…".
fn points_param(points: &[(f32, f32, f32, bool)]) -> String {
    points
        .iter()
        .map(|(t, l, k, step)| format!("{t:.5}:{l:.4}:{k:.3}:{}", u8::from(*step)))
        .collect::<Vec<_>>()
        .join(";")
}

/// A layer's amp envelope points (seconds, or beats when synced — see
/// `amp_sync`).
fn amp_points_param(layer: &super::model::OmniLayer) -> String {
    points_param(&layer.amp_points)
}

// ── Patch → composition tree ─────────────────────────────────────────────────

pub const LAYER_NAMES: [&str; 4] = ["Layer A", "Layer B", "Layer C", "Layer D"];

fn fx_rack_from(name: &str, types: &[String], params: &[[f32; 15]]) -> Container {
    let mut rack = Container::module(name);
    for slot in 0..4 {
        let label = types
            .get(slot)
            .map(std::string::String::as_str)
            .filter(|s| !s.is_empty() && *s != "No Effect");
        let p = params.get(slot).copied().unwrap_or([0.0; 15]);
        // Realize to native DSP with the effect's own settings when we know
        // the unit (see `effects`); otherwise a pass-through placeholder.
        if let Some(c) = label.and_then(|fx| super::effects::effect_chain(fx, &p)) {
            // Several blocks for one slot: their own container, so the rack
            // still reads as four slots.
            rack = rack.add(c);
            continue;
        }
        rack = match label.and_then(|fx| super::effects::effect_block(fx, &p)) {
            Some(block) => rack.add(block),
            None => rack.block(BlockType::Custom, format!("{name} Slot {}", slot + 1)),
        };
    }
    rack
}

/// An Omnisphere mod source → the modulator or performance source our tree
/// provides.
fn translate_source(source: &str) -> Option<String> {
    // Sources: MIDI performance names map directly; Omnisphere modulator
    // names map onto the modulator blocks our tree attaches.
    Some(match source {
        "Wheel" => "Wheel".to_string(),
        // Omnisphere's velocity source is squared (measured).
        "Velo" => "Velocity Squared".to_string(),
        "After" => "Aftertouch".to_string(),
        "Bender" => "Bender".to_string(),
        "Key" => "Key".to_string(),
        "Alt" => "Alt".to_string(),
        "Constant" | "Bias1" | "Bias2" => "Constant".to_string(),
        "Random" | "Random2" | "Random Unipolar" => "Random".to_string(),
        "MPEv" => "MPEPressure".to_string(),
        "MPE3" => "MPETimbre".to_string(),
        s if s.starts_with("LFO") => format!("LFO {}", &s[3..]),
        s if s.ends_with("FENV") => "Filter Env".to_string(),
        s if s.starts_with("ModEnv") => format!("Mod Env {}", &s[6..]),
        _ => return None,
    })
}

/// Translate one Omnisphere mod-matrix route into our route model, when the
/// target is something the runtime drives today.
///
/// Returns `(layer_index, source, target, depth)` — `layer_index` scopes the
/// route to a layer (`A freq` targets Layer A's filter); part-wide routes use
/// the layer the target names.
pub fn translate_route(
    route: &OmniModRoute,
    filter_labels: &[String],
) -> Option<(usize, String, String, f32)> {
    // Targets: "<L> freq" / "<L> res" where <L> is A..D → the layer's Filter 1.
    let (layer_letter, param) = route.target.split_once(' ')?;
    // A modulator's own depth ("LFO1 swing"): the swing adds depth·source
    // (measured, linear). Part-level; carried on Layer A, where the part's
    // LFOs are in scope.
    if let (Some(n), "swing") = (layer_letter.strip_prefix("LFO"), param) {
        let source = translate_source(&route.source)?;
        return Some((0, source, format!("LFO {n}.amp"), route.depth));
    }
    let layer_idx = match layer_letter {
        "A" => 0,
        "B" => 1,
        "C" => 2,
        "D" => 3,
        _ => return None,
    };
    // Pitch targets ride the synth oscillator's tune param; freq/res ride
    // the layer's Filter 1.
    let (block, param, scale): (&str, &str, f32) = match param {
        // The filter's emulated Omnisphere knob: routes add setting units,
        // and the knob param spans 1.5 settings.
        "freq" => (
            filter_labels.get(layer_idx)?.as_str(),
            "knob",
            1.0 / signal_sampler::native::NativeFilter::KNOB_SPAN,
        ),
        "res" => (filter_labels.get(layer_idx)?.as_str(), "resonance", 1.0),
        // Measured: a tune row moves ~96 semitones per unit of depth; our
        // tune param spans 48.
        "tune" => ("Soundsource", "tune", 2.0),
        // tuneFine is ±1 semitone on a ±24 semitone param.
        "tuneFine" => ("Soundsource", "tune", 1.0 / 24.0),
        // Amp tremolo → the Amp's cut-only tremolo (measured: gain =
        // clamp(1 + depth·source, 0, 1); the param is 0.5-centred, ±0.5).
        "atrm" => ("Amp", "tremolo", 0.5),
        // Shape (measured: `pdepth` morphs the played wave toward `wf1`).
        "pdepth" => ("Soundsource", "wt_mix", 1.0),
        // Symmetry: on a classic layer, the crossfade between its captured
        // Symmetry steps (a knob unit spans four); see the classic remap.
        "pwidth" => ("Soundsource", "wt_mix_sym", 4.0),
        // Hard sync, in the knob's own units.
        "hrdsnc" => ("Soundsource", "sync_knob", 1.0),
        // Harmonia mix.
        "Harmmix" => ("Soundsource", "harm_mix", 1.0),
        // Pan: the Amp's balance, in the same knob units.
        "pan" => ("Amp", "pan", 1.0),
        // The filter envelope's depth: a modulated depth on the envelope
        // itself (the section route then runs at unit depth; see below).
        "envdpth" => ("Filter Env", "amp", 1.0),
        _ => return None, // hrdsnc/mogrify/timbre/LFO-param/E1P0/… — later
    };
    let source = translate_source(&route.source)?;
    Some((
        layer_idx,
        source,
        format!("{block}.{param}"),
        route.depth * scale,
    ))
}

/// Map a parsed patch onto the Omnisphere composition tree, realizing each
/// layer's Soundsource block against `index` (unmatched names stay
/// Omnisphere's layer level taper: `(level, dB relative to level 1.0)`,
/// measured through the real plugin (init part, layer A, note 48, the held
/// note's RMS). Read in between by linear interpolation in dB.
const LEVEL_TAPER: [(f32, f32); 10] = [
    (0.0, -120.0),
    (0.05, -56.5),
    (0.1, -44.5),
    (0.25, -28.7),
    (0.4, -20.4),
    (0.5, -16.6),
    (0.6, -13.5),
    (0.75, -9.55),
    (0.9, -4.6),
    (1.0, 0.0),
];

/// A synth-mode layer's calibration: at Omnisphere's default level (0.75)
/// its held note matches the real plugin's RMS (0.067 at note 48 on the init
/// part's Jupiter 8 Saw, played from its real wavetable; ours read 0.0380
/// before the taper). Re-measured once the amp envelope's attack was read
/// from its true peak (it had been reading the sustain point as the peak).
// +6 dB since the Aux rack stopped summing a second dry copy in.
const SYNTH_LAYER_CAL_DB: f32 = 19.25;

/// A synth layer with no playable wavetable (the older classic oscillator,
/// or waves missing locally) plays the generated saw, which runs hotter than
/// a real wavetable: measured −7.85 dB against Omnisphere's classic saw.
const GENERATED_SAW_CAL_DB: f32 = -7.85;

/// Whether a layer's named waves resolve to files we can play.
fn waves_resolve(layer: &super::OmniLayer) -> bool {
    match &layer.waves {
        Some((a, b)) => super::wavetable_path(a).is_some() && super::wavetable_path(b).is_some(),
        // The classic oscillator plays captured tables (or its noise), in
        // the same units as the named ones.
        None => super::model::classic_osc(layer.osc_wave, layer.osc_shape, layer.osc_symmetry).is_some(),
    }
}

/// The dB a layer at `level` (0..1) plays at, relative to level 1.0.
fn layer_level_db(level: f32) -> f32 {
    let l = level.clamp(0.0, 1.0);
    for w in LEVEL_TAPER.windows(2) {
        let ((l0, d0), (l1, d1)) = (w[0], w[1]);
        if l <= l1 {
            return d0 + (d1 - d0) * (l - l0) / (l1 - l0);
        }
    }
    0.0
}

/// placeholders — the structure still routes).
pub fn patch_to_container(patch: &OmniPatch, index: &SoundsourceIndex) -> Container {
    // Filter block labels per layer (route targets reference them by name).
    // Every layer's first filter is "Filter 1" and its source "Soundsource",
    // the names a keys module uses, so the rig's live controls (cutoff, the
    // envelopes) reach an imported layer the same way; Omnisphere's own names
    // ride along as the blocks' `model` / `soundsource` params.
    let filter_labels: Vec<String> = patch
        .layers
        .iter()
        .take(4)
        .map(|_| "Filter 1".to_string())
        .collect();
    // Live routes bucketed per layer; the rest stay inspectable params.
    let mut layer_routes: Vec<Vec<(String, String, f32)>> = vec![Vec::new(); 4];
    for route in &patch.mod_routes {
        if let Some((idx, source, target, depth)) = translate_route(route, &filter_labels) {
            layer_routes[idx].push((source, target.clone(), depth));
            // `lo` shifts the target whatever the source does (same units).
            if route.offset != 0.0 && route.depth != 0.0 {
                let offset = route.offset * depth / route.depth;
                layer_routes[idx].push(("Constant".to_string(), target, offset));
            }
        }
    }

    // A classic-oscillator layer's Shape scans its table's frames (its
    // Symmetry is the table crossfade), so Shape routes move the scan.
    for (i, layer) in patch.layers.iter().take(4).enumerate() {
        let classic = layer.soundsource.is_empty()
            && layer.waves.is_none()
            && matches!(
                super::model::classic_osc(layer.osc_wave, layer.osc_shape, layer.osc_symmetry),
                Some(super::model::ClassicOsc::Tables { .. })
            );
        for (_, target, _) in &mut layer_routes[i] {
            if classic && target == "Soundsource.wt_mix" {
                *target = "Soundsource.wt_position".to_string();
            } else if target == "Soundsource.wt_mix_sym" {
                // Symmetry is the classic tables' crossfade; a named
                // wavetable has no Symmetry we model.
                *target = if classic { "Soundsource.wt_mix" } else { "" }.to_string();
            }
        }
        layer_routes[i].retain(|(_, t, _)| !t.is_empty());
    }

    let mut quadzone = Container::parallel("Quadzone").param("mode", "Fader");
    for (i, layer) in patch.layers.iter().take(4).enumerate() {
        let name = LAYER_NAMES[i];

        let mut osc = Container::module("Oscillator");
        osc = if layer.soundsource.is_empty() {
            // Synth mode: the wavetable voice carries the whole oscillator
            // stack (unison / harmonia / FM / ring) as build params.
            let mut wt = RigBlock::of_type(BlockType::Wavetable)
                .named("Soundsource")
                .with_param("transpose", format!("{:.4}", layer.transpose))
                .with_param("amp_points", amp_points_param(layer))
                .with_param("amp_sync", if layer.amp_env_synced { "1" } else { "0" })
                .with_param("vel_sens", format!("{:.3}", layer.amp_velsens))
                .with_param(
                    "glide_s",
                    format!("{:.4}", if layer.glide { patch.glide_s } else { 0.0 }),
                )
                // With no named waves (older patches' classic oscillator)
                // Omnisphere plays a saw whatever `OSC type` says (measured:
                // every type reads as a saw, differing only in its top
                // roll-off), so the generated shape is a saw.
                .with_param("shape", format!("{:.4}", 2.0 / 3.0));
            // The oscillator plays the patch's own waves, read from the
            // extracted library (measured against Omnisphere: `wf0` is the
            // waveform heard whenever it is named; frame 0 matches it to
            // 0.3 dB at mid pitch), with Shape morphing into `wf1`. Missing
            // files leave the generated shape.
            {
                if layer.waves.is_none() {
                    // The classic oscillator, captured from the plugin.
                    match super::model::classic_osc(layer.osc_wave, layer.osc_shape, layer.osc_symmetry) {
                        Some(super::model::ClassicOsc::Tables { lo, hi, mix, shape }) => {
                            wt = wt
                                .with_param("wave0", lo.to_string_lossy().to_string())
                                .with_param("wave1", hi.to_string_lossy().to_string())
                                .with_param("wt_stride", "1")
                                .with_param("wt_position", format!("{shape:.4}"))
                                .with_param("wt_mix", format!("{mix:.4}"))
                                .with_param("classic", "1");
                        }
                        Some(super::model::ClassicOsc::Noise { shape, symmetry }) => {
                            wt = wt
                                .with_param("noise", "1")
                                .with_param("noise_shape", format!("{shape:.4}"))
                                .with_param("noise_symmetry", format!("{symmetry:.4}"));
                        }
                        None => {}
                    }
                }
                if let Some((a, b)) = &layer.waves {
                    if let (Some(pa), Some(pb)) =
                        (super::wavetable_path(a), super::wavetable_path(b))
                    {
                        wt = wt
                            .with_param("wave0", pa.to_string_lossy().to_string())
                            .with_param("wave1", pb.to_string_lossy().to_string())
                            .with_param("wt_stride", "1")
                            .with_param("wt_position", "0")
                            .with_param("wt_mix", format!("{:.4}", layer.osc_shape));
                    } else {
                        tracing::warn!(wave = %a, "omni import: wavetable not in the local extraction — generated shape");
                    }
                }
            }
            if layer.unison_count > 1 {
                wt = wt
                    .with_param("unison_voices", layer.unison_count.to_string())
                    // Calibrated: udpth → ~185 cents total spread (measured
                    // 189/184/182 across a 3-point sweep). Our param is
                    // cents/100, so scale by 1.85.
                    .with_param(
                        "unison_detune",
                        format!("{:.4}", layer.unison_detune * 1.85),
                    )
                    .with_param("unison_width", format!("{:.4}", layer.unison_width));
                if layer.unison_octave > 0.0 {
                    wt = wt.with_param("unison_octave", format!("{:.4}", layer.unison_octave));
                }
                if layer.unison_analog > 0.0 {
                    wt = wt.with_param("unison_analog", format!("{:.4}", layer.unison_analog));
                }
                if layer.unison_drift > 0.0 {
                    wt = wt.with_param("unison_drift", format!("{:.4}", layer.unison_drift));
                }
            }
            if let Some((a, d, s, r)) = layer.amp_env {
                wt = wt
                    .with_param("amp_attack", format!("{a:.4}"))
                    .with_param("amp_decay", format!("{d:.4}"))
                    .with_param("amp_sustain", format!("{s:.4}"))
                    .with_param("amp_release", format!("{r:.4}"));
            }
            if layer.fm_depth > 0.0 {
                wt = wt
                    .with_param("fm_depth", format!("{:.4}", layer.fm_depth))
                    .with_param("fm_ratio", format!("{:.4}", layer.fm_ratio.max(0.01)))
                    .with_param("fm_shape", format!("{:.4}", layer.fm_shape));
            }
            wt = wt.with_param("sync_knob", format!("{:.4}", layer.sync_knob));
            if layer.sync_ratio > 1.0 {
                wt = wt.with_param("sync_ratio", format!("{:.4}", layer.sync_ratio));
            }
            if layer.ring_mix > 0.0 {
                wt = wt
                    .with_param("ring_mix", format!("{:.4}", layer.ring_mix))
                    .with_param("ring_ratio", format!("{:.4}", layer.ring_ratio.max(0.01)));
            }
            for (i, (level, smi, pan, shape)) in layer.harmonia.iter().take(4).enumerate() {
                let n = i + 1;
                wt = wt
                    .with_param(format!("harm{n}_level"), format!("{level:.4}"))
                    .with_param(format!("harm{n}_interval"), format!("{smi:.1}"))
                    .with_param(format!("harm{n}_pan"), format!("{pan:.4}"))
                    .with_param(format!("harm{n}_shape"), format!("{shape:.4}"));
            }
            osc.add(wt)
        } else if let Some(spec) = index.find_source(&layer.soundsource) {
            // Sample mode: unison + the amp ADSR ride the Sampler block
            // (the engine applies them per voice at trigger time).
            let mut sb = RigBlock::sample_lib(spec.to_string_lossy().to_string())
                .named("Soundsource")
                .with_param("soundsource", layer.soundsource.clone());
            // The layer's fixed transposition: whole semitones + cents.
            if layer.transpose != 0.0 {
                let st = layer.transpose.trunc();
                sb = sb
                    .with_param("transpose", format!("{st}"))
                    .with_param("fine", format!("{:.2}", (layer.transpose - st) * 100.0));
            }
            if layer.unison_count > 1 {
                sb = sb
                    .with_param("unison_voices", layer.unison_count.to_string())
                    // Calibrated: udpth → ~185 cents total spread (measured
                    // 189/184/182 across a 3-point sweep). Our param is
                    // cents/100, so scale by 1.85.
                    .with_param(
                        "unison_detune",
                        format!("{:.4}", layer.unison_detune * 1.85),
                    )
                    .with_param("unison_width", format!("{:.4}", layer.unison_width));
            }
            // The amp envelope as the plugin plays it: the AENV breakpoints
            // (the ADSR params are ignored by Omnisphere), and its velocity
            // law on top of the velocity layer the sample picks.
            if layer.amp_points.len() >= 2 {
                sb = sb
                    .with_param("amp_points", amp_points_param(layer))
                .with_param("amp_sync", if layer.amp_env_synced { "1" } else { "0" })
                    .with_param("amp_attack", "0");
            } else if let Some((a, d, s, r)) = layer.amp_env {
                sb = sb
                    .with_param("amp_attack", format!("{a:.4}"))
                    .with_param("amp_decay", format!("{d:.4}"))
                    .with_param("amp_sustain", format!("{s:.4}"))
                    .with_param("amp_release", format!("{r:.4}"));
            }
            sb = sb.with_param("vel_sens", format!("{:.3}", layer.amp_velsens));
            // Harmonia voices play the soundsource itself at their intervals
            // (the engine stacks them per note).
            for (i, (level, smi, pan, _shape)) in layer.harmonia.iter().take(4).enumerate() {
                let n = i + 1;
                sb = sb
                    .with_param(format!("harm{n}_level"), format!("{level:.4}"))
                    .with_param(format!("harm{n}_interval"), format!("{smi:.1}"))
                    .with_param(format!("harm{n}_pan"), format!("{pan:.4}"));
            }
            // One soundsource of a multi-soundsource pack (Keyscape).
            if let Some(a) = index.articulation_for(&layer.soundsource) {
                sb = sb.with_param("articulation", a.to_string());
            }
            if let Some(v) = super::index::dynamic_zone_velocity(&layer.soundsource) {
                sb = sb.with_param("zone_velocity", v.to_string());
            }
            let timbre = layer.timbre_semis();
            if timbre != 0 {
                sb = sb.with_param("timbre_semis", timbre.to_string());
            }
            osc.add(sb)
        } else {
            tracing::warn!(
                soundsource = %layer.soundsource,
                library = %layer.ss_library,
                "omni import: soundsource not in the local extraction — placeholder"
            );
            osc.block(BlockType::Sampler, &layer.soundsource)
        };
        // The oscillator sub-modules chain in SERIES after the source. The LIVE
        // native ones (Harmonia → modal, Dual Freq Shifter, Waveshaper) are
        // sound-generating/processing, so emit them ONLY when the patch engages
        // them: an always-on modal Harmonia here GENERATES its own tone and
        // masks the soundsource — every sample-mode patch otherwise collapses
        // to the same modal-piano voice. Unison/FM/Ring/Granular have no native
        // DSP yet, so they stay inert structural placeholders (pass-through).
        let mut osc = osc.block(BlockType::Unison, "Unison");
        // Harmonia only in sample mode when active; in synth mode the wavetable
        // already carries the harmonia voices as its own params.
        // (A sample layer's Harmonia rides the Sampler block — see above; the
        // modal Harmonic block generated its own tone and masked the source.)
        osc = osc
            .block(BlockType::FmOperator, "FM")
            .block(BlockType::RingModulator, "Ring Mod");
        if let Some((hz_a, mix_a, hz_b, mix_b, parallel)) = layer.dfs {
            osc = osc.add(
                RigBlock::of_type(BlockType::Dfs)
                    .named("Dual Freq Shifter")
                    .with_param("shift_a_hz", format!("{hz_a:.2}"))
                    .with_param("mix_a", format!("{mix_a:.4}"))
                    .with_param("shift_b_hz", format!("{hz_b:.2}"))
                    .with_param("mix_b", format!("{mix_b:.4}"))
                    .with_param("parallel", if parallel { "1" } else { "0" }),
            );
        }
        if let Some((drive, crush, reduce, mix)) = layer.shaper {
            osc = osc.add(
                RigBlock::of_type(BlockType::Waveshaper)
                    .named("Waveshaper")
                    .with_param("drive", format!("{drive:.4}"))
                    .with_param("crush", format!("{crush:.4}"))
                    .with_param("reduce", format!("{reduce:.4}"))
                    .with_param("mix", format!("{mix:.4}")),
            );
        }
        let osc = osc.block(BlockType::Granular, "Granular");

        let filter_label = filter_labels[i].clone();
        let mut built = Container::layer(name)
            .param("level", format!("{:.3}", layer.level))
            // The layer's volume: Omnisphere's level taper (measured), plus
            // the synth voice's calibration; a layer switched off is silent.
            .volume(if layer.enabled {
                layer_level_db(layer.level)
                    // The part's own gain (measured taper) and headroom.
                    + patch.gain_db
                    + patch.headroom_db
                    // The filter section's pre/post gain (measured).
                    + layer.filter_gain_db
                    + if layer.soundsource.is_empty() {
                        SYNTH_LAYER_CAL_DB
                            + if waves_resolve(layer) {
                                0.0
                            } else {
                                GENERATED_SAW_CAL_DB
                            }
                    } else {
                        0.0
                    }
            } else {
                -200.0
            })
            .param(
                "filter_routing",
                if layer.filter_parallel {
                    "Parallel"
                } else {
                    "Series"
                },
            )
            .param("filter_freq", format!("{:.3}", layer.filter_freq))
            .param("filter_res", format!("{:.3}", layer.filter_res))
            .add(osc)
            .add({
                // Each engaged filter is its measured model (by `type1` /
                // `type2`; the factory name only as a fallback), at the
                // corner the knob taper gives its effective setting.
                let mut f1 = RigBlock::of_type(BlockType::Filter)
                    .named(filter_label.clone())
                    .with_param("model", layer.filter_name.clone());
                // In parallel the balance weighs the two (measured law).
                let b = layer.filter_balance;
                let parallel = layer.filter_active && layer.filter_parallel;
                let (w1, w2) = if parallel {
                    (1.0 - b * b, 1.0 - (1.0 - b) * (1.0 - b))
                } else {
                    (1.0, 1.0)
                };
                if layer.filter_active && layer.filter1_on {
                    f1 = modelled_filter(
                        f1,
                        layer.filter_type1,
                        &layer.filter_name,
                        layer.filter_freq,
                        layer.filter_res,
                        w1,
                    );
                } else if parallel && layer.filter2.is_some() {
                    // A parallel pair with filter 1 off: its branch is silent.
                    f1 = f1.with_param("gain_db", "-120.0");
                }
                let mut f2 = RigBlock::of_type(BlockType::Filter).named("Filter 2");
                if let Some((setting, res)) = layer.filter2 {
                    if layer.filter_active {
                        f2 = modelled_filter(f2, layer.filter_type2, "", setting, res, w2);
                    }
                } else if parallel {
                    // Nothing in the second branch: it must not pass dry audio.
                    f2 = f2.with_param("gain_db", "-120.0");
                }
                // SERIES chains the filters; PARALLEL sums them.
                let filters = if parallel {
                    Container::parallel("Filters")
                } else {
                    Container::module("Filters")
                };
                filters.add(f1).add(f2)
            })
            // "Amp Stage": a route onto "Amp" (the patch's amp tremolo) must
            // find the block, not a container of the same name.
            .add(
                Container::module("Amp Stage").add(
                    RigBlock::of_type(BlockType::Amp)
                        .named("Amp")
                        .with_param("pan", format!("{:.4}", layer.pan))
                        // The tremolo's rest point is the layer's base
                        // amplitude (multiplier = 2·tremolo, clamped 0..1).
                        .with_param("tremolo", format!("{:.4}", (1.0 - layer.amp_cut) / 2.0)),
                ),
            )
            .add(fx_rack_from("Layer FX", &layer.fx, &layer.fx_params))
            .modulator(BlockType::Envelope, "Amp Env")
            .modulator_block({
                // The filter envelope: its breakpoints when the patch has
                // them (sustaining at the penultimate point, like the amp
                // envelope), else the imported ADSR.
                let env_amp = if layer_routes[i]
                    .iter()
                    .any(|(_, target, _)| target == "Filter Env.amp")
                {
                    layer.filter_env_depth.abs()
                } else {
                    1.0
                };
                if layer.filter_points.len() >= 2 {
                    RigBlock::of_type(BlockType::MultisegEnvelope)
                        .named("Filter Env")
                        .with_param("amp", format!("{env_amp:.4}"))
                        .with_param("points", points_param(&layer.filter_points))
                        .with_param("sync", if layer.filter_env_synced { "1" } else { "0" })
                        .with_param("sustain", (layer.filter_points.len() - 2).to_string())
                        .with_param("vel_sens", format!("{:.3}", layer.filter_env_velsens))
                } else {
                    let mut fe = RigBlock::of_type(BlockType::Envelope)
                        .named("Filter Env")
                        .with_param("amp", format!("{env_amp:.4}"))
                        .with_param("vel_sens", format!("{:.3}", layer.filter_env_velsens));
                    if let Some((a, d, s, r)) = layer.filter_env {
                        fe = fe
                            .with_param("attack", format!("{a:.4}"))
                            .with_param("decay", format!("{d:.4}"))
                            .with_param("sustain", format!("{s:.4}"))
                            .with_param("release", format!("{r:.4}"));
                    }
                    fe
                }
            });
        // The filter section's own envelope depth (independent of matrix rows).
        // Measured: it moves the cutoff knob `envdpth` settings at full
        // envelope, like a matrix row. When matrix rows modulate the depth
        // (`envdpth` targets), the envelope's own amp carries the depth and
        // the route runs at unit depth, so the rows add onto it.
        let depth_modulated = layer_routes[i]
            .iter()
            .any(|(_, target, _)| target == "Filter Env.amp");
        if layer.filter_active && (layer.filter_env_depth != 0.0 || depth_modulated) {
            let span = signal_sampler::native::NativeFilter::KNOB_SPAN;
            let depth = if depth_modulated {
                layer.filter_env_depth.signum() / span
            } else {
                layer.filter_env_depth / span
            };
            built = built.route("Filter Env", format!("{}.knob", filter_labels[i]), depth);
        }
        for (source, target, depth) in layer_routes[i].drain(..) {
            built = built.route(source, target, depth);
        }
        quadzone = quadzone.add(built);
    }

    let title = if patch.name.is_empty() {
        "Omnisphere Patch".to_string()
    } else {
        patch.name.clone()
    };
    let mut preset = Container::preset(title)
        .add(quadzone)
        .add(fx_rack_from(
            "Common FX",
            &patch.common_fx,
            &patch.common_fx_params,
        ))
        // The Aux rack returns in parallel with the main path, fed by the
        // part's aux send (measured: `send × −3 dB`, linear; the dry path
        // is untouched). A layer send into it summed a second dry copy in.
        .add(
            Container::parallel("Aux Split")
                .add(Container::module("Main"))
                .add(
                    Container::module("Aux Return")
                        .volume(if patch.aux_send > 0.0 {
                            20.0 * patch.aux_send.log10() - 3.0
                        } else {
                            -200.0
                        })
                        .add(fx_rack_from(
                            "Aux Rack",
                            &patch.aux_fx,
                            &patch.aux_fx_params,
                        )),
                ),
        )
        .modulator(BlockType::ModMatrix, "Mod Matrix");
    for n in 1..=9usize {
        let mut lfo = RigBlock::of_type(BlockType::Lfo).named(format!("LFO {n}"));
        if let Some(l) = patch.lfos.get(n - 1) {
            // Measured rate, wave, swing (amplitude) and polarity.
            lfo = lfo
                .with_param("rate", format!("{:.4}", l.rate_hz()))
                .with_param("wave", l.wave().to_string())
                .with_param("amp", format!("{:.4}", l.swing))
                .with_param("unipolar", if l.unipolar { "1" } else { "0" });
            if l.sync {
                // Tempo-synced: the rate picks a note division (measured).
                lfo = lfo.with_param("sync_beats", format!("{:.5}", l.sync_beats()));
            }
            if l.retrigger {
                lfo = lfo.with_param("retrigger", "1");
            }
        }
        preset = preset.modulator_block(lfo);
    }
    for (n, env) in patch.mod_envs.iter().enumerate().take(6) {
        let points = points_param(&env.points);
        preset = preset.modulator_block(
            RigBlock::of_type(BlockType::MultisegEnvelope)
                .named(format!("Mod Env {}", n + 1))
                .with_param("points", points)
                .with_param("loop", if env.looping { "1" } else { "0" })
                // Measured: a Mod Env's level ignores velocity.
                .with_param("sync", if env.synced { "1" } else { "0" }),
        );
    }
    if patch.arp_on {
        let mut arp = RigBlock::of_type(BlockType::Arpeggiator)
            .named("Arp")
            .with_param("on", "1")
            .with_param(
                "step_beats",
                format!("{:.5}", patch.arp_step_beats.max(0.03125)),
            )
            .with_param("steps", patch.arp_steps.len().to_string());
        for (i, (on, vel, gate)) in patch.arp_steps.iter().enumerate() {
            arp = arp
                .with_param(format!("step{i}_on"), if *on { "1" } else { "0" })
                .with_param(format!("step{i}_vel"), vel.to_string())
                .with_param(format!("step{i}_gate"), format!("{gate:.3}"));
        }
        preset = preset.modulator_block(arp);
    }
    // Carry the browser tags + mod routes as preset params (inspectable in
    // dumps and the TUI; the mod routes become live once the ModMatrix
    // runtime lands).
    for (k, v) in &patch.tags {
        preset = preset.param(format!("tag:{k}"), v.clone());
    }
    for (i, route) in patch.mod_routes.iter().enumerate() {
        preset = preset.param(
            format!("mod{i}"),
            format!("{} -> {} @ {:.3}", route.source, route.target, route.depth),
        );
    }
    preset
}

/// Convenience: read + parse + map a `.prt_omn` patch or `.mlt_omn` Multi.
///
/// # Errors
///
/// Returns an error if the file cannot be read or if the XML cannot be parsed
/// as a valid Omnisphere patch or multi.
pub fn load_patch_file(path: &Path, index: &SoundsourceIndex) -> Result<Container, String> {
    if path.extension().is_some_and(|e| e == "mlt_omn") {
        return super::multi::load_multi_file(path, index);
    }
    let xml = std::fs::read_to_string(path).map_err(|e| format!("read {path:?}: {e}"))?;
    let patch = parse_patch(&xml)?;
    Ok(patch_to_container(&patch, index))
}
