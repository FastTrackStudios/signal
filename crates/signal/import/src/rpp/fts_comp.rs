//! Building an FTS Comp plugin state out of a translated Pro-C 3 instance.
//!
//! The same thin layer [`super::fts_eq`] is for the equalizer: naming and
//! units only, with [`crate::fabfilter::proc3`] owning what a preset means.
//!
//! ## What crosses, and what does not
//!
//! Pro-C 3 and FTS Comp agree on more than they disagree — hold, lookahead
//! and range have *identical* ranges on both, which is not a coincidence:
//! the FTS compressor was built against Pro-C in the first place. Threshold,
//! ratio, attack, release, knee, stereo link, the input and output trims and
//! the six side-chain EQ bands all carry across in their own units.
//!
//! Three things do not, and are reported rather than quietly approximated:
//!
//! - **Style.** Pro-C has fourteen; FTS Comp has four detector models. The
//!   table below is a judgement about which of the four each of the fourteen
//!   is nearest, not a measurement, and it is the one place in this file
//!   where that is true.
//! - **Character.** Pro-C drives its character stage in decibels either side
//!   of zero; FTS Comp takes a 0..1 amount into a differently-shaped
//!   waveshaper. The amount is mapped, the sound is not claimed to match.
//! - **Mix above 100%.** Pro-C's dry/wet reaches 200%; FTS Comp's parallel
//!   fold stops at fully wet. Over-wet presets are clamped and named.
//!
//! Anything Pro-C carries that FTS Comp has no control for at all — auto
//! threshold, auto release, character routing, the stereo-link mode and its
//! mid-only half, the external side-chain input and its trim, oversampling —
//! is listed by [`unmapped`] so a conversion can say what it left behind.

use std::collections::BTreeMap;

use crate::fabfilter::proc3::ProC3;
use crate::rpp::fts_eq::ParamValue;

/// The plugin's CLAP id.
pub const CLAP_ID: &str = "com.fasttrackstudio.comp";
/// `Plugin::NAME`.
pub const NAME: &str = "FTS Comp";
/// `Plugin::VENDOR`.
pub const VENDOR: &str = "FastTrackStudio";
/// `Plugin::VERSION`.
pub const VERSION: &str = "0.1.0";

/// Side-chain EQ bands FTS Comp publishes.
const SC_BANDS: usize = 6;

/// Which FTS Comp detector model stands in for each of Pro-C 3's styles.
///
/// Indices into `comp_ui::params::STYLE_LABELS` — 0 Clean, 1 FET, 2 VCA,
/// 3 Opto — against Pro-C's fourteen, in its stored order. This is the one
/// table here that is a judgement rather than a measurement: fourteen
/// behaviours do not fit in four, and the honest thing is to say which four
/// they were folded into rather than to imply a match.
const STYLE_MAP: [u32; 14] = [
    0, // Clean     -> Clean
    0, // Versatile -> Clean
    3, // Smooth    -> Opto
    1, // Punch     -> FET
    0, // Upward    -> Clean (FTS has upward compression, but not as a style)
    2, // TTM       -> VCA
    3, // Op-El     -> Opto
    3, // Vari-Mu   -> Opto
    2, // Classic   -> VCA
    3, // Opto      -> Opto
    0, // Vocal     -> Clean
    0, // Mastering -> Clean
    2, // Bus       -> VCA
    1, // Pumping   -> FET
];

/// Pro-C's character settings against FTS Comp's waveshapers.
///
/// `CHARACTER_LABELS` is Tape, Tube, Trans, Bright, Cubic, Clip, Asym.
/// Pro-C's are Off, Tube, Diode, Bright — so three of the four have an
/// obvious counterpart and Off is the absence of one.
const CHARACTER_MAP: [u32; 4] = [
    0, // Off    -> irrelevant; drive goes to zero instead
    1, // Tube   -> Tube
    2, // Diode  -> Trans
    3, // Bright -> Bright
];

/// The character drive, in dB, that FTS Comp's fully-open drive stands for.
const DRIVE_FULL_SCALE_DB: f64 = 24.0;

