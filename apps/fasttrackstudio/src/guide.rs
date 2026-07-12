//! The audible guide: click, count-in, and section cues rendered into
//! the app's audio output.
//!
//! Wiring:
//! - [`install`] runs on the audio thread right after
//!   `attach_audio_engine` succeeds. It builds a [`GuideEngine`] at the
//!   device sample rate (real samples from `~/.config/fts/guide-samples/`
//!   when present, synthesized placeholders otherwise) and registers
//!   daw-standalone's aux post-render hook, which adds the guide into
//!   the interleaved output block every callback.
//! - The audio callback only ever `try_lock`s the engine mutex — under
//!   contention (a schedule rebuild in progress) the guide skips that
//!   block instead of blocking the audio thread.
//! - [`set_current_song`] is called by the session event bridge whenever
//!   the active song changes; the schedule rebuild (sections → cue
//!   timeline, plus optional TTS cue pre-rendering) happens on a
//!   throwaway worker thread and is swapped into the engine under the
//!   mutex.
//! - TTS: with the `tts` cargo feature, cues are synthesized only when
//!   `FTS_TTS=1` AND `~/.config/fts/tts-voice.wav` exists (no implicit
//!   model downloads). Cached cues in `~/.config/fts/tts-cache/` load
//!   feature-free; everything else falls back to the synthesized chime.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use session_guide::{
    BlockClock, ClickSound, CueBank, CueSchedule, GuideConfig, GuideEngine, GuideSongTiming,
    SampleBank, TtsRenderer, sections_from_song,
};

/// Master enable for the guide buses (wired to the settings popover).
/// Independent of engine installation so the UI can flip it at any time.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// Per-bus toggles, applied into `GuideEngine.config` under the engine
/// mutex by [`apply_bus_config`]. Click (metronome) and count-in on by
/// default; spoken/section cues OFF by default — the assetless fallback is
/// a two-tone chime, which reads as noise, so cues stay silent until a TTS
/// voice or real section samples are configured.
static CLICK_ON: AtomicBool = AtomicBool::new(true);
static COUNT_ON: AtomicBool = AtomicBool::new(true);
static CUES_ON: AtomicBool = AtomicBool::new(false);

/// The engine behind the aux hook, once audio is up.
static GUIDE: OnceLock<Arc<GuideShared>> = OnceLock::new();

/// Latest active song, kept so a rebuild can run whenever either side
/// (audio install / bridge event) arrives first.
static CURRENT_SONG: Mutex<Option<session_proto::Song>> = Mutex::new(None);

/// Frames of stereo scratch the hook pre-allocates (matches the aux
/// staging cap in daw-standalone).
const MAX_FRAMES: usize = 16_384;

struct GuideShared {
    engine: Mutex<GuideEngine>,
    /// Log the audibility marker only once per process.
    first_audible_logged: AtomicBool,
    /// Device sample rate the bank was built at.
    sample_rate: u32,
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    tracing::info!("guide {}", if on { "enabled" } else { "disabled" });
}

pub fn click_enabled() -> bool {
    CLICK_ON.load(Ordering::Relaxed)
}
pub fn count_enabled() -> bool {
    COUNT_ON.load(Ordering::Relaxed)
}
pub fn cues_enabled() -> bool {
    CUES_ON.load(Ordering::Relaxed)
}

pub fn set_click_enabled(on: bool) {
    CLICK_ON.store(on, Ordering::Relaxed);
    apply_bus_config();
}
pub fn set_count_enabled(on: bool) {
    COUNT_ON.store(on, Ordering::Relaxed);
    apply_bus_config();
}
pub fn set_cues_enabled(on: bool) {
    CUES_ON.store(on, Ordering::Relaxed);
    apply_bus_config();
}

/// Push the per-bus toggles into the live engine config. Briefly locks the
/// engine mutex — the audio callback only `try_lock`s it, so at worst it
/// skips one block. No-op until the engine is installed (install() calls
/// this to seed the initial state, e.g. cues-off).
fn apply_bus_config() {
    let Some(shared) = GUIDE.get() else { return };
    let Ok(mut engine) = shared.engine.lock() else { return };
    let click = CLICK_ON.load(Ordering::Relaxed);
    engine.config.enable_beat = click;
    engine.config.enable_measure_accent = click;
    engine.config.enable_count = COUNT_ON.load(Ordering::Relaxed);
    engine.config.enable_guide = CUES_ON.load(Ordering::Relaxed);
}

