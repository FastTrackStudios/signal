//! DAW audio access. Call on the backend owner thread.
use daw::service::{ItemRef, Items, ProjectContext, TakeRef, Takes};
/// The item worth editing on a drum mic: the **longest** audio item on
/// the track whose fixed lane — when the track shows lanes at all — is
/// playing. Returns how many items were looked at, and the pick as
/// `(item guid, length secs, item volume)`.
// r[impl drums.open.runner]
pub(crate) fn edit_item<D: expression_editor_audio::daw_bound::DrumDaw>(
    daw: &D,
    ctx: &ProjectContext,
    track: &daw::service::Track,
) -> (usize, Option<(String, f64, f64)>) {
    let items = Items::get_items(
        daw,
        ctx.clone(),
        daw::service::TrackRef::Guid(track.guid.clone()),
    );
    let seen = items.len();
    let mut best: Option<(String, f64, f64)> = None;
    for item in items {
        if !is_playing(track, &item) {
            continue;
        }
        let is_audio = Takes::get_active_take(daw, ctx.clone(), ItemRef::Guid(item.guid.clone()))
            .is_some_and(|t| t.source_type == daw::service::SourceType::Audio);
        if !is_audio {
            continue;
        }
        let length = item.length.as_seconds();
        if best.as_ref().is_none_or(|(_, l, _)| length > *l) {
            best = Some((item.guid.clone(), length, item.volume));
        }
    }
    (seen, best)
}

/// Compose one track's *playing* audio into a single timeline buffer —
/// every playing-lane audio item read through the accessor and placed
/// at its position, out to `take_secs`. This is what a lane draws
/// after an edit landed on the daw: the split pieces where they now
/// sit, not where the take was when it loaded.
pub fn track_timeline<D: expression_editor_audio::daw_bound::DrumDaw>(
    daw: &D,
    ctx: &ProjectContext,
    track: &daw::service::Track,
    take_secs: f64,
    sample_rate: f64,
) -> Vec<f64> {
    let mut out = vec![0.0f64; (take_secs * sample_rate).ceil().max(0.0) as usize];
    let items = Items::get_items(
        daw,
        ctx.clone(),
        daw::service::TrackRef::Guid(track.guid.clone()),
    );
    let mut placed: Vec<_> = items
        .into_iter()
        .filter(|item| {
            is_playing(track, item)
                && Takes::get_active_take(daw, ctx.clone(), ItemRef::Guid(item.guid.clone()))
                    .is_some_and(|take| take.source_type == daw::service::SourceType::Audio)
        })
        .collect();
    // Ascending position: a later piece's attack overwrites the
    // previous piece's crossfade tail, which is what detection needs.
    placed.sort_by(|a, b| a.position.as_seconds().total_cmp(&b.position.as_seconds()));
    for item in placed {
        let Some((samples, rate)) =
            read_take_mono(daw, ctx, &item.guid, item.length.as_seconds(), item.volume)
        else {
            continue;
        };
        let at = (item.position.as_seconds() * sample_rate).round().max(0.0) as usize;
        if (rate - sample_rate).abs() < 1e-6 {
            for (i, s) in samples.iter().enumerate() {
                if at + i < out.len() {
                    out[at + i] = *s;
                }
            }
        } else {
            // Rates differing inside one kit is unusual; nearest-
            // neighbour is fine for drawing and detection.
            let n = (samples.len() as f64 * sample_rate / rate) as usize;
            for i in 0..n {
                let src = (i as f64 * rate / sample_rate) as usize;
                if at + i < out.len() && src < samples.len() {
                    out[at + i] = samples[src];
                }
            }
        }
    }
    out
}

/// Pull one take's audio through the accessor, chunked, mono, at the
/// source rate — the same read [`expression_editor_audio::AudioSession::load`] does, capped at
/// the item length. Returns `(samples, sample_rate)`.
// r[impl drums.open.runner]
pub fn read_take_mono<D: expression_editor_audio::daw_bound::DrumDaw>(
    daw: &D,
    ctx: &ProjectContext,
    item_guid: &str,
    length_secs: f64,
    volume: f64,
) -> Option<(Vec<f64>, f64)> {
    use daw::service::audio_accessor::GetSamplesRequest;

    let accessor = daw.create_take_accessor(
        ctx.clone(),
        ItemRef::Guid(item_guid.to_string()),
        TakeRef::Active,
    )?;
    // Probe for the source's own rate and channel count: asking for a
    // rate the host does not have would resample, and a resampled
    // analysis puts every hit slightly off.
    let probe = daw.get_samples(GetSamplesRequest {
        accessor_id: accessor.clone(),
        sample_rate: 0.0,
        num_channels: 0,
        start_time: 0.0,
        num_samples: 1,
    });
    let sample_rate = if probe.sample_rate > 0.0 {
        probe.sample_rate
    } else {
        48_000.0
    };
    let channels = probe.num_channels.max(1);

    // Chunked, because a whole multitrack take in one call makes the
    // host allocate all of it before returning any — same bound the
    // audio session reads at.
    const CHUNK: u32 = 1 << 16;
    let total = (length_secs.max(0.0) * sample_rate).ceil() as u32;
    let mut interleaved = Vec::with_capacity(total as usize * channels as usize);
    let mut done = 0u32;
    while done < total {
        let want = CHUNK.min(total - done);
        let chunk = daw.get_samples(GetSamplesRequest {
            accessor_id: accessor.clone(),
            sample_rate,
            num_channels: channels,
            start_time: done as f64 / sample_rate,
            num_samples: want,
        });
        if chunk.samples.is_empty() {
            break;
        }
        interleaved.extend_from_slice(&chunk.samples);
        done += want;
    }
    daw.destroy_accessor(&accessor);

    let mut samples = expression_editor_audio::to_mono(&interleaved, channels.max(1) as usize);
    if samples.is_empty() {
        return None;
    }
    // The accessor hands back source audio; the item's own gain is on
    // top, and detection thresholds are absolute.
    if volume != 1.0 {
        for s in &mut samples {
            *s *= volume;
        }
    }
    Some((samples, sample_rate))
}

/// The performance shared by capture and writes. Muted items and parked
/// fixed lanes are alternatives, not members of the audible kit.
pub(crate) fn is_playing(track: &daw::service::Track, item: &daw::service::Item) -> bool {
    !item.muted
        && (track.lane_count == 0
            || item
                .fixed_lane
                .is_none_or(|lane| lane < 64 && track.lane_play_mask & (1u64 << lane) != 0))
}