/// Every stage-1 parameter FTS Comp publishes that a Pro-C preset can set,
/// at the value a fresh plugin gives it.
///
/// Written out in full for the same reason [`super::fts_eq`] writes all 24
/// bands: a plugin state sets what it names and resets nothing, so a preset
/// that stays quiet about a control inherits whatever was there before it.
/// That cost the equalizer up to 6 dB on instances late in a project, and it
/// would cost this one a stray side-chain filter or a leftover expander.
const DEFAULTS: &[(&str, ParamValue)] = &[
    ("threshold", ParamValue::F32(-20.0)),
    ("ratio", ParamValue::F32(4.0)),
    ("attack", ParamValue::F32(3.0)),
    ("release", ParamValue::F32(100.0)),
    ("knee", ParamValue::F32(6.0)),
    ("makeup", ParamValue::F32(0.0)),
    ("mix", ParamValue::F32(1.0)),
    ("link", ParamValue::F32(1.0)),
    ("style", ParamValue::I32(0)),
    ("charmode", ParamValue::I32(0)),
    ("drive", ParamValue::F32(0.0)),
    ("ingain", ParamValue::F32(0.0)),
    ("automake", ParamValue::Bool(false)),
    ("rmsmix", ParamValue::F32(0.0)),
    ("feedback", ParamValue::F32(0.0)),
    ("hold", ParamValue::F32(0.0)),
    ("lookahead", ParamValue::F32(0.0)),
    ("inertia", ParamValue::F32(0.0)),
    ("inertiadecay", ParamValue::F32(0.0)),
    ("schp", ParamValue::F32(20.0)),
    ("sclp", ParamValue::F32(20.0)),
    ("range", ParamValue::F32(60.0)),
    ("expthresh", ParamValue::F32(-60.0)),
    ("expratio", ParamValue::F32(1.0)),
    ("upthresh", ParamValue::F32(-60.0)),
    ("upratio", ParamValue::F32(1.0)),
    ("ceiling", ParamValue::F32(0.0)),
];

/// A side-chain EQ band at rest.
const SC_DEFAULTS: &[(&str, ParamValue)] = &[
    ("scshape", ParamValue::I32(0)),
    ("scfreq", ParamValue::F32(1000.0)),
    ("scgain", ParamValue::F32(0.0)),
    ("scq", ParamValue::F32(0.707)),
];

/// Translate a decoded Pro-C 3 into FTS Comp's parameter map.
pub fn plugin_params(comp: &ProC3) -> BTreeMap<String, ParamValue> {
    use ParamValue::{Bool, F32, I32};
    let mut out: BTreeMap<String, ParamValue> = BTreeMap::new();
    for (name, value) in DEFAULTS {
        out.insert((*name).to_string(), *value);
    }
    for n in 1..=SC_BANDS {
        for (name, value) in SC_DEFAULTS {
            out.insert(format!("{name}_{n}"), *value);
        }
    }

    let set = |out: &mut BTreeMap<String, ParamValue>, k: &str, v: ParamValue| {
        out.insert(k.to_string(), v);
    };

    set(&mut out, "threshold", F32(comp.threshold_db as f32));
    set(&mut out, "ratio", F32(comp.ratio as f32));
    set(&mut out, "attack", F32(comp.attack_ms as f32));
    set(&mut out, "release", F32(comp.release_ms as f32));
    set(&mut out, "knee", F32(comp.knee_db as f32));
    set(&mut out, "range", F32(comp.range_db as f32));
    set(&mut out, "hold", F32(comp.hold_ms as f32));
    set(&mut out, "lookahead", F32(comp.lookahead_ms as f32));
    set(&mut out, "link", F32(comp.stereo_link as f32));
    set(&mut out, "makeup", F32(comp.output_level_db as f32));
    set(&mut out, "ingain", F32(comp.input_level_db as f32));
    set(&mut out, "automake", Bool(comp.auto_gain));
    // FTS Comp's fold stops at fully wet; Pro-C's dry/wet goes to 200%.
    set(&mut out, "mix", F32(comp.mix.clamp(0.0, 1.0) as f32));

    let style = *STYLE_MAP
        .get(comp.style as usize)
        .unwrap_or(&0);
    set(&mut out, "style", I32(style as i32));

    // Character Off means no drive at all, whatever waveshaper is selected.
    let on = comp.character > 0;
    set(
        &mut out,
        "charmode",
        I32(*CHARACTER_MAP.get(comp.character as usize).unwrap_or(&0) as i32),
    );
    set(
        &mut out,
        "drive",
        F32(if on {
            (comp.character_drive_db / DRIVE_FULL_SCALE_DB).clamp(0.0, 1.0) as f32
        } else {
            0.0
        }),
    );

    // The side-chain EQ is Pro-Q's, six bands of it. A band that is not in
    // the preset or is switched off is left at rest rather than skipped, so
    // it cannot inherit a filter from whatever the plugin held before.
    for (slot, band) in comp.sc_eq.iter().take(SC_BANDS).enumerate() {
        let n = slot + 1;
        if !band.is_active() {
            continue;
        }
        set(&mut out, &format!("scfreq_{n}"), F32(band.freq_hz as f32));
        set(&mut out, &format!("scgain_{n}"), F32(band.gain_db as f32));
        // The band Q parameter is a display Q, as the equalizer's is.
        set(
            &mut out,
            &format!("scq_{n}"),
            F32((band.q * std::f64::consts::SQRT_2) as f32),
        );
        set(
            &mut out,
            &format!("scshape_{n}"),
            I32(band.shape.min(4) as i32),
        );
    }

    out
}