fn config_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/fts")
}

/// Build the guide engine at the device rate and register the aux
/// post-render hook. Called from the audio thread after
/// `attach_audio_engine` succeeds (the hook itself then runs on the
/// cpal callback).
pub fn install(audio: &daw_standalone::audio_engine::AudioEngine) {
    let sample_rate = audio.sample_rate();

    let mut engine = GuideEngine::new(GuideConfig::default());
    // Real sample library first (legacy FTS-GUIDE layout under
    // ~/.config/fts/guide-samples/{Click,Counts,Guide}), synthesized
    // placeholders for anything missing — audible with zero assets.
    let samples_dir = config_dir().join("guide-samples");
    engine
        .bank_mut()
        .load_click(&samples_dir.join("Click"), ClickSound::Blip, sample_rate);
    engine
        .bank_mut()
        .load_counts(&samples_dir.join("Counts"), "English Female", sample_rate);
    engine
        .bank_mut()
        .load_guide_dir(&samples_dir.join("Guide"), sample_rate);
    engine.bank_mut().synthesize_defaults(sample_rate);

    let shared = Arc::new(GuideShared {
        engine: Mutex::new(engine),
        first_audible_logged: AtomicBool::new(false),
        sample_rate,
    });
    if GUIDE.set(shared.clone()).is_err() {
        tracing::warn!("guide engine already installed; skipping re-install");
        return;
    }

    // The aux hook: planar scratch (pre-allocated, never grown) →
    // GuideEngine::render_stereo → add into the interleaved block.
    let mut left = vec![0.0f32; MAX_FRAMES];
    let mut right = vec![0.0f32; MAX_FRAMES];
    audio.set_aux_renderer(Box::new(move |buf, clock| {
        if !ENABLED.load(Ordering::Relaxed) {
            return;
        }
        // Never block the audio thread: skip the block on contention
        // (e.g. a schedule rebuild holding the engine).
        let Ok(mut engine) = shared.engine.try_lock() else {
            return;
        };
        let channels = clock.channels.max(1);
        let frames = (buf.len() / channels).min(MAX_FRAMES);
        if frames == 0 {
            return;
        }
        let (l, r) = (&mut left[..frames], &mut right[..frames]);
        l.fill(0.0);
        r.fill(0.0);
        engine.render_stereo(
            l,
            r,
            &BlockClock {
                playing: clock.playing,
                pos_seconds: clock.pos_seconds,
                pos_beats: clock.pos_beats,
                tempo_bpm: clock.tempo_bpm,
                time_sig_num: clock.time_sig_num,
                time_sig_den: clock.time_sig_den,
                sample_rate: clock.sample_rate,
            },
        );
        drop(engine);

        let mut audible = false;
        for f in 0..frames {
            let (lv, rv) = (l[f], r[f]);
            if lv != 0.0 || rv != 0.0 {
                audible = true;
            }
            if channels == 1 {
                buf[f] += (lv + rv) * 0.5;
            } else {
                buf[f * channels] += lv;
                buf[f * channels + 1] += rv;
            }
        }
        if audible && !shared.first_audible_logged.swap(true, Ordering::Relaxed) {
            // One-shot audibility marker (allocates/logs exactly once).
            tracing::info!(
                "guide: first audible block at beat {:.2} ({:.2}s, {:.1} bpm)",
                clock.pos_beats,
                clock.pos_seconds,
                clock.tempo_bpm,
            );
        }
    }));
    tracing::info!("guide engine installed on aux hook ({sample_rate} Hz)");

    // Spoken cues default ON when a reference voice is present (TTS renders
    // section names); OFF otherwise so the synth chime never fires unasked.
    if tts_voice_path().exists() {
        CUES_ON.store(true, Ordering::Relaxed);
    }
    // Seed the live config from the per-bus toggles.
    apply_bus_config();

    // A song may have been announced before audio came up.
    spawn_rebuild();
}

/// Called by the session event bridge whenever the active song changes
/// (or its data is re-hydrated). Cheap: stores the song and kicks the
/// rebuild worker.
pub fn set_current_song(song: session_proto::Song) {
    match CURRENT_SONG.lock() {
        Ok(mut slot) => *slot = Some(song),
        Err(_) => return,
    }
    spawn_rebuild();
}

