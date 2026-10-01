//! Freezing a Core: its whole core tone — compressor, drives, amps, cabs,
//! post compressor, EQ — learnt as NAM captures (one a side), so a patch can
//! play it as a single model. The original never goes: a Core snapshot
//! keeps every setting and gains its frozen captures beside them
//! ([`PresetSnapshotDef::frozen_nam`]); `frozen` picks which one plays, and
//! flips back and forth as often as wanted.
//!
//! Trained the official way: the Core renders NAM's own training signal
//! (`v3_0_0.wav`) on an offline rig built exactly as the live one, and the
//! NAM trainer's standard pipeline (`scripts/nam_freeze_train.py` — latency
//! from the signal's blips, its data checks, the A2 model) learns it.
//! [`compare`] then plays a guitar through the live Core and the frozen
//! one and reports how far apart they are: the proof.
//!
//! The gate is the Core's but stays live ([`UNFROZEN_CORE_BLOCKS`]) — a
//! capture can't gate — and plays after the frozen captures. Room is not
//! the Core's at all. What sat between the Core's blocks (pitch, the volume
//! pedal, the pre effects) plays into the frozen Core, as it did into the
//! amps.

use std::path::{Path, PathBuf};

use signal_sampler::rig_profile::RigPatch;

use crate::compose::{Compositions, PresetSnapshotDef, flatten, snapshot_patch};
use crate::profiles::{DrivePresetDef, OverrideDef, PresetDef, ProfileDef, UNFROZEN_CORE_BLOCKS};

/// The sample rate everything is rendered and trained at (NAM's standard).
pub const RATE: u32 = 48_000;

/// The Core's blocks a frozen Core plays out — what the captures learnt.
/// The amps stay: they play the captures.
const LEARNT: [&str; 9] = ["Pre Comp", "Boost", "Drive 1", "Drive 2", "Drive 3", "Cab L", "Cab R", "Post Comp", "Amp EQ"];

/// Where the NAM trainer lives: a Python environment with
/// `neural-amp-modeler`, and the official training signal beside it.
#[must_use]
pub fn trainer_home() -> PathBuf {
    std::env::var_os("SIGNAL_NAM_TRAINER").map_or_else(
        || {
            let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
            data.unwrap_or_else(|| PathBuf::from(".")).join("signal").join("nam-trainer")
        },
        PathBuf::from,
    )
}

/// NAM's official training signal, v3.
#[must_use]
pub fn training_signal_path() -> PathBuf {
    trainer_home().join("signals").join("v3_0_0.wav")
}

fn slug(preset: &str, snapshot: &str) -> String {
    let s = |s: &str| s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect::<String>();
    format!("{}--{}", s(preset), s(snapshot))
}

/// Where a Core snapshot's freeze is worked: the rendered training data,
/// the trainer's checkpoints. A cache — only the captures it makes are
/// kept, with the config ([`capture_path`]).
#[must_use]
pub fn freeze_dir(preset: &str, snapshot: &str) -> PathBuf {
    let cache = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")));
    cache.unwrap_or_else(std::env::temp_dir).join("signal").join("freeze").join(slug(preset, snapshot))
}

/// Where a Core snapshot's frozen capture (`side` L or R) is kept: with
/// the config, in the rig directory's `frozen/` — it is part of the Core
/// preset, and goes where the preset goes.
#[must_use]
pub fn capture_path(preset: &str, snapshot: &str, side: &str) -> PathBuf {
    crate::library::rig_dir().join("frozen").join(format!("{}-{side}.nam", slug(preset, snapshot)))
}

