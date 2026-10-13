//! The guide instrument: what makes the Click, Count and Guide MIDI tracks
//! audible on `daw-standalone`.
//!
//! Session writes the click, the count-in and the spoken section cues as
//! MIDI notes (`session_guide::midi`: 60–65 click, 72–79 count, 84–102
//! cues). This is the instrument on those tracks: `session_guide`'s
//! `GuideEngine` in MIDI mode — the same DSP the FTS Guide CLAP/VST3
//! plugin wraps — running in-process as a native `PluginInstance`, so the
//! renderer feeds it each track's notes like any hosted synth.
//!
//! A spoken cue and the count note under it are both in the MIDI — on
//! the Guide and Count tracks — and the cue takes the count's place in the
//! audio: "Verse, 2, 3, 4". The generator tags each count note a cue lands
//! on (`COUNT_UNDER_CUE_VELOCITY`); the instrument on the Guide track
//! (`fts.guide:guide`) reports every block that it is playing unmuted, and
//! the Count track's instrument skips a tagged note while it is. Mute the
//! Guide track and the "1" is back. Mute is a slow fact, so reading the
//! last block's report is enough — which makes it hold in any track order.
//!
//! Samples come from the FTS-GUIDE library ([`Library`]): natively the
//! folder at `~/.config/fts/guide-samples/{Click,Counts,Guide}`, in a
//! browser the files it needs, fetched as Ogg
//! (`session_guide::samples::library`). Anything missing is synthesized
//! (clicks and count beeps), except spoken cues, which need the real
//! samples.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use daw_proto::live_midi::MidiEventExt;
use daw_standalone::plugin::{
    FxFactory, PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance,
    PluginParamInfo,
};
use session_guide::{BlockClock, ClickSound, GuideConfig, GuideEngine, TriggerSource};

/// The FX ident the guide generator puts on its tracks.
pub const IDENT: &str = "fts.guide";

/// The click kit and voice the guide plays.
pub const CLICK: ClickSound = ClickSound::Cowbell;
pub const VOICE: &str = "English Female";

/// Where the instrument's samples come from.
#[derive(Clone)]
pub enum Library {
    /// The FTS-GUIDE folder on this machine.
    Folder(PathBuf),
    /// The files MIDI-mode playback needs (library-relative path → bytes,
    /// all in format `ext`) — what a browser fetched.
    Files {
        files: Arc<HashMap<String, Vec<u8>>>,
        ext: &'static str,
    },
}

impl Library {
    fn load_into(&self, bank: &mut session_guide::SampleBank, rate: u32) {
        match self {
            Self::Folder(dir) => {
                bank.load_click(&dir.join("Click"), CLICK, rate);
                bank.load_counts(&dir.join("Counts"), VOICE, rate);
                bank.load_guide_dir(&dir.join("Guide"), rate);
            }
            Self::Files { files, ext } => bank.load_bytes(files, ext, CLICK, VOICE, rate),
        }
        // On-beats high, off-beat eighths low, the bar's one the same as
        // any other beat — see `SampleBank::beats_high_offbeats_low`.
        bank.beats_high_offbeats_low();
        bank.synthesize_defaults(rate);
    }
}

/// Where the FTS-GUIDE sample library lives.
#[must_use]
pub fn samples_dir() -> PathBuf {
    std::env::var_os("FTS_GUIDE_SAMPLES").map_or_else(
        || {
            std::env::var_os("HOME")
                .map_or_else(|| PathBuf::from("."), PathBuf::from)
                .join(".config/fts/guide-samples")
        },
        PathBuf::from,
    )
}

/// Whether the Guide track's instrument is playing unmuted — shared by the
/// guide instruments of one engine.
#[derive(Default)]
pub struct CueGate {
    /// The last render cycle the Guide track's instrument ran unmuted in.
    guide_alive: std::sync::atomic::AtomicU64,
}

impl CueGate {
    fn guide_playing_in(&self, cycle: u64) {
        // +1 so zero means "never".
        self.guide_alive
            .fetch_max(cycle + 1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether the Guide track played this block or the one before —
    /// whichever order the two tracks render in.
    fn guide_is_playing(&self, cycle: u64) -> bool {
        let seen = self.guide_alive.load(std::sync::atomic::Ordering::Relaxed);
        seen != 0 && seen + 1 >= cycle
    }
}

/// Which guide track an instrument plays for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Click,
    Count,
    Guide,
    /// Named plainly (`fts.guide`): plays whatever it is given.
    Any,
}

impl Role {
    fn of(fx_name: &str) -> Option<Self> {
        match fx_name.strip_prefix(IDENT)? {
            "" => Some(Self::Any),
            ":click" => Some(Self::Click),
            ":count" => Some(Self::Count),
            ":guide" => Some(Self::Guide),
            _ => None,
        }
    }
}

/// One guide engine, fed from its track's MIDI.
pub struct GuideInstrument {
    engine: Option<GuideEngine>,
    sample_rate: f64,
    gate: Arc<CueGate>,
    role: Role,
    library: Library,
}

impl GuideInstrument {
    #[must_use]
    pub fn new() -> Self {
        Self::for_role(
            Role::Any,
            Arc::new(CueGate::default()),
            Library::Folder(samples_dir()),
        )
    }

