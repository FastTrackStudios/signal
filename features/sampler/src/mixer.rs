//! FX chains for a preset ([`FxChain`] of hosted, NAM or native [`FxSlot`]s
//! — a preset's master chain) and the mic naming rule a drum kit's tracks
//! follow ([`mic_is_bus`]).
//!
//! A drum kit's mixing — close mics direct, overhead / room mics sent to
//! shared buses, faders, mutes, solos and meters — is the daw's: the kit
//! loads as tracks (`SamplerRig::load_kit_tracks`).

use signal_plugin_host::HostedPlugin;

use crate::convolver::Convolver;
#[cfg(not(target_arch = "wasm32"))]
use crate::nam::NamProcessor;

/// Max block size (in frames) we prepare hosted plugins for at install time.
///
/// The cpal callback's actual block is normally 256–1024 frames; preparing
/// for 8192 keeps us safe against larger upstream buffers + future variable-
/// block-size paths without re-preparing per block.
///
/// On iOS, exactly what the RemoteIO backend hands the rig at most
/// (`audio::io::duplex_ios::MAX_PROCESS_FRAMES`, a bigger callback run in
/// pieces of it): every block is sized by this, a neural amp model's every
/// layer with it, and at 8192 the phone's eight resident patches took
/// 2.9 GB — a patch load building eight more was the app killed for memory.
#[cfg(not(target_os = "ios"))]
pub const FX_PREPARE_BLOCK: u32 = 8192;
#[cfg(target_os = "ios")]
pub const FX_PREPARE_BLOCK: u32 = audio::io::duplex_ios::MAX_PROCESS_FRAMES as u32;

// ── FX chain (REAPER-style track FX) ────────────────────────────────────────

/// One slot's DSP backend — what's actually filling the slot. Maps 1:1 to
/// [`signal_proto::block_kind::BlockKind`]'s non-Native variants.
///
/// Adding a new backend (e.g. built-in convolution reverb) is a new variant here +
/// a matching `BlockKind` arm.
pub enum FxBackend {
    /// Third-party CLAP / VST3 plugin loaded via `signal-plugin-host`.
    Hosted(HostedPlugin),
    /// Built-in Neural Amp Modeler — works for any block role that wants
    /// neural-net amp/pedal modeling (Amp, Drive, Cabinet, …).
    /// Native-only: the NAM C++ core doesn't build for wasm32.
    #[cfg(not(target_arch = "wasm32"))]
    Nam(NamProcessor),
    /// Built-in cabinet impulse-response convolution (a `BlockKind::Native`
    /// realization of a Cabinet block). Mono FIR; see [`crate::convolver`].
    CabIr(Convolver),
}

impl std::fmt::Debug for FxBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hosted(h) => f.debug_tuple("Hosted").field(h).finish(),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Nam(n) => f.debug_tuple("Nam").field(n).finish(),
            Self::CabIr(c) => f.debug_tuple("CabIr").field(c).finish(),
        }
    }
}

impl FxBackend {
    /// Short tag for UI labels / log lines — matches `BlockKind::tag()`.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Hosted(_) => "plugin",
            #[cfg(not(target_arch = "wasm32"))]
            Self::Nam(_) => "nam",
            Self::CabIr(_) => "cabir",
        }
    }

    /// Cached display name (plugin descriptor, NAM filename, or IR filename).
    pub fn display_name(&self) -> &str {
        match self {
            Self::Hosted(h) => &h.descriptor().name,
            #[cfg(not(target_arch = "wasm32"))]
            Self::Nam(n) => &n.display_name,
            Self::CabIr(c) => &c.display_name,
        }
    }

    /// Render one interleaved-stereo block in place. Each backend handles
    /// its own buffer-shape conversion (NAM does stereo↔mono internally).
    pub fn process_interleaved(&mut self, inout: &mut [f32]) {
        match self {
            Self::Hosted(h) => {
                let _ = h.process_interleaved(inout, &[], &[]);
            }
            #[cfg(not(target_arch = "wasm32"))]
            Self::Nam(n) => n.process_interleaved(inout),
            Self::CabIr(c) => c.process_interleaved(inout),
        }
    }

    /// Release any audio-thread resources before the slot is dropped.
    pub fn deactivate(&mut self) {
        if let Self::Hosted(h) = self {
            h.deactivate();
        }
        // NamProcessor releases its model in Drop.
    }
}

/// One slot in a track-style FX chain — a [`FxBackend`] instance plus a
/// per-slot bypass and a cached display name for the UI. The backend is
/// owned by the slot and deactivated when the slot is removed.
pub struct FxSlot {
    pub backend: FxBackend,
    pub bypassed: bool,
    /// Cached display name — read once at install time so the UI doesn't
    /// have to lock the audio thread to label the slot.
    pub display_name: String,
}

impl std::fmt::Debug for FxSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FxSlot")
            .field("display_name", &self.display_name)
            .field("bypassed", &self.bypassed)
            .field("backend", &self.backend)
            .finish()
    }
}

/// A serial FX chain that processes its slots in order, in place on an
/// interleaved-stereo buffer. Modeled after a REAPER track FX chain: each
/// slot is one [`FxBackend`], bypassable per-slot.
///
/// An empty chain is a no-op.
#[derive(Debug, Default)]
pub struct FxChain {
    pub slots: Vec<FxSlot>,
}

impl FxChain {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Process `inout` (interleaved stereo) through each non-bypassed slot
    /// in order. Errors / failures inside a backend are swallowed — the UI
    /// surface reports load/prepare failures separately; the audio thread
    /// must never panic on a misbehaving plugin.
    pub fn process(&mut self, inout: &mut [f32]) {
        for slot in &mut self.slots {
            if slot.bypassed {
                continue;
            }
            slot.backend.process_interleaved(inout);
        }
    }
}

/// A drum mic id routes to a shared **bus** (overhead / room) rather than
/// straight out a close-mic channel when its name looks like an overhead or
/// room/ambient mic. Everything else is a direct close mic.
#[must_use]
pub fn mic_is_bus(id: &str) -> bool {
    let l = id.to_ascii_lowercase();
    l == "oh"
        || l.contains("overhead")
        || l.starts_with("room")
        || l.contains("ambien")
        || l.contains("ambc")
}

/// UI snapshot of one FX-chain slot. `slot_idx` matches the index taken by
/// the preset master chain's slot setters.
#[derive(Clone, Debug)]
pub struct FxSlotStrip {
    pub slot_idx: usize,
    pub display_name: String,
    pub bypassed: bool,
}

impl FxChain {
    /// Snapshot the chain for the UI.
    #[must_use]
    pub fn strip(&self) -> Vec<FxSlotStrip> {
        self.slots
            .iter()
            .enumerate()
            .map(|(slot_idx, s)| FxSlotStrip {
                slot_idx,
                display_name: s.display_name.clone(),
                bypassed: s.bypassed,
            })
            .collect()
    }
}