/// Play a frozen Core on `patch`: the captures in the amps, at their trim,
/// and what they learnt bypassed. Called by [`flatten`] for a snapshot set
/// to play frozen.
pub(crate) fn play_frozen(patch: &mut crate::profiles::PatchDef, snap: &PresetSnapshotDef, overrides: &mut Vec<OverrideDef>, synthesised: &mut Vec<PresetDef>) {
    let mut pool = |side: &str, nam: &str| {
        let name = format!("Frozen · {} · {} · {side}", patch.rig_preset, snap.name);
        if !synthesised.iter().any(|p| p.name == name) {
            synthesised.push(PresetDef { name: name.clone(), nam: nam.to_string(), hash: String::new(), cab: String::new(), cab_hash: String::new(), level_db: snap.frozen_trim_db });
        }
        name
    };
    patch.preset = pool("L", &snap.frozen_nam);
    let right = if snap.frozen_nam2.is_empty() { snap.frozen_nam.clone() } else { snap.frozen_nam2.clone() };
    patch.preset2 = pool("R", &right);
    overrides.extend(LEARNT.iter().map(|b| OverrideDef { module: String::new(), block: (*b).to_string(), param: String::new(), op: "bypass".to_string(), value: 1.0, text: String::new() }));
}

/// A Core snapshot's chain, built exactly as the live rig builds a patch on
/// it, at its own level of 0 dB — live, or (`frozen` given) playing those
/// captures at 0 dB trim.
#[must_use]
pub fn snapshot_chain(comp: &Compositions, base: &ProfileDef, drives: &[DrivePresetDef], preset: &str, snapshot: &str, frozen: Option<(&str, &str)>) -> Option<RigPatch> {
    let mut def = base.clone();
    def.patches = vec![snapshot_patch(base, preset, snapshot, "freeze")?];
    let mut calm = comp.clone();
    let snap = calm
        .presets
        .iter_mut()
        .find(|p| p.name.eq_ignore_ascii_case(preset))
        .and_then(|p| p.snapshots.iter_mut().find(|s| s.name.eq_ignore_ascii_case(snapshot)))?;
    snap.level_db = 0.0;
    snap.frozen = frozen.is_some();
    if let Some((l, r)) = frozen {
        snap.frozen_nam = l.to_string();
        snap.frozen_nam2 = r.to_string();
        snap.frozen_trim_db = 0.0;
    }
    let flat = flatten(&def, &calm);
    let mut built = crate::nodes::to_nodes_with_store(&flat, drives).to_profile(&flat, drives);
    built.patches.drain(..).next()
}

/// `patch` with only the Core playing: its blocks (live or frozen), less
/// the gate; everything else — pre effects, modulation, time, the patch's
/// level, the master section — bypassed.
#[must_use]
pub fn core_only(patch: &RigPatch) -> RigPatch {
    let mut p = patch.clone();
    for b in &mut p.chain {
        let learnt = crate::compose::is_core_block(&b.name) && !UNFROZEN_CORE_BLOCKS.iter().any(|u| u.eq_ignore_ascii_case(&b.name));
        if !learnt {
            b.bypassed = true;
        }
    }
    p.output_trim_db = 0.0;
    p
}

fn read_wav(path: &Path) -> Result<Vec<f32>, String> {
    let mut r = hound::WavReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let spec = r.spec();
    if spec.sample_rate != RATE {
        return Err(format!("{} is {} Hz, not {RATE}", path.display(), spec.sample_rate));
    }
    let ch = usize::from(spec.channels.max(1));
    let all: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>().map_err(|e| e.to_string())?,
        hound::SampleFormat::Int => {
            let full = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|s| s.map(|v| v as f32 / full)).collect::<Result<_, _>>().map_err(|e| e.to_string())?
        }
    };
    Ok(all.chunks(ch).map(|f| f[0]).collect())
}

/// Write mono 24-bit, as the official signal is (the trainer reads PCM).
fn write_wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    let spec = hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 24, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| format!("{}: {e}", path.display()))?;
    let full = ((1i32 << 23) - 1) as f32;
    for s in samples {
        w.write_sample((s.clamp(-1.0, 1.0) * full).round() as i32).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())
}