    /// An instrument for one guide track, sharing `gate` with the others.
    #[must_use]
    pub fn for_role(role: Role, gate: Arc<CueGate>, library: Library) -> Self {
        Self {
            engine: None,
            sample_rate: 48_000.0,
            gate,
            role,
            library,
        }
    }
}

impl Default for GuideInstrument {
    fn default() -> Self {
        Self::new()
    }
}

/// The count's and the guide's level: 8 dB under unity.
const VOICE_GAIN: f32 = 0.398;

impl PluginInstance for GuideInstrument {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: IDENT.to_owned(),
            name: "FTS Guide".to_owned(),
            vendor: "FastTrackStudio".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        Vec::new()
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }

    fn prepare(&mut self, sample_rate: f64, _block_size: u32) -> Result<(), PluginError> {
        // MIDI mode: play only the notes handed in, never a grid of its
        // own — the track's notes ARE the click.
        let config = GuideConfig {
            source: TriggerSource::Midi,
            enable_count: true,
            enable_guide: true,
            // The spoken count and the cues a little under the click (which
            // stays at unity): at unity the words came out on top of it.
            count_gain: VOICE_GAIN,
            guide_gain: VOICE_GAIN,
            ..GuideConfig::default()
        };
        let mut engine = GuideEngine::new(config);
        // The bank loads at an integer device rate.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a sample rate: a small positive integer carried as f64"
        )]
        let rate = sample_rate.round() as u32;
        self.library.load_into(engine.bank_mut(), rate);
        self.engine = Some(engine);
        self.sample_rate = sample_rate;
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.engine.is_some()
    }

    fn process_block(
        &mut self,
        _in_l: &[f32],
        _in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        out_l.fill(0.0);
        out_r.fill(0.0);
        let Some(engine) = self.engine.as_mut() else {
            return Ok(());
        };
        let cycle = daw_standalone::plugin::render_cycle();
        // The Guide track's instrument says it is being heard.
        if self.role == Role::Guide
            && !daw_standalone::plugin::track_muted()
            && let Some(cycle) = cycle
        {
            self.gate.guide_playing_in(cycle);
        }
        let guide_playing = cycle.is_some_and(|c| self.gate.guide_is_playing(c));
        for event in events.midi {
            // Note-on with velocity: status 0x9n, velocity > 0.
            let Some((status, key, velocity)) = event.message.to_raw_bytes() else {
                continue;
            };
            if status & 0xF0 != 0x90 || velocity == 0 {
                continue;
            }
            let Some(trigger) = session_guide::midi::trigger_for_midi_note(key) else {
                continue;
            };
            let under_cue = velocity == session_guide::midi::COUNT_UNDER_CUE_VELOCITY;
            if matches!(trigger, session_guide::GuideTrigger::Count(_))
                && under_cue
                && guide_playing
            {
                continue; // the cue says it instead
            }
            engine.trigger(event.offset as usize, trigger);
        }
        // MIDI mode ignores the transport; the rate is all it reads.
        let clock = BlockClock {
            playing: true,
            pos_seconds: 0.0,
            pos_beats: 0.0,
            tempo_bpm: 120.0,
            time_sig_num: 4,
            time_sig_den: 4,
            sample_rate: self.sample_rate,
        };
        engine.render_stereo(out_l, out_r, &clock);
        Ok(())
    }

    fn deactivate(&mut self) {
        self.engine = None;
    }
}

/// Makes guide instruments for `Effects::add(.., "fts.guide")`, all sharing
/// one [`CueGate`] and one [`Library`].
pub struct GuideFxFactory {
    gate: Arc<CueGate>,
    library: Library,
}

impl FxFactory for GuideFxFactory {
    fn installed(&self) -> Vec<daw_proto::fx::InstalledFx> {
        Vec::new()
    }

    /// Cheaply, without loading the samples: a saved session names the FX
    /// (`fts.guide:click`, …) and the loader asks before it adds one.
    fn provides(&self, name: &str) -> bool {
        Role::of(name).is_some()
    }

