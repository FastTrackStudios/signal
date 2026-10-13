//! DAW Global Signals
//!
//! Global Dioxus signals for DAW state that UI components can read and
//! subscribe to. These are populated by the FX browser hook and polled
//! by DAW panels (mixer, TCP, arrangement).

use crate::prelude::*;

// ── FX Browser Signals ──────────────────────────────────────────────

/// Whether a DAW connection is available for FX browsing
pub static FX_DAW_CONNECTED: GlobalSignal<bool> = Signal::global(|| false);

/// Tracks available in the current project
pub static FX_TRACKS: GlobalSignal<Vec<daw_proto::Track>> = Signal::global(Vec::new);

/// Currently selected track GUID for FX browsing
pub static FX_SELECTED_TRACK: GlobalSignal<Option<String>> = Signal::global(|| None);

/// FX chain for the selected track
pub static FX_CHAIN: GlobalSignal<Vec<daw_proto::Fx>> = Signal::global(Vec::new);

/// Currently selected FX GUID
pub static FX_SELECTED_FX: GlobalSignal<Option<String>> = Signal::global(|| None);

/// Parameters for the selected FX (updated live via FxEvent subscription)
pub static FX_PARAMETERS: GlobalSignal<Vec<daw_proto::FxParameter>> = Signal::global(Vec::new);

/// Whether we're currently loading FX data
pub static FX_LOADING: GlobalSignal<bool> = Signal::global(|| false);