/// What this preset carries that FTS Comp has no control for.
///
/// Returned rather than logged so the caller decides how loudly to say it —
/// and returned at all because the alternative is a converter that drops a
/// preset's external side chain without mentioning it.
pub fn unmapped(comp: &ProC3) -> Vec<String> {
    let mut out = Vec::new();
    if comp.auto_threshold {
        out.push("auto threshold".into());
    }
    if comp.auto_release {
        out.push("auto release".into());
    }
    if comp.character > 0 && comp.character_pre {
        out.push("character before the compressor".into());
    }
    if comp.stereo_link_mode != 0 {
        out.push(format!("stereo link mode {}", comp.stereo_link_mode));
    }
    if comp.side_chain_input != 0 {
        out.push("an external side chain".into());
    }
    if comp.oversampling != 0 {
        out.push("oversampling".into());
    }
    if comp.mix > 1.0 {
        out.push(format!("{:.0}% mix, clamped to 100", comp.mix * 100.0));
    }
    if comp.dry_gain_db.is_some() {
        out.push("a separate dry path".into());
    }
    out
}

/// The bytes an FTS Comp instance would have saved for this Pro-C preset.
pub fn clap_state(comp: &ProC3) -> Vec<u8> {
    super::fts_eq::encode_state(VERSION, &plugin_params(comp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fabfilter::proc3;

    fn preset() -> ProC3 {
        proc3::decode(&crate::fabfilter::ffbs::FfbsState {
            version: 1,
            params: {
                let mut p = vec![0.0f32; proc3::PARAM_COUNT];
                p[proc3::field::THRESHOLD] = -24.0;
                p[proc3::field::RATIO] = 0.6; // 4:1
                p[proc3::field::KNEE] = 12.0;
                p[proc3::field::RANGE] = 60.0;
                p[proc3::field::ATTACK] = 0.4; // 16 ms
                p[proc3::field::RELEASE] = 0.2; // 56.5 ms
                p[proc3::field::STEREO_LINK] = 0.5; // fully linked
                p[proc3::field::MIX] = 1.0;
                p[proc3::field::WET_GAIN] = 0.0;
                p[proc3::field::DRY_GAIN] = -1.0;
                p[proc3::field::OUTPUT_LEVEL] = 0.1; // +3.6 dB
                p
            },
            metadata: Default::default(),
        })
        .expect("decode")
    }

    #[test]
    fn the_core_controls_cross_in_their_own_units() {
        let p = plugin_params(&preset());
        assert_eq!(p["threshold"], ParamValue::F32(-24.0));
        match p["ratio"] {
            // Interpolated off a measured table, so exact equality is the
            // wrong question to ask of it.
            ParamValue::F32(v) => assert!((v - 4.0).abs() < 0.01, "{v}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(p["knee"], ParamValue::F32(12.0));
        match p["attack"] {
            ParamValue::F32(v) => assert!((v - 16.005).abs() < 0.01, "{v}"),
            other => panic!("{other:?}"),
        }
        match p["release"] {
            ParamValue::F32(v) => assert!((v - 56.5).abs() < 0.1, "{v}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_output_trim_is_a_36_db_fraction() {
        // The Pro-Q lesson, repeated: read as decibels this preset's 0.1 is a
        // tenth of a dB, and it is really +3.6.
        let p = plugin_params(&preset());
        match p["makeup"] {
            ParamValue::F32(v) => assert!((v - 3.6).abs() < 0.01, "{v}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_limiting_ratio_survives() {
        // Pro-C limits at 100:1 and FTS Comp used to stop at 20:1, which
        // turned every limiting preset into a 20:1 compressor.
        let mut c = preset();
        c.ratio = 100.0;
        assert_eq!(plugin_params(&c)["ratio"], ParamValue::F32(100.0));
    }

    #[test]
    fn every_stage_parameter_is_stated() {
        let p = plugin_params(&preset());
        for (name, _) in DEFAULTS {
            assert!(p.contains_key(*name), "{name} is unstated");
        }
        assert!(p.contains_key("scq_6"), "the sixth side-chain band is unstated");
    }

    #[test]
    fn character_off_means_no_drive() {
        let mut c = preset();
        c.character = 0;
        c.character_drive_db = 18.0;
        assert_eq!(plugin_params(&c)["drive"], ParamValue::F32(0.0));
    }

    #[test]
    fn what_cannot_cross_is_named() {
        let mut c = preset();
        c.auto_threshold = true;
        c.side_chain_input = 1;
        c.mix = 2.0;
        let missing = unmapped(&c);
        assert!(missing.iter().any(|m| m.contains("auto threshold")), "{missing:?}");
        assert!(missing.iter().any(|m| m.contains("external side chain")), "{missing:?}");
        assert!(missing.iter().any(|m| m.contains("200% mix")), "{missing:?}");
        // And it is clamped, not passed through.
        assert_eq!(plugin_params(&c)["mix"], ParamValue::F32(1.0));
    }
}