    fn create(&self, name_or_ident: &str, sample_rate: f64) -> Option<Box<dyn PluginInstance>> {
        let role = Role::of(name_or_ident)?;
        // Prepared here, on the thread adding the FX — loading ~20 MB of
        // samples. The renderer only prepares a plugin that is not
        // prepared yet, and it does that on the AUDIO thread: left to it,
        // the first block after pressing play took 40-60 ms, a dropout.
        let mut guide = GuideInstrument::for_role(role, self.gate.clone(), self.library.clone());
        guide.prepare(sample_rate, 512).ok()?;
        Some(Box::new(guide))
    }
}

/// Install the engine's FX factory on `daw`: the guide's instruments,
/// playing from `library`, and (desktop) the FTS processors built in.
/// Before the guide tracks are made: the factory prepares each instrument
/// as its FX is added.
pub fn install(daw: &daw_standalone::Standalone, library: Library) {
    let guide: Arc<dyn FxFactory> = Arc::new(GuideFxFactory {
        gate: Arc::default(),
        library,
    });
    // The FTS processors run built in beside it (`builtin_fx`): an FX
    // named `FTS Comp` — or a project's `CLAP: FTS Comp (FastTrackStudios)`
    // — is the engine's own, no plugin file loaded.
    // And signal's rigs, with the `signal` feature: the bass rig's NAM
    // amp and cab, the drum kit's sampled voices.
    #[cfg(feature = "native")]
    let factory: Arc<dyn FxFactory> = Arc::new(builtin_fx::Chain(vec![
        Arc::new(builtin_fx::BuiltinFx),
        #[cfg(feature = "signal")]
        Arc::new(signal_fx::SignalFx),
        guide,
    ]));
    #[cfg(not(feature = "native"))]
    let factory = guide;
    daw.set_fx_factory(factory);
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// 0.2 s of a stereo tone, as Ogg — longer than any synthesized
    /// fallback, so sound late in a block is the fetched sample's.
    fn ogg_tone(hz: f32) -> Vec<u8> {
        let frames = RATE as usize / 5;
        let mut pcm = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let v = (i as f32 / RATE as f32 * hz * std::f32::consts::TAU).sin() * 0.5;
            pcm.push(v);
            pcm.push(v);
        }
        fts_sample::cache::encode_ogg_vorbis(&pcm, 2, RATE, 0.6).expect("encode")
    }

    /// Energy 100–200 ms after a note on `key` — after the synthesized
    /// defaults (at most ~110 ms) have died away.
    fn late_energy(guide: &mut GuideInstrument, key: u8) -> f32 {
        let (event, _) = daw_proto::MidiEvent::decode(&[0x90, key, 100]).expect("note on");
        let midi = [daw_standalone::plugin::PluginMidiEvent {
            offset: 0,
            message: event,
        }];
        let block = RATE as usize / 5;
        let (mut l, mut r) = (vec![0.0; block], vec![0.0; block]);
        guide
            .process_block(
                &[],
                &[],
                &mut l,
                &mut r,
                &PluginEvents {
                    params: &[],
                    midi: &midi,
                    note_expressions: &[],
                },
            )
            .expect("process");
        l[block / 2..].iter().map(|s| s * s).sum()
    }

    /// The browser's library — the Ogg files `library::files` names, as
    /// fetched — plays the click, the count and the cues from MIDI.
    #[test]
    fn a_fetched_ogg_library_plays_click_count_and_cue() {
        let files: HashMap<String, Vec<u8>> =
            session_guide::samples::library::files(CLICK, VOICE, "ogg")
                .into_iter()
                .map(|path| (path, ogg_tone(440.0)))
                .collect();
        let library = Library::Files {
            files: Arc::new(files),
            ext: "ogg",
        };
        for (key, what) in [(61, "a click"), (72, "count 1"), (85, "the chorus cue")] {
            let mut guide = GuideInstrument::for_role(Role::Any, Arc::default(), library.clone());
            guide.prepare(f64::from(RATE), 512).expect("prepare");
            let energy = late_energy(&mut guide, key);
            assert!(energy > 1.0, "{what} plays its fetched sample: {energy}");
        }

        // With nothing fetched, the same notes fall back to the short
        // synthesized sounds — silent by the second half of the block.
        let empty = Library::Files {
            files: Arc::default(),
            ext: "ogg",
        };
        let mut guide = GuideInstrument::for_role(Role::Any, Arc::default(), empty);
        guide.prepare(f64::from(RATE), 512).expect("prepare");
        assert!(
            late_energy(&mut guide, 61) < 1e-3,
            "a synthesized tick is short"
        );
    }
}
