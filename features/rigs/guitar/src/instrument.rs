//! Which instrument the rig is: the guitar or the bass. One engine, one
//! chain, one UI; what differs is the library it plays from (its own
//! directory, its own shipped defaults), the interface calibration it keeps,
//! its MIDI learning, and how low its tuner listens.
//!
//! Process-wide: only one rig owns the audio interface at a time, so the
//! rig switches instrument rather than two running side by side
//! ([`crate::GuitarRigBackend::switch_instrument`]).

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Instrument {
    #[default]
    Guitar,
    Bass,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

impl Instrument {
    pub const ALL: [Self; 2] = [Self::Guitar, Self::Bass];

    /// The wire name (`guitar`, `bass`).
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Guitar => "guitar",
            Self::Bass => "bass",
        }
    }

    /// From its wire name; anything else is the guitar.
    #[must_use]
    pub fn from_id(id: &str) -> Self {
        if id.trim().eq_ignore_ascii_case("bass") { Self::Bass } else { Self::Guitar }
    }

    /// The library's directory under the config root (`rig` stays the
    /// guitar's, as every existing library has it; `bass-rig` keeps clear of
    /// the retired bass rig's `bass/`).
    #[must_use]
    pub fn dir_name(self) -> &'static str {
        match self {
            Self::Guitar => "rig",
            Self::Bass => "bass-rig",
        }
    }

    /// Its audio rig's name: the interface calibration and routing it keeps
    /// (`rigs/<slug>.styx`).
    #[must_use]
    pub fn audio_rig_name(self) -> &'static str {
        match self {
            Self::Guitar => "Guitar Rig",
            Self::Bass => "Bass Rig",
        }
    }

    /// The lowest note its tuner listens for, Hz: a guitar's low E is 82,
    /// a five-string bass's low B 31.
    #[must_use]
    pub fn tuner_floor_hz(self) -> f32 {
        match self {
            Self::Guitar => 60.0,
            Self::Bass => 27.0,
        }
    }
}

/// The instrument the rig is now.
#[must_use]
pub fn current() -> Instrument {
    if CURRENT.load(Ordering::Relaxed) == 1 { Instrument::Bass } else { Instrument::Guitar }
}

/// Make the rig this instrument — before the library is read
/// (`SIGNAL_INSTRUMENT` at start, or a switch).
pub fn set(i: Instrument) {
    CURRENT.store(u8::from(i == Instrument::Bass), Ordering::Relaxed);
}

/// Where the last instrument played is kept, across launches.
fn memory() -> std::path::PathBuf {
    signal_rig_host::store::signal_config_dir().join("instrument")
}

/// At start: `SIGNAL_INSTRUMENT=bass` (the app's `--bass`) says which;
/// otherwise the instrument played last.
pub fn init() {
    let chosen = std::env::var("SIGNAL_INSTRUMENT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| std::fs::read_to_string(memory()).ok());
    if let Some(v) = chosen {
        set(Instrument::from_id(&v));
    }
}

/// Keep `i` as the instrument to open on next time (not in a run that
/// saves nothing).
pub fn remember(i: Instrument) {
    if crate::library::rig_is_ephemeral() {
        return;
    }
    let path = memory();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, i.id()) {
        tracing::warn!("instrument: cannot remember it: {e}");
    }
}