/// What a freeze rendered: the Core's output of the training signal, a
/// side at a time.
pub struct Rendered {
    pub output_l: PathBuf,
    /// The right side, when the Core's sides differ (a stereo pair).
    pub output_r: Option<PathBuf>,
    /// The gain put on the renders to keep them under full scale, dB (the
    /// trainer normalises its output anyway; [`compare`] finds the level).
    pub headroom_db: f32,
}

/// Render a Core snapshot's training data into `dir`: the official signal
/// through the Core alone (its gate out), unaligned — the trainer finds the
/// latency itself.
///
/// # Errors
/// When the signal is missing, the snapshot does not build, or the rig
/// does not render.
pub fn render_training(comp: &Compositions, base: &ProfileDef, drives: &[DrivePresetDef], preset: &str, snapshot: &str, dir: &Path) -> Result<Rendered, String> {
    let input = read_wav(&training_signal_path())?;
    let chain = snapshot_chain(comp, base, drives, preset, snapshot, None).ok_or_else(|| format!("{preset} · {snapshot} does not build"))?;
    let (l, r) = crate::measure::render_through(&core_only(&chain), RATE, &input).ok_or("the Core did not render")?;
    let peak = l.iter().chain(&r).fold(0.0f32, |m, s| m.max(s.abs()));
    if peak <= 1e-6 {
        return Err("the Core rendered silence".into());
    }
    let gain = 10f32.powf(-1.0 / 20.0) / peak;
    let scale = |x: &[f32]| x.iter().map(|s| s * gain).collect::<Vec<f32>>();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let output_l = dir.join("output_l.wav");
    write_wav(&output_l, &scale(&l))?;
    let differs = l.iter().zip(&r).any(|(a, b)| (a - b).abs() > 1e-4);
    let output_r = if differs {
        let p = dir.join("output_r.wav");
        write_wav(&p, &scale(&r))?;
        Some(p)
    } else {
        None
    };
    Ok(Rendered { output_l, output_r, headroom_db: 20.0 * gain.log10() })
}

/// How the frozen Core played against the live one, a side at a time.
#[derive(Clone, Copy, Debug)]
pub struct Check {
    /// The output trim that lands the frozen side at the live one's level, dB.
    pub trim_db: f32,
    /// Error-to-signal ratio at that trim (0 = identical; NAM calls
    /// < 0.01 very good).
    pub esr: f32,
    /// The frozen side's lag behind the live one, samples.
    pub lag: isize,
}

/// The lag of `b` behind `a` (±`max`), by cross-correlation over a stretch
/// with signal in it.
fn lag_of(a: &[f32], b: &[f32], max: isize) -> isize {
    let start = a.iter().position(|s| s.abs() > 1e-3).unwrap_or(0);
    let len = (RATE as usize).min(a.len().saturating_sub(start));
    let corr = |lag: isize| -> f64 {
        (start..start + len)
            .filter_map(|i| {
                let j = i as isize + lag;
                (j >= 0 && (j as usize) < b.len()).then(|| f64::from(a[i]) * f64::from(b[j as usize]))
            })
            .sum()
    };
    (-max..=max).max_by(|&x, &y| corr(x).total_cmp(&corr(y))).unwrap_or(0)
}

fn check_side(live: &[f32], frozen: &[f32]) -> Check {
    let lag = lag_of(live, frozen, 2400);
    let pairs: Vec<(f64, f64)> = (0..live.len())
        .filter_map(|i| {
            let j = i as isize + lag;
            (j >= 0 && (j as usize) < frozen.len()).then(|| (f64::from(live[i]), f64::from(frozen[j as usize])))
        })
        .collect();
    let (ll, ff, lf) = pairs.iter().fold((0.0, 0.0, 0.0), |(ll, ff, lf), (l, f)| (ll + l * l, ff + f * f, lf + l * f));
    // The least-squares gain: the level the frozen side needs.
    let g = if ff > 0.0 { lf / ff } else { 1.0 };
    let err: f64 = pairs.iter().map(|(l, f)| (l - g * f).powi(2)).sum();
    Check { trim_db: (20.0 * g.abs().max(1e-9).log10()) as f32, esr: if ll > 0.0 { (err / ll) as f32 } else { f32::NAN }, lag }
}

