//! **Patch → composition tree** — map an [`OmniPatch`] onto the Omnisphere
//! routing tree, realizing soundsources and emitting block params + routes.

use std::path::Path;

use signal_proto::block::BlockType;

use super::model::{
    OmniModRoute, OmniPatch, classify_effect, classify_filter_full, classify_type1, omni_cutoff_hz,
};

/// Omnisphere's normalized cutoff → OUR normalized cutoff param, via the
/// calibrated Hz curve.
fn omni_cutoff_norm(v: f32) -> f32 {
    signal_sampler::native::NativeFilter::norm_from_cutoff(omni_cutoff_hz(v))
}
use super::{SoundsourceIndex, parse_patch};
use signal_sampler::rig::RigBlock;
use signal_sampler::rig_node::Container;

// ── Patch → composition tree ─────────────────────────────────────────────────

pub(crate) const LAYER_NAMES: [&str; 4] = ["Layer A", "Layer B", "Layer C", "Layer D"];

fn fx_rack_from(name: &str, types: &[String]) -> Container {
    let mut rack = Container::module(name);
    for slot in 0..4 {
        let label = types
            .get(slot)
            .map(|s| s.as_str())
            .filter(|s| !s.is_empty() && *s != "No Effect");
        rack = match label {
            // Realize to native DSP when we recognize the unit; otherwise keep
            // the name on a placeholder slot (renders as pass-through).
            Some(fx) => rack.block(classify_effect(fx).unwrap_or(BlockType::Custom), fx),
            None => rack.block(BlockType::Custom, format!("{name} Slot {}", slot + 1)),
        };
    }
    rack
}

/// Translate one Omnisphere mod-matrix route into our route model, when the
/// target is something the runtime drives today.
///
/// Returns `(layer_index, source, target, depth)` — `layer_index` scopes the
/// route to a layer (`A freq` targets Layer A's filter); part-wide routes use
/// the layer the target names.
pub(crate) fn translate_route(
    route: &OmniModRoute,
    filter_labels: &[String],
) -> Option<(usize, String, String, f32)> {
    // Targets: "<L> freq" / "<L> res" where <L> is A..D → the layer's Filter 1.
    let (layer_letter, param) = route.target.split_once(' ')?;
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
        "freq" => (filter_labels.get(layer_idx)?.as_str(), "cutoff", 1.0),
        "res" => (filter_labels.get(layer_idx)?.as_str(), "resonance", 1.0),
        "tune" => ("Synth Osc", "tune", 1.0),
        // tuneFine is ±1 semitone on a ±24 semitone param.
        "tuneFine" => ("Synth Osc", "tune", 1.0 / 24.0),
        // Osc amp tremolo → the layer's Amp gain.
        "atrm" => ("Amp", "gain", 1.0),
        // PWM depth → the square's pulse width (Symmetry axis).
        "pdepth" => ("Synth Osc", "symmetry", 1.0),
        // Harmonia mix.
        "Harmmix" => ("Synth Osc", "harm_mix", 1.0),
        _ => return None, // hrdsnc/mogrify/timbre/LFO-param/E1P0/… — later
    };
    // Sources: MIDI performance names map directly; Omnisphere modulator
    // names map onto the modulator blocks our tree attaches.
    let source = match route.source.as_str() {
        "Wheel" => "Wheel".to_string(),
        "Velo" => "Velocity".to_string(),
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
        s if s.starts_with("ModEnv") => "Mod Env".to_string(),
        _ => return None,
    };
    Some((
        layer_idx,
        source,
        format!("{block}.{param}"),
        route.depth * scale,
    ))
}

