//! Comprehensive macro system for real-time parameter automation.
//!
//! This module provides a complete macro implementation for Signal-Live, enabling
//! smooth, real-time control of FX parameters through a hierarchical knob system
//! with support for sub-macros, curves, envelopes, and recording/playback.
//!
//! # Architecture
//!
//! The macro system is built on several core components:
//!
//! 1. **Macro Registry** (`macro_registry`)
//!    - Global thread-safe store of macro→FX parameter bindings
//!    - Maps knob IDs to concrete parameter targets
//!    - Lifecycle management: cleared on patch/preset changes
//!
//! 2. **Macro Setup** (`macro_setup`)
//!    - Resolves abstract macro bindings to concrete FX parameter indices
//!    - Recursively processes knob hierarchies (sub-macros)
//!    - Integrates with DAW-specific parameter APIs
//!
//! 3. **Macro Recorder** (`macro_recorder`)
//!    - Real-time capture of knob movements with timestamps
//!    - Enables performance automation recording & playback
//!    - Lock-free reads via Arc<Mutex> for thread safety
//!
//! # Data Flow
//!
//! ```text
//! Block Load
//!   ↓
//! setup_macros_for_block()     [macro_setup]
//!   ├─ Collect bindings from MacroBank.knobs (including children)
//!   ├─ Resolve param names to concrete indices
//!   └─ Return MacroSetupResult
//!     ↓
//! macro_registry::register()    [macro_registry]
//!   └─ Store knob→param mappings globally
//!
//! Performance (Real-Time)
//!   ↓
//! on_macro_change(knob_id, value)  [fts-control-desktop]
//!   ├─ Record if recording active  [macro_recorder]
//!   ├─ Get targets from registry
//!   └─ Set FX parameters in parallel (join_all)
//!
//! Patch Change
//!   ↓
//! macro_registry::clear()           [fts-control-desktop]
//!   └─ Remove stale bindings from previous patch
//! ```
//!
//! # Thread Safety
//!
//! - **Registry**: `LazyLock<RwLock<HashMap>>` for lock-free reads, atomic writes
//! - **Recorder**: `Arc<Mutex>` allows async spawn without lifetime issues
//! - **All parameter updates**: Async/await with `join_all()` for parallel execution
//!
//! # Example Usage
//!
//! ```ignore
//! // 1. Load a block with a MacroBank onto a track
//! let result = setup_macros_for_block(&track, &fx, &block).await?;
//!
//! // 2. Register bindings globally
//! if let Some(ref setup) = result {
//!     macro_registry::register(setup);
//! }
//!
//! // 3. Respond to knob changes (from UI)
//! on_macro_change("drive".into(), 0.75);
//!   ├─ Looks up targets: [param_idx=5, min=0.0, max=1.0, ...]
//!   ├─ Maps value: param_val = 0.0 + (1.0 - 0.0) * 0.75 = 0.75
//!   └─ Sets: fx.param(5).set(0.75).await
//!
//! // 4. On patch change
//! macro_registry::clear();
//! ```
//!
//! # Limitations & Future Work
//!
//! - **No debouncing**: Each knob change triggers an RPC call. Consider adding
//!   batching or throttling for high-frequency updates (e.g., LFO modulation).
//! - **No serialization**: Recorded sequences are ephemeral (not persisted).
//!   Consider `bincode` or `serde_json` for save/load.
//! - **No automation curves**: Knob values map linearly through [min, max].
//!   Could extend to support response curves from macromod.
//! - **No MIDI learn**: Currently manual binding definition. Could add UI
//!   to detect MIDI CC changes and auto-bind parameters.

pub use crate::macro_registry::MacroParamTarget;
pub use crate::macro_setup::{LiveMacroBinding, MacroSetupResult};
pub use crate::macro_recorder::{MacroRecord, MacroRecorder};
