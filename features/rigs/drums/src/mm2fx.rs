//! Map a parsed MM2 (Cradle) FX slot onto one of our built-in `signal-fx`
//! processors, wrapped as a [`HostedPlugin`] the drum mixer can host. This is
//! the bridge that turns MM2's per-piece mix recipe into real processing on our
//! own drum samples.
//!
//! Coverage: EQ (parametric bands), Modern/Vintage Compressor, Limiter (→ comp
//! with a brick-wall ratio), Reverb, Transient (NativeTransient dual-envelope
//! designer), Drive (→ NativeSaturate voiced by MM2's Soft/Tape/Hard mode).

use signal_fx::{NativeComp, NativeEq, NativeReverb, NativeSaturate, NativeTransient};
use signal_plugin_host::{HostedPlugin, PluginInstance};

use crate::cradle::{FxSlot, Mixer, Strip};

/// Normalize a strip/target name to a token set for matching, folding trailing
/// plural `s` so "Overhead" matches MM2's "Overheads".
fn tokens(name: &str) -> Vec<String> {
    let mut t: Vec<String> = name
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.strip_suffix('s').unwrap_or(s).to_string())
        .collect();
    t.sort();
    t
}

/// Find the MM2 strip whose name matches `target` (token-set equality).
pub fn match_strip<'a>(mixer: &'a Mixer, target: &str) -> Option<&'a Strip> {
    let want = tokens(target);
    mixer.strips.iter().find(|s| tokens(&s.name) == want)
}

/// Linear level → dB (MM2 `level` is a linear fader value).
pub fn level_to_db(level: f32) -> f32 {
    if level > 0.0 {
        20.0 * level.log10()
    } else {
        -96.0
    }
}

/// Build a hostable processor for one MM2 FX slot, or `None` if it's bypassed
/// or its type has no mapping yet.
pub fn build_processor(slot: &FxSlot, sample_rate: f64) -> Option<HostedPlugin> {
    build_instance(slot, sample_rate).map(HostedPlugin::from_instance)
}

/// As [`build_processor`], but returns the bare [`PluginInstance`] — what a
/// daw track FX slot hosts directly (the per-track kit mixer).
pub fn build_instance(slot: &FxSlot, sample_rate: f64) -> Option<Box<dyn PluginInstance>> {
    if slot.bypass {
        return None;
    }
    let inner: Box<dyn PluginInstance> = match slot.fx_type.as_str() {
        "EQ" => Box::new(build_eq(slot, sample_rate)),
        "Modern Compressor" | "Vintage Compressor" => Box::new(build_comp(slot, sample_rate)),
        "Limiter" => Box::new(build_limiter(slot, sample_rate)),
        "Reverb" => Box::new(build_reverb(slot, sample_rate)),
        "Transient" => Box::new(build_transient(slot, sample_rate)),
        "Drive" => Box::new(build_drive(slot, sample_rate)),
        other => {
            tracing::debug!(fx = other, "mm2 import: no DSP mapping yet — skipped");
            return None;
        }
    };
    Some(inner)
}

fn build_eq(slot: &FxSlot, sr: f64) -> NativeEq {
    let mut eq = NativeEq::new(sr);
    for (i, band) in slot
        .eq_bands()
        .iter()
        .filter(|b| b.enabled)
        .take(24)
        .enumerate()
    {
        let n = i + 1; // signal-fx bands are 1-indexed
        eq.set_named(&format!("b{n}_used"), 1.0);
        eq.set_named(&format!("b{n}_on"), 1.0);
        eq.set_named(&format!("b{n}_freq"), band.freq as f64);
        eq.set_named(&format!("b{n}_gain"), band.gain as f64);
        eq.set_named(&format!("b{n}_q"), band.q as f64);
        eq.set_named(&format!("b{n}_shape"), eq_shape_code(&band.mode));
    }
    eq
}

fn build_comp(slot: &FxSlot, sr: f64) -> NativeComp {
    let mut c = NativeComp::new(sr);
    if let Some(t) = slot.num("threshold") {
        c.set_named("threshold", t.clamp(-60.0, 0.0));
    }
    c.set_named("ratio", mm2_ratio(slot));
    c.set_named(
        "attack",
        comp_time_ms(slot, "attack", &FAST_ATTACK, 10.0).clamp(0.1, 200.0),
    );
    c.set_named(
        "release",
        comp_time_ms(slot, "release", &MED_RELEASE, 120.0).clamp(5.0, 1000.0),
    );
    c.set_named("knee", knee_db(slot.text("knee")));
    c
}