/// Map a parsed patch onto the Omnisphere composition tree, realizing each
/// layer's Soundsource block against `index` (unmatched names stay
/// placeholders — the structure still routes).
pub fn patch_to_container(patch: &OmniPatch, index: &SoundsourceIndex) -> Container {
    // Filter block labels per layer (route targets reference them by name).
    let filter_labels: Vec<String> = patch
        .layers
        .iter()
        .take(4)
        .map(|l| {
            if l.filter_name.is_empty() {
                "Filter 1".to_string()
            } else {
                l.filter_name.clone()
            }
        })
        .collect();
    // Live routes bucketed per layer; the rest stay inspectable params.
    let mut layer_routes: Vec<Vec<(String, String, f32)>> = vec![Vec::new(); 4];
    for route in &patch.mod_routes {
        if let Some((idx, source, target, depth)) = translate_route(route, &filter_labels) {
            layer_routes[idx].push((source, target, depth));
        }
    }

    let mut quadzone = Container::parallel("Quadzone").param("mode", "Fader");
    for (i, layer) in patch.layers.iter().take(4).enumerate() {
        let name = LAYER_NAMES[i];

        let mut osc = Container::module("Oscillator");
        osc = if layer.soundsource.is_empty() {
            // Synth mode: the wavetable voice carries the whole oscillator
            // stack (unison / harmonia / FM / ring) as build params.
            let mut wt = RigBlock::of_type(BlockType::Wavetable).named("Synth Osc");
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
                    .with_param("fm_shape", format!("{:.4}", layer.fm_shape));
            }
            if layer.ring_mix > 0.0 {
                wt = wt.with_param("ring_mix", format!("{:.4}", layer.ring_mix));
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
        } else {
            match index.find(&layer.soundsource) {
                Some(spec) => {
                    // Sample mode: unison + amp attack/release ride the
                    // Sampler block (the engine handles them at trigger time;
                    // decay/sustain need a full per-voice ADSR — pending).
                    let mut sb = RigBlock::sample_lib(spec.to_string_lossy().to_string())
                        .named(&layer.soundsource);
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
                    if let Some((a, _d, _s, r)) = layer.amp_env {
                        sb = sb
                            .with_param("amp_attack", format!("{a:.4}"))
                            .with_param("amp_release", format!("{r:.4}"));
                    }
                    osc.add(sb)
                }
                None => {
                    tracing::warn!(
                        soundsource = %layer.soundsource,
                        library = %layer.ss_library,
                        "omni import: soundsource not in the local extraction — placeholder"
                    );
                    osc.block(BlockType::Sampler, &layer.soundsource)
                }
            }
        };
        let mut shaper_block = RigBlock::of_type(BlockType::Waveshaper).named("Waveshaper");
        if let Some((drive, crush, reduce, mix)) = layer.shaper {
            shaper_block = shaper_block
                .with_param("drive", format!("{drive:.4}"))
                .with_param("crush", format!("{crush:.4}"))
                .with_param("reduce", format!("{reduce:.4}"))
                .with_param("mix", format!("{mix:.4}"));
        }
        let mut dfs_block = RigBlock::of_type(BlockType::Dfs).named("Dual Freq Shifter");
        if let Some((hz_a, mix_a, hz_b, mix_b, parallel)) = layer.dfs {
            dfs_block = dfs_block
                .with_param("shift_a_hz", format!("{hz_a:.2}"))
                .with_param("mix_a", format!("{mix_a:.4}"))
                .with_param("shift_b_hz", format!("{hz_b:.2}"))
                .with_param("mix_b", format!("{mix_b:.4}"))
                .with_param("parallel", if parallel { "1" } else { "0" });
        }
        let osc = osc
            .block(BlockType::Unison, "Unison")
            .block(BlockType::Harmonic, "Harmonia")
            .block(BlockType::FmOperator, "FM")
            .block(BlockType::RingModulator, "Ring Mod")
            .add(dfs_block)
            .add(shaper_block)
            .block(BlockType::Granular, "Granular");

        let filter_label = filter_labels[i].clone();
        let mut built = Container::layer(name)
            .param("level", format!("{:.3}", layer.level))
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
                // Filter 1 carries the imported cutoff/resonance when the
                // section is engaged. The algorithm comes from the
                // MEASURED type1 table (per-slot fingerprints through the
                // real engine); the factory name only decides the ladder
                // character. Cutoff goes through the calibrated
                // 15 Hz × 2^(9.55·v) curve into our normalized map.
                let mut f1 = RigBlock::of_type(BlockType::Filter).named(filter_label.clone());
                if layer.filter_active {
                    let (_, _, character) = classify_filter_full(&layer.filter_name);
                    let (mode, poles) =
                        layer
                            .filter_type1
                            .and_then(classify_type1)
                            .unwrap_or_else(|| {
                                let (m, p, _) = classify_filter_full(&layer.filter_name);
                                (m, p)
                            });
                    f1 = f1
                        .with_param(
                            "cutoff",
                            format!("{:.4}", omni_cutoff_norm(layer.filter_freq)),
                        )
                        .with_param("resonance", format!("{:.4}", layer.filter_res))
                        .with_param("mode", mode)
                        .with_param("poles", poles.to_string())
                        .with_param("character", character);
                }
                let mut f2 = RigBlock::of_type(BlockType::Filter).named("Filter 2");
                if let Some((freq, res)) = layer.filter2 {
                    if layer.filter_active {
                        f2 = f2
                            .with_param("cutoff", format!("{:.4}", omni_cutoff_norm(freq)))
                            .with_param("resonance", format!("{res:.4}"));
                    }
                }
                // SERIES chains the filters; PARALLEL sums them.
                let filters = if layer.filter_parallel {
                    Container::parallel("Filters")
                } else {
                    Container::module("Filters")
                };
                filters.add(f1).add(f2)
            })
            .add(Container::module("Amp").block(BlockType::Amp, "Amp"))
            .add(fx_rack_from("Layer FX", &layer.fx))
            .send("Aux Rack", "To Aux")
            .modulator(BlockType::Envelope, "Amp Env")
            .modulator_block({
                // The filter envelope carries its imported ADSR so the
                // mod engine gates/sweeps with the patch's own shape.
                let mut fe = RigBlock::of_type(BlockType::Envelope).named("Filter Env");
                if let Some((a, d, s, r)) = layer.filter_env {
                    fe = fe
                        .with_param("attack", format!("{a:.4}"))
                        .with_param("decay", format!("{d:.4}"))
                        .with_param("sustain", format!("{s:.4}"))
                        .with_param("release", format!("{r:.4}"));
                }
                fe
            })
            .modulator(BlockType::MultisegEnvelope, "Mod Env");
        // The filter section's own envelope depth (independent of matrix rows).
        if layer.filter_active && layer.filter_env_depth != 0.0 {
            built = built.route(
                "Filter Env",
                format!("{}.cutoff", filter_labels[i]),
                layer.filter_env_depth,
            );
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
        .add(fx_rack_from("Common FX", &patch.common_fx))
        .add(fx_rack_from("Aux Rack", &patch.aux_fx))
        .modulator(BlockType::ModMatrix, "Mod Matrix");
    for n in 1..=8usize {
        let mut lfo = RigBlock::of_type(BlockType::Lfo).named(format!("LFO {n}"));
        if let Some((rate, ty, sync, retrig)) = patch.lfos.get(n - 1) {
            // Normalized rate → Hz (exp sweep 0.05..20; CALIBRATE) and
            // normalized type → wave index 0..4 (4 = S&H).
            lfo = lfo
                .with_param("rate", format!("{:.4}", 0.05 * 400f32.powf(*rate)))
                .with_param("wave", format!("{}", (ty * 4.0).round() as u32));
            if *sync {
                // Tempo-synced: rate index → beats/cycle (CALIBRATE).
                let beats = [4.0, 2.0, 1.0, 0.5, 0.25, 0.125][(rate * 5.0).round() as usize];
                lfo = lfo.with_param("sync_beats", format!("{beats}"));
            }
            if *retrig {
                lfo = lfo.with_param("retrigger", "1");
            }
        }
        preset = preset.modulator_block(lfo);
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
pub fn load_patch_file(path: &Path, index: &SoundsourceIndex) -> Result<Container, String> {
    if path.extension().is_some_and(|e| e == "mlt_omn") {
        return super::multi::load_multi_file(path, index);
    }
    let xml = std::fs::read_to_string(path).map_err(|e| format!("read {path:?}: {e}"))?;
    let patch = parse_patch(&xml)?;
    Ok(patch_to_container(&patch, index))
}
