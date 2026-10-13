//! Item playback: mix one item's source audio into its track bus,
//! honoring fades, take envelopes, play-rate/pitch resampling and
//! REAPER channel modes.

use super::StereoBuffer;
use super::envelope::EnvelopeCursor;
use super::snapshot::ItemSnapshot;
use crate::audio_engine::source::AudioSource;

/// Returns `true` if any sample was mixed into the bus (drives the
/// caller's dirty-bus tracking).
pub(crate) fn mix_item_into_bus(
    bus: &mut StereoBuffer,
    audio: &AudioSource,
    item: &ItemSnapshot,
    block_start_seconds: f64,
    block_end_seconds: f64,
    output_rate: u32,
    playrate: f64,
) -> bool {
    let item_start = item.position_seconds;
    let item_end = item_start + item.length_seconds;

    // Quick out: item doesn't overlap this block.
    if item_end <= block_start_seconds || item_start >= block_end_seconds {
        return false;
    }

    let base_gain = (item.item_volume * item.take_volume) as f32;
    if base_gain == 0.0 {
        return false;
    }

    // Per-frame envelope cursors. All in item-relative time, which
    // advances monotonically as `frame` increments.
    let mut c_vol = item
        .take_volume_env
        .as_ref()
        .map(|p| EnvelopeCursor::new(p));
    let mut c_pan = item.take_pan_env.as_ref().map(|p| EnvelopeCursor::new(p));
    let mut c_mute = item.take_mute_env.as_ref().map(|p| EnvelopeCursor::new(p));
    let mut c_pitch = item.take_pitch_env.as_ref().map(|p| EnvelopeCursor::new(p));
    let has_take_pan = item.take_pan_env.is_some();

    let fade_in = item.fade_in_seconds.max(0.0);
    let fade_out = item.fade_out_seconds.max(0.0);

    let audio_rate = audio.sample_rate().max(1) as f64;
    let output_rate_f = output_rate as f64;

    // Pitch → rate multiplier. `powf` costs ~an entire frame's mixing
    // work, so fold the static case out of the loop; per-frame powf
    // only happens while a pitch envelope is actually present.
    let static_pitch_mult = if item.take_pitch_semitones == 0.0 {
        1.0
    } else {
        2f64.powf(item.take_pitch_semitones / 12.0)
    };
    let source_len = audio.frame_count() as f64;
    let last_frame = audio.frame_count().saturating_sub(1);

    let mut wrote = false;
    // A streamed source decodes where it is read: say so once a block.
    let mut told = !matches!(audio, AudioSource::Streamed(_));
    for frame in 0..bus.frames {
        let block_time = block_start_seconds + (frame as f64 * playrate / output_rate_f);
        if block_time < item_start || block_time >= item_end {
            continue;
        }
        let item_time = block_time - item_start;

        // Mute env first — early-out before paying for source lookup.
        let env_muted = c_mute
            .as_mut()
            .and_then(|c| c.eval_at(item_time))
            .map(|v| v > 0.5)
            .unwrap_or(false);
        if env_muted {
            continue;
        }

        // Pitch: static + envelope semitones → rate multiplier.
        let pitch_mult = match c_pitch.as_mut() {
            Some(c) => {
                let st = c.eval_at(item_time).unwrap_or(0.0);
                2f64.powf((item.take_pitch_semitones + st) / 12.0)
            }
            None => static_pitch_mult,
        };

        // r[impl drums.open.playback-warped]
        // Take playback time → source time through the take's stretch
        // markers (the same map the accessor and peaks use), falling
        // back to offset + rate when the take is unwarped. Pitch
        // scales the played position either way.
        let played = item_time * item.play_rate * pitch_mult;
        let source_time =
            match daw_proto::StretchMarker::source_position_at(&item.stretch_markers, played) {
                Some(s) => s,
                None => item.start_offset_seconds + played,
            };
        if source_time < 0.0 {
            continue;
        }
        let mut source_frame_f = source_time * audio_rate;
        if source_frame_f >= source_len {
            if item.loop_source && source_len > 0.0 {
                // `LOOP 1`: the source tiles across the item.
                source_frame_f %= source_len;
            } else {
                continue;
            }
        }

        // Linear interpolation between two source frames.
        // (`as usize` truncates toward zero == floor for the
        // non-negative values guaranteed above.)
        let i0 = source_frame_f as usize;
        if !told {
            audio.prefetch(i0, bus.frames);
            told = true;
        }
        let frac = (source_frame_f - i0 as f64) as f32;
        let i1 = (i0 + 1).min(last_frame);
        // REAPER channel mode: how the source's channels map onto the
        // stereo item signal.
        let (l, r) = match item.channel_mode {
            0 => audio.stereo_interp(i0, i1, frac),
            1 => {
                // Reverse stereo.
                let (l, r) = audio.stereo_interp(i0, i1, frac);
                (r, l)
            }
            2 => {
                // Mono downmix.
                let (l, r) = audio.stereo_interp(i0, i1, frac);
                let m = (l + r) * 0.5;
                (m, m)
            }
            m @ 3..=66 => {
                // Mono of source channel (3 = left/ch1, 4 = right/ch2,
                // 5+ = channel m−2, all 1-based).
                let v = audio.channel_interp(i0, i1, frac, (m - 3) as usize);
                (v, v)
            }
            m => {
                // Stereo pair starting at channel m−67 (0-based).
                let c = (m - 67) as usize;
                (
                    audio.channel_interp(i0, i1, frac, c),
                    audio.channel_interp(i0, i1, frac, c + 1),
                )
            }
        };

        // Volume envelope per-frame.
        let env_vol = c_vol
            .as_mut()
            .and_then(|c| c.eval_at(item_time))
            .unwrap_or(1.0) as f32;

        // Pan envelope per-frame (only engaged if envelope exists).
        let (pan_lg, pan_rg) = if has_take_pan {
            let v = c_pan
                .as_mut()
                .and_then(|c| c.eval_at(item_time))
                .unwrap_or(0.5);
            let (l, r) = super::balance((v - 0.5) * 2.0);
            (l as f32, r as f32)
        } else {
            (1.0, 1.0)
        };

        // Fade in / out, shaped per the item's REAPER curve type.
        let mut fade = 1.0f32;
        if fade_in > 0.0 && item_time < fade_in {
            fade *= fade_curve(item.fade_in_shape, item_time / fade_in) as f32;
        }
        let time_until_end = item_end - block_time;
        if fade_out > 0.0 && time_until_end < fade_out {
            fade *= fade_curve(item.fade_out_shape, (time_until_end / fade_out).max(0.0)) as f32;
        }
        let g = base_gain * env_vol * fade;

        bus.samples[frame * 2] += l * g * pan_lg;
        bus.samples[frame * 2 + 1] += r * g * pan_rg;
        wrote = true;
    }
    wrote
}

/// Fade gain at normalized position `t` (0 = silent end, 1 = full)
/// for a REAPER fade curve type. Polynomial approximations of
/// REAPER's curve family: fast-* are quadratic/quartic, slow-*
/// are smoothstep/smootherstep S-curves.
fn fade_curve(shape: daw_proto::item::FadeShape, t: f64) -> f64 {
    use daw_proto::item::FadeShape as FS;
    let t = t.clamp(0.0, 1.0);
    match shape {
        FS::Linear => t,
        FS::FastStart => 1.0 - (1.0 - t) * (1.0 - t),
        FS::FastEnd => t * t,
        FS::FastStartSteep => {
            let u = 1.0 - t;
            1.0 - u * u * u * u
        }
        FS::FastEndSteep => t * t * t * t,
        FS::SlowStartEnd => t * t * (3.0 - 2.0 * t),
        FS::SlowStartEndSteep => t * t * t * (t * (t * 6.0 - 15.0) + 10.0),
    }
}