fn build_limiter(slot: &FxSlot, sr: f64) -> NativeComp {
    let mut c = NativeComp::new(sr);
    if let Some(t) = slot.num("threshold") {
        c.set_named("threshold", t.clamp(-60.0, 0.0));
    }
    c.set_named("ratio", 20.0); // brick wall
    c.set_named("attack", 0.5);
    c.set_named("release", 50.0);
    c.set_named("knee", 0.0);
    c
}

fn build_reverb(slot: &FxSlot, sr: f64) -> NativeReverb {
    let mut r = NativeReverb::new(sr);
    r.set_named("mix", slot.num("mix").unwrap_or(0.2).clamp(0.0, 1.0));
    r.set_named("decay", slot.num("decay").unwrap_or(0.45).clamp(0.0, 1.0));
    r.set_named("size", size_from_mode(slot.text("mode")));
    r
}

fn build_transient(slot: &FxSlot, sr: f64) -> NativeTransient {
    let mut t = NativeTransient::new(sr);
    // MM2 stores attack/sustain already normalized bipolar — same contract
    // as NativeTransient's params.
    t.set_named("attack", slot.num("attack").unwrap_or(0.0).clamp(-1.0, 1.0));
    t.set_named(
        "sustain",
        slot.num("sustain").unwrap_or(0.0).clamp(-1.0, 1.0),
    );
    t.set_named("mix", slot.num("mix").unwrap_or(1.0).clamp(0.0, 1.0));
    if let Some(out) = slot.num("output") {
        t.set_named("output", out.clamp(-24.0, 24.0));
    }
    t
}

fn build_drive(slot: &FxSlot, sr: f64) -> NativeSaturate {
    let mut s = NativeSaturate::new(sr);
    // Voicing first (model configures shapers/bias), then the amount —
    // MM2 modes: Soft (tube-ish), Tape, Hard (clipper).
    s.set_named(
        "model",
        match slot.text("mode") {
            Some("Tape") => 3.0,
            Some("Hard") => 6.0, // Fuzz: hard/diode shaper pair
            _ => 2.0,            // Soft → Tube
        },
    );
    // MM2 drive is 0..1; NativeSaturate drive is dB into the shaper.
    s.set_named(
        "drive",
        slot.num("drive").unwrap_or(0.0).clamp(0.0, 1.0) * 24.0,
    );
    s.set_named("mix", slot.num("mix").unwrap_or(1.0).clamp(0.0, 1.0));
    if let Some(out) = slot.num("output") {
        s.set_named("output", out.clamp(-24.0, 24.0));
    }
    if let Some(os) = slot.num("numOversamplingStages") {
        s.set_named("oversample", os.clamp(0.0, 3.0));
    }
    s
}

// ── param conversions ───────────────────────────────────────────────────────

/// MM2 EQ filter `mode` → signal-fx shape code (see `eq_shape_to_filter`).
fn eq_shape_code(mode: &str) -> f64 {
    match mode {
        "lowShelf" => 1.0,
        "highShelf" => 2.0,
        "highPass" => 3.0,
        "lowPass" => 4.0,
        "notch" => 5.0,
        _ => 0.0, // bell / peak
    }
}

/// MM2 stores compression ratio as its reciprocal (0.5 = 2:1, 0.25 = 4:1,
/// 0.17 ≈ 6:1). Invert to a real ratio, clamped to signal-fx's 1..20.
fn mm2_ratio(slot: &FxSlot) -> f64 {
    match slot.num("ratio") {
        Some(n) if n > 0.0 => (1.0 / n).clamp(1.0, 20.0),
        _ => 4.0,
    }
}

/// Discrete-time labels MM2's Vintage Compressor uses, mapped to ms.
const FAST_ATTACK: [(&str, f64); 3] = [("Fast", 1.0), ("Medium", 10.0), ("Slow", 40.0)];
const MED_RELEASE: [(&str, f64); 3] = [("Fast", 50.0), ("Medium", 150.0), ("Slow", 400.0)];

/// Compressor attack/release: numeric (Modern, in seconds → ms) or a discrete
/// label (Vintage: Fast/Medium/Slow).
fn comp_time_ms(slot: &FxSlot, key: &str, labels: &[(&str, f64); 3], default_ms: f64) -> f64 {
    if let Some(sec) = slot.num(key) {
        return sec * 1000.0;
    }
    if let Some(txt) = slot.text(key) {
        if let Some((_, ms)) = labels.iter().find(|(l, _)| *l == txt) {
            return *ms;
        }
    }
    default_ms
}

fn knee_db(knee: Option<&str>) -> f64 {
    match knee {
        Some("Hard") => 0.0,
        Some("Soft") => 12.0,
        _ => 6.0, // Medium / unset
    }
}

fn size_from_mode(mode: Option<&str>) -> f64 {
    match mode {
        Some("Hall") => 0.85,
        Some("Room") => 0.4,
        Some("Plate") => 0.6,
        _ => 0.5,
    }
}
