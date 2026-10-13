//! Frame surfaces embedded in a host's strip.
//!
//! A mixer channel's Tone rack, an FX slot expanded in place, REAPER's TCP
//! and MCP inline UI: each is a frame surface drawn inside a box its host
//! owns. This crate is the host-facing half:
//!
//! - [`form`]: the `Embed { host: Strip | Tcp | Mcp }` form factor, and the
//!   width tiers (session's `Rack::at`) every embed sheds detail by.
//! - [`Embed`]: one surface in a strip — resize (reflow, not letterbox),
//!   two paint passes (params, recorded once; feeds and motion, live),
//!   pointer routing in strip-local coordinates.
//! - [`Canvas`]: what it paints into — the host's own scene. An anyrender
//!   0.11 `PaintScene` (feature `anyrender-0-11`, session's mixer), frame's
//!   replayable [`Recording`], or the CPU rasteriser ([`cpu`], REAPER's
//!   LICE bitmap, tests).
//! - [`tone`]: session's Tone rack (EQ, gate, compressor, saturator,
//!   de-esser, delay, reverb) as frame documents, at session's widths.
//! - [`generic`]: an embed for any catalog, from its featured params.
//! - [`curves`]: the rack's curve providers, which a host overrides with
//!   the plugins' own maths.
//! - [`fx_slot`]: the provider shape for daw-ui's embedded-FX slot seam.
//! - `nice_plug` (feature `nice-plug`): any embed as a nice-plug
//!   `EmbeddedEditor` — REAPER's `REAPER_FXEMBED` protocol.
//!
//! # Hosting one in a strip
//!
//! ```ignore
//! let mut rack = Embed::new(ToneRack::channel(), EmbedHost::Strip);
//! rack.set_size(strip_w, rack_h);
//! // Once, and again whenever `rack.needs_record()`:
//! rack.record(&mut AnyRender(&mut strip_scene), Affine::IDENTITY);
//! // Every frame, over the replayed recording, while `rack.animating()`:
//! rack.live(&mut AnyRender(&mut live_scene), strip_transform, now_ms);
//! // Pointer, in strip-local px:
//! let out = rack.pointer(Pointer::Down { x, y, button: Button::Left, mods }, now_ms);
//! engine.send(out.calls);
//! ```

#[cfg(feature = "cpu")]
pub mod cpu;
pub mod curves;
pub mod embed;
pub mod form;
pub mod fx_slot;
pub mod generic;
#[cfg(feature = "nice-plug")]
pub mod nice_plug;
pub mod tone;

// The painting seam, shared with the theme runtime.
pub use render_canvas::{canvas, paint, text};

#[cfg(feature = "anyrender-0-11")]
pub use canvas::AnyRender;
pub use canvas::{Canvas, CanvasExt, Recording, Tally};
pub use embed::{
    BuildCtx, Built, Button, Derived, DocumentView, Embed, EmbedView, Handled, Pointer, Values,
};
pub use form::{EmbedForm, EmbedHost, SizeHints, Tier};
pub use generic::FeaturedView;
pub use tone::{TonePanel, ToneRack, Which};

pub use surface_runtime::{EngineCall, Modifiers};
