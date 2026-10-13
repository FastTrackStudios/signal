//! A Time face's Time knob, on its block: the knob's position, its
//! TIME | BEAT switch and the time it reads out (frame's `kit::time_knob`:
//! `<ns>/time_knob`, `<ns>/sync`, `<ns>/time_text`), from the block — and the
//! block params a turn or a flip of the switch sets.
//!
//! - A **reverb** in Time runs on its decay, read in seconds; in Beat its
//!   tail is a length in beats (`sync_div`, `REVERB_BEATS`) and its decay
//!   the one that rings that long at the tempo (the rig keeps it so as the
//!   tempo moves).
//! - A **delay** in Time runs on its time in ms (its taps free); in Beat on
//!   a division of the tempo (`tap_div_l`/`_r`, `DELAY_BEATS` — the engine
//!   derives the time).

use signal_guitar_proto::LiveBlock;
use signal_guitar_proto::time::{DELAY_BEATS, FREE_DIV, MOD_BEATS, REVERB_BEATS};
use signal_proto::block::BlockType;

use crate::control::verb_seconds;

/// The knob's position (0..1), the switch (0 Time, 1 Beat) and the time it
/// reads out.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeView {
    pub time: f64,
    pub sync: f64,
    pub text: String,
    /// A delay's time as it plays, ms (its division's at the tempo when
    /// locked): what the face's pictures animate from.
    pub playing_ms: Option<f64>,
}

fn v(b: &LiveBlock, name: &str, dflt: f32) -> f32 {
    b.params.iter().find(|p| p.name == name).map_or(dflt, |p| p.value)
}

fn seconds_text(s: f64) -> String {
    if s < 1.0 { format!("{:.0} ms", s * 1000.0) } else if s < 10.0 { format!("{s:.1} s") } else { format!("{s:.0} s") }
}

/// A modulation's rate range the knob sweeps in Time (Hz), logarithmically.
const MOD_HZ: (f64, f64) = (0.05, 12.0);

fn is_mod(bt: BlockType) -> bool {
    matches!(bt, BlockType::Chorus | BlockType::Flanger | BlockType::Vibrato | BlockType::Trem | BlockType::Phaser | BlockType::Rotary)
}

fn hz_text(hz: f64) -> String {
    format!("{hz:.2} Hz")
}

/// The delay time range the knob sweeps in Time (ms), logarithmically.
const DELAY_MS: (f64, f64) = (20.0, 2500.0);