/// Play a guitar (the DI reference, which the training never heard) through
/// the live Core and through the frozen one, and measure the difference a
/// side at a time.
///
/// # Errors
/// When either chain does not build or render.
pub fn compare(comp: &Compositions, base: &ProfileDef, drives: &[DrivePresetDef], preset: &str, snapshot: &str, nam_l: &str, nam_r: &str) -> Result<(Check, Check), String> {
    let di = signal_sampler::nam_calibrate::DiReference::load_or_synthetic(f64::from(RATE));
    let di: Vec<f32> = di.samples.iter().map(|&s| s as f32).collect();
    let live = snapshot_chain(comp, base, drives, preset, snapshot, None).ok_or("the live Core does not build")?;
    let frozen = snapshot_chain(comp, base, drives, preset, snapshot, Some((nam_l, nam_r))).ok_or("the frozen Core does not build")?;
    let (ll, lr) = crate::measure::render_through(&core_only(&live), RATE, &di).ok_or("the live Core did not render")?;
    let (fl, fr) = crate::measure::render_through(&core_only(&frozen), RATE, &di).ok_or("the frozen Core did not render")?;
    Ok((check_side(&ll, &fl), check_side(&lr, &fr)))
}

/// The snapshot `preset` · `snapshot` in `comp`, to write a freeze into.
pub fn snapshot_mut<'a>(comp: &'a mut Compositions, preset: &str, snapshot: &str) -> Option<&'a mut PresetSnapshotDef> {
    comp.presets
        .iter_mut()
        .find(|p| p.name.eq_ignore_ascii_case(preset))
        .and_then(|p| p.snapshots.iter_mut().find(|s| s.name.eq_ignore_ascii_case(snapshot)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A snapshot set to play frozen plays its captures in the amps and
    /// bypasses what they learnt — the gate and everything outside the Core
    /// untouched; set back, it plays exactly as before.
    #[test]
    fn a_frozen_core_plays_its_captures_and_flips_back() {
        let (lib, comp, drives) = crate::compose::golden::shipped();
        let (preset, snapshot) = (comp.presets[0].name.clone(), comp.presets[0].snapshots[0].name.clone());
        let mut frozen = comp.clone();
        let s = snapshot_mut(&mut frozen, &preset, &snapshot).unwrap();
        s.frozen_nam = "/frozen/l.nam".into();
        s.frozen_nam2 = "/frozen/r.nam".into();
        s.frozen = true;
        let mut def = lib.clone();
        def.patches = vec![snapshot_patch(&lib, &preset, &snapshot, "p").unwrap()];
        let chain_of = |c: &Compositions| crate::profiles::build_profile(&flatten(&def, c), &drives).patches.remove(0).chain;
        let chain = chain_of(&frozen);
        let block = |n: &str| chain.iter().find(|b| b.name == n).unwrap_or_else(|| panic!("no {n}"));
        assert_eq!(block("Amp L").nam, "/frozen/l.nam");
        assert_eq!(block("Amp R").nam, "/frozen/r.nam");
        for b in ["Pre Comp", "Post Comp", "Amp EQ", "Cab L", "Drive 1"] {
            assert!(block(b).bypassed, "{b} plays in a frozen Core");
        }
        assert!(!block("Gate").bypassed, "the gate stays live");
        // Flipped back: the live Core, block for block.
        snapshot_mut(&mut frozen, &preset, &snapshot).unwrap().frozen = false;
        let shape = |c: &Compositions| format!("{:?}", chain_of(c).iter().map(|b| (b.name.clone(), b.nam.clone(), b.bypassed, b.params.clone())).collect::<Vec<_>>());
        assert_eq!(shape(&frozen), shape(&comp));
    }
}