/// Rebuild the cue schedule off the audio thread (and off the UI task —
/// TTS pre-rendering and wav cache reads can be slow).
fn spawn_rebuild() {
    if GUIDE.get().is_none() {
        return; // install() will re-kick once audio is up
    }
    if let Err(e) = std::thread::Builder::new()
        .name("fts-guide-schedule".into())
        .spawn(rebuild_schedule)
    {
        tracing::warn!("could not spawn guide schedule rebuild: {e}");
    }
}

fn rebuild_schedule() {
    let Some(shared) = GUIDE.get() else { return };
    let song = match CURRENT_SONG.lock() {
        Ok(slot) => slot.clone(),
        Err(_) => None,
    };
    let Some(song) = song else { return };

    let sections = sections_from_song(&song);
    let timing = GuideSongTiming::from_song(&song);

    // TTS cues: pre-render/load into a TEMP bank so slow work never
    // holds the engine mutex the audio callback try_locks.
    let texts = CueSchedule::tts_texts(&sections);
    let cue_bank = CueBank::new(config_dir().join("tts-cache"), tts_voice_id());
    let mut temp = SampleBank::default();
    {
        let uncached = texts
            .iter()
            .any(|t| !cue_bank.cache_path(t).exists());
        let mut tts = if uncached { load_tts() } else { None };
        let loaded = cue_bank.preload_into(
            &mut temp,
            &texts,
            shared.sample_rate,
            tts.as_deref_mut(),
        );
        if !loaded.is_empty() {
            tracing::info!("guide: {} TTS cue(s) ready for '{}'", loaded.len(), song.name);
        }
    }

    let Ok(mut engine) = shared.engine.lock() else { return };
    for (key, sample) in temp.guides {
        engine.bank_mut().insert_guide(key, sample);
    }
    engine.set_sections(&sections, &timing);
    drop(engine);
    tracing::info!(
        "guide: schedule rebuilt for '{}' ({} sections, {:.0} bpm {}/{})",
        song.name,
        sections.len(),
        timing.tempo_bpm,
        timing.time_sig_num,
        timing.time_sig_den,
    );
}

/// Stable cue-cache voice id. Must match `ChatterboxTts::voice_id()` for
/// the app's config so cached cues load even when the `tts` feature is
/// compiled out.
fn tts_voice_id() -> String {
    "chatterbox:ResembleAI/chatterbox-turbo-ONNX@main:Quantized:fts".to_string()
}

/// Path to the reference voice clip Chatterbox clones. Spoken cues need it
/// (voice-cloning), so its presence gates TTS.
fn tts_voice_path() -> PathBuf {
    config_dir().join("tts-voice.wav")
}

/// Load the local TTS backend. Activates whenever the reference voice wav
/// exists (the flake ships onnxruntime; the model auto-downloads to the HF
/// cache on first use). Set `FTS_TTS=0` to force the synth fallback even
/// with a voice present. Loading is slow (model load + first-time download)
/// — this only runs on the guide's rebuild worker, never the audio thread.
#[cfg(feature = "tts")]
fn load_tts() -> Option<Box<dyn TtsRenderer>> {
    if std::env::var("FTS_TTS").map(|v| v == "0").unwrap_or(false) {
        return None;
    }
    let voice = tts_voice_path();
    if !voice.exists() {
        tracing::info!(
            "no TTS voice at {}; spoken cues stay off (drop a clean ~10s voice clip there)",
            voice.display(),
        );
        return None;
    }
    let config = session_guide::ChatterboxTtsConfig::with_voice(voice, "fts");
    match session_guide::ChatterboxTts::load(config) {
        Ok(tts) => {
            debug_assert_eq!(tts.voice_id(), tts_voice_id());
            Some(Box::new(tts))
        }
        Err(e) => {
            tracing::warn!("chatterbox TTS unavailable: {e}");
            None
        }
    }
}

/// Without the `tts` feature only already-cached cues load; uncached
/// section names fall back to the synthesized chime / count samples.
#[cfg(not(feature = "tts"))]
fn load_tts() -> Option<Box<dyn TtsRenderer>> {
    None
}
