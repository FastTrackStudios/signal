//! A streamed song heard by its reference on this device — what a phone
//! joining a shared set plays by default, as the web page does
//! ([`crate::web_engine`]): one stereo stream of a few megabytes (the
//! song's mix but the guide, [`crate::reference`]) instead of every stem,
//! the guide playing live over it from the engine.
//!
//! The stems are attached all the same (their waveforms draw) but their
//! bytes are not fetched ([`crate::stream_set::stream`]). Loading the
//! multitracks ([`load_multitracks`]) fetches them; each song keeps its
//! reference until its stems have arrived at the playhead
//! ([`crate::stream_set`] hands over), so there is no gap.
//!
//! The reference is mixed into the device's output by the audio engine's
//! post-render hook ([`renderer`], installed by `crate::open`), for the
//! song the engine is playing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

use daw::standalone::audio_engine::AuxRenderer;
use daw::standalone::audio_engine::streamed::Streamed;

/// A song's reference: the reference proper, and its preview.
struct Heard {
    full: Streamed,
    preview: Option<Streamed>,
}

/// The songs heard by their reference, by project.
static HEARD: LazyLock<Mutex<HashMap<String, Heard>>> = LazyLock::new(Mutex::default);

/// Whether the multitracks were asked for: from then on every song is heard
/// by its stems.
static MULTITRACKS: AtomicBool = AtomicBool::new(false);

/// Whether songs are heard by their stems ([`load_multitracks`]) rather
/// than their reference.
#[must_use]
pub fn multitracks() -> bool {
    MULTITRACKS.load(Ordering::Relaxed)
}

/// Hear `project` by its reference: `full`, and its `preview` where that
/// has not arrived.
pub fn hear(project: &str, full: Streamed, preview: Option<Streamed>) {
    if let Ok(mut heard) = HEARD.lock() {
        heard.insert(project.to_owned(), Heard { full, preview });
    }
}

/// Hear `project` — a song opened from `folder` on this device — by its
/// reference, when it came down by its reference only
/// ([`crate::song_stream::REFERENCE_ONLY`]): its stems are not here, and
/// its reference is, read from disk as a streamed one is from the network.
/// Nothing for a song downloaded whole, or never downloaded.
pub fn hear_local(project: &str, folder: &std::path::Path) {
    let proxies = folder.join("Media").join("Proxies");
    if !proxies.join(crate::song_stream::REFERENCE_ONLY).exists() {
        return;
    }
    let file = proxies.join("Reference.ogg");
    match fts_sample::ogg_stream::OggStream::open_file(&file) {
        Ok(stream) => {
            let full = Streamed::new(stream.channels(), stream.sample_rate(), stream.frames());
            let feeder =
                daw::standalone::audio_engine::streamed::StreamFeeder::new(full.clone(), stream);
            daw::standalone::audio_engine::streamed::butler_adopt(feeder);
            hear(project, full, None);
        }
        Err(e) => {
            tracing::warn!(error = %e, reference.song = %project, "reference: a downloaded song's reference would not open; it plays its guide alone")
        }
    }
}

/// Hear `project` by its stems: its reference goes.
pub fn stop(project: &str) {
    if let Ok(mut heard) = HEARD.lock() {
        heard.remove(project);
    }
}

/// Whether any song is heard by its reference — what Settings offers the
/// multitracks for.
#[must_use]
pub fn any() -> bool {
    HEARD.lock().is_ok_and(|heard| !heard.is_empty())
}

/// Ask for the multitracks: every song's stems are fetched, and each hands
/// over from its reference as they arrive ([`crate::stream_set`]).
pub fn load_multitracks() {
    if !MULTITRACKS.swap(true, Ordering::Relaxed) {
        crate::stream_set::hear_stems();
    }
}

/// The audio engine's post-render hook for `project`: its reference, mixed
/// into the output while the transport plays and the song is heard by it.
///
/// On the audio thread: the table is only `try_lock`ed (a block is skipped
/// the moment a song is added or taken off, never waited for), and the mix
/// allocates nothing.
#[must_use]
pub fn renderer(project: String) -> AuxRenderer {
    Box::new(move |buf, clock| {
        if !clock.playing {
            return;
        }
        let Ok(heard) = HEARD.try_lock() else {
            return;
        };
        let Some(song) = heard.get(project.as_str()) else {
            return;
        };
        // Until it is first heard, what the block held before the mix: the
        // difference is the reference's.
        let before: f32 = if HEARD_ONCE.load(Ordering::Relaxed) {
            0.0
        } else {
            buf.iter().map(|s| s.abs()).sum()
        };
        crate::reference::mix(
            &song.full,
            song.preview.as_ref(),
            clock.pos_seconds,
            clock.sample_rate,
            clock.playrate,
            buf,
            clock.channels,
        );
        if !HEARD_ONCE.load(Ordering::Relaxed) {
            let after: f32 = buf.iter().map(|s| s.abs()).sum();
            if (after - before).abs() > 1e-3 && !HEARD_ONCE.swap(true, Ordering::Relaxed) {
                // Once per process: the one line that says the reference
                // reached the output (it allocates, and only this once).
                tracing::info!(reference.song = %project, reference.at = clock.pos_seconds, "reference: first heard");
            }
        }
    })
}

/// Whether a reference has been heard yet — see [`renderer`].
static HEARD_ONCE: AtomicBool = AtomicBool::new(false);