/// The decay that rings for `seconds` (the engine's law, inverted).
fn verb_decay_for(alg: f32, var: f32, seconds: f64) -> f32 {
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if verb_seconds(alg, var, mid).0 < seconds {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// What the Time knob shows for `b` at `bpm`.
#[must_use]
pub fn view(b: &LiveBlock, bpm: f32) -> Option<TimeView> {
    let quarter = 60.0 / f64::from(bpm.max(1.0));
    match b.block_type {
        BlockType::Reverb => {
            let (alg, var, decay) = (v(b, "algorithm", 1.0), v(b, "variant", 0.0), v(b, "decay", 0.4));
            let secs = verb_seconds(alg, var, decay).0;
            let div = v(b, "sync_div", 0.0).round().max(0.0) as usize;
            Some(match REVERB_BEATS.get(div).filter(|_| div > 0) {
                Some((label, _)) => TimeView { time: (div as f64 - 1.0) / 6.0, sync: 1.0, text: format!("{label} · {}", seconds_text(secs)), playing_ms: None },
                None => TimeView { time: f64::from(decay), sync: 0.0, text: seconds_text(secs), playing_ms: None },
            })
        }
        BlockType::Delay => {
            let div = v(b, "tap_div_l", FREE_DIV as f32).round().max(0.0) as usize;
            Some(match DELAY_BEATS.iter().position(|d| d.1 == div) {
                Some(k) => {
                    let (label, _, len) = DELAY_BEATS[k];
                    TimeView { time: k as f64 / (DELAY_BEATS.len() - 1) as f64, sync: 1.0, text: format!("{label} · {:.0} ms", len * quarter * 1000.0), playing_ms: Some(len * quarter * 1000.0) }
                }
                None => {
                    let ms = f64::from(v(b, "time", 350.0));
                    let t = (ms / DELAY_MS.0).ln() / (DELAY_MS.1 / DELAY_MS.0).ln();
                    TimeView { time: t.clamp(0.0, 1.0), sync: 0.0, text: format!("{ms:.0} ms"), playing_ms: Some(ms) }
                }
            })
        }
        bt if is_mod(bt) => {
            let hz = f64::from(v(b, "rate", 1.0));
            let div = v(b, "sync_div", 0.0).round().max(0.0) as usize;
            Some(match MOD_BEATS.get(div).filter(|_| div > 0) {
                Some((label, beats)) => {
                    let synced = 1.0 / (beats * quarter);
                    TimeView { time: (div as f64 - 1.0) / (MOD_BEATS.len() - 2) as f64, sync: 1.0, text: format!("{label} · {}", hz_text(synced)), playing_ms: None }
                }
                None => TimeView { time: ((hz / MOD_HZ.0).ln() / (MOD_HZ.1 / MOD_HZ.0).ln()).clamp(0.0, 1.0), sync: 0.0, text: hz_text(hz), playing_ms: None },
            })
        }
        _ => None,
    }
}

/// A turn of the knob to `pos`: the block params it sets.
#[must_use]
pub fn turn(b: &LiveBlock, bpm: f32, pos: f64) -> Vec<(&'static str, f32)> {
    let pos = pos.clamp(0.0, 1.0);
    let synced = view(b, bpm).is_some_and(|t| t.sync >= 0.5);
    match (b.block_type, synced) {
        (BlockType::Reverb, false) => vec![("decay", pos as f32)],
        (BlockType::Reverb, true) => {
            let div = 1 + (pos * 6.0).round() as usize;
            let secs = REVERB_BEATS[div].1 * 60.0 / f64::from(bpm.max(1.0));
            vec![("sync_div", div as f32), ("decay", verb_decay_for(v(b, "algorithm", 1.0), v(b, "variant", 0.0), secs))]
        }
        (BlockType::Delay, false) => vec![("time", (DELAY_MS.0 * (DELAY_MS.1 / DELAY_MS.0).powf(pos)) as f32)],
        (BlockType::Delay, true) => {
            let div = DELAY_BEATS[(pos * (DELAY_BEATS.len() - 1) as f64).round() as usize].1 as f32;
            vec![("tap_div_l", div), ("tap_div_r", div)]
        }
        (bt, false) if is_mod(bt) => vec![("rate", (MOD_HZ.0 * (MOD_HZ.1 / MOD_HZ.0).powf(pos)) as f32)],
        (bt, true) if is_mod(bt) => {
            let div = 1 + (pos * (MOD_BEATS.len() - 2) as f64).round() as usize;
            let hz = f64::from(bpm.max(1.0)) / 60.0 / MOD_BEATS[div].1;
            vec![("sync_div", div as f32), ("rate", hz as f32)]
        }
        _ => Vec::new(),
    }
}

/// The switch flipped to Beat (`beat`) or Time: the block params it sets —
/// to the beat length nearest what it plays now, or back to running free
/// at the time it had.
#[must_use]
pub fn flip(b: &LiveBlock, bpm: f32, beat: bool) -> Vec<(&'static str, f32)> {
    let quarter = 60.0 / f64::from(bpm.max(1.0));
    match b.block_type {
        BlockType::Reverb if beat => {
            let (alg, var) = (v(b, "algorithm", 1.0), v(b, "variant", 0.0));
            let beats = verb_seconds(alg, var, v(b, "decay", 0.4)).0 / quarter;
            let div = (1..REVERB_BEATS.len()).min_by(|&x, &y| (REVERB_BEATS[x].1.ln() - beats.ln()).abs().total_cmp(&(REVERB_BEATS[y].1.ln() - beats.ln()).abs())).unwrap_or(4);
            vec![("sync_div", div as f32), ("decay", verb_decay_for(alg, var, REVERB_BEATS[div].1 * quarter))]
        }
        BlockType::Reverb => vec![("sync_div", 0.0)],
        BlockType::Delay if beat => {
            let quarters = f64::from(v(b, "time", 350.0)) / 1000.0 / quarter;
            let k = (0..DELAY_BEATS.len()).min_by(|&x, &y| (DELAY_BEATS[x].2.ln() - quarters.ln()).abs().total_cmp(&(DELAY_BEATS[y].2.ln() - quarters.ln()).abs())).unwrap_or(5);
            let div = DELAY_BEATS[k].1 as f32;
            vec![("tap_div_l", div), ("tap_div_r", div)]
        }
        BlockType::Delay => {
            // Free at the time it was playing, so nothing jumps.
            let div = v(b, "tap_div_l", FREE_DIV as f32).round() as usize;
            let ms = DELAY_BEATS.iter().find(|d| d.1 == div).map_or(f64::from(v(b, "time", 350.0)), |d| d.2 * quarter * 1000.0);
            vec![("time", ms as f32), ("tap_div_l", FREE_DIV as f32), ("tap_div_r", FREE_DIV as f32)]
        }
        bt if is_mod(bt) && beat => {
            // The length nearest its cycle now.
            let beats = 1.0 / (f64::from(v(b, "rate", 1.0)).max(0.01) * quarter);
            let div = (1..MOD_BEATS.len()).min_by(|&x, &y| (MOD_BEATS[x].1.ln() - beats.ln()).abs().total_cmp(&(MOD_BEATS[y].1.ln() - beats.ln()).abs())).unwrap_or(5);
            vec![("sync_div", div as f32), ("rate", (1.0 / (MOD_BEATS[div].1 * quarter)) as f32)]
        }
        bt if is_mod(bt) => vec![("sync_div", 0.0)],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_guitar_proto::BlockParam;

    fn block(bt: BlockType, params: &[(&str, f32)]) -> LiveBlock {
        LiveBlock {
            id: "b".into(),
            engine: 0,
            block_type: bt,
            name: "b".into(),
            bypassed: false,
            param_name: None,
            param_value: 0.0,
            param_min: 0.0,
            param_max: 1.0,
            output_level_db: None,
            detail: String::new(),
            asset: String::new(),
            module: String::new(),
            empty: false,
            params: params.iter().map(|(n, v)| BlockParam { name: (*n).to_string(), value: *v, min: 0.0, max: 1.0, overridden: false }).collect(),
            preset: String::new(),
            options: Vec::new(),
            option: 0,
            overridden: false,
        }
    }

    /// A Hall flipped to Beat at 120 BPM takes the length nearest its tail,
    /// and its decay rings exactly that long; turned, it steps by length.
    #[test]
    fn a_reverb_locks_to_the_beat() {
        let hall = block(BlockType::Reverb, &[("algorithm", 1.0), ("decay", verb_decay_for(1.0, 0.0, 2.1))]);
        let w = flip(&hall, 120.0, true);
        assert_eq!(w[0], ("sync_div", 4.0), "2.1 s at 120 BPM is a bar");
        let synced = block(BlockType::Reverb, &[("algorithm", 1.0), ("decay", w[1].1), ("sync_div", 4.0)]);
        let t = view(&synced, 120.0).unwrap();
        assert_eq!(t.sync, 1.0);
        assert!(t.text.starts_with("1 bar · 2.0"), "{}", t.text);
        assert_eq!(turn(&synced, 120.0, 1.0)[0], ("sync_div", 7.0), "full turn: 8 bars");
        assert_eq!(flip(&synced, 120.0, false), vec![("sync_div", 0.0)]);
    }

    /// A delay at 375 ms flipped to Beat at 120 BPM is a dotted eighth; back
    /// to Time it runs free at that time.
    #[test]
    fn a_delay_locks_to_the_beat_and_back() {
        let free = block(BlockType::Delay, &[("time", 375.0), ("tap_div_l", 7.0)]);
        assert_eq!(view(&free, 120.0).unwrap().text, "375 ms");
        let w = flip(&free, 120.0, true);
        assert_eq!(w, vec![("tap_div_l", 1.0), ("tap_div_r", 1.0)], "1/8. is division 1");
        let synced = block(BlockType::Delay, &[("time", 375.0), ("tap_div_l", 1.0)]);
        assert_eq!(view(&synced, 120.0).unwrap().text, "1/8. · 375 ms");
        assert_eq!(flip(&synced, 100.0, false), vec![("time", 450.0), ("tap_div_l", 7.0), ("tap_div_r", 7.0)]);
    }
}
