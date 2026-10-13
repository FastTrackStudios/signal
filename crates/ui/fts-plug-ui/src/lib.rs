//! Shared Dioxus editor chrome for the FTS plugin suite.
//!
//! `eq-ui`, `comp-ui` and `trigger-ui` each grew their own copy of the same
//! scaffolding — the nice_plug→[`fts_audio_ui`] param adapter, the ~30 Hz
//! repaint driver, the header, the labelled control sections. This crate is
//! that scaffolding, lifted, so a new plugin editor is just its parameter tree
//! plus its own visualizer.
//!
//! A minimal editor:
//!
//! ```ignore
//! #[component]
//! pub fn App() -> Element {
//!     rsx! {
//!         PluginApp { tailwind_css: include_str!("../assets/tailwind.css").to_string(),
//!             Shell {}
//!         }
//!     }
//! }
//!
//! #[component]
//! fn Shell() -> Element {
//!     let ui = use_context::<SharedState>().get::<MyUiState>().unwrap();
//!     let ctx = use_param_context();
//!     let skin = Skin::accented(accents::LIMITER);
//!     rsx! {
//!         PluginRoot { title: "FTS Limiter", subtitle: "Brickwall Limiter", skin,
//!             ControlSurface {
//!                 Section { label: "Gain", skin,
//!                     ParamKnob {
//!                         handle: param_handle(ui.params.input_gain.as_ptr(), ctx.clone()),
//!                         testid: "ingain",
//!                     }
//!                 }
//!             }
//!         }
//!     }
//! }
//! ```
//!
//! See [`chrome`]'s module docs for the two constraints every editor has to
//! respect: the redraw tick must reach the DOM, and the surface must *fit*
//! (Blitz collapses what overflows instead of scrolling it).

pub mod chrome;
pub mod feed;
pub mod param_adapter;
pub mod skin;

pub use param_adapter::{param_handle, param_handle_with_options};
pub use skin::{accents, Skin};

/// `use fts_plug_ui::prelude::*` — the chrome plus the widget kit it builds on.
pub mod prelude {
    pub use crate::chrome::{
        use_redraw_tick, ControlSurface, ParamKnob, ParamSelector, ParamToggle, PluginApp,
        PluginRoot, Section, BASE_CSS,
    };
    pub use crate::feed::{IoGrMeters, PeakMeter, WaveRing, WAVE_HISTORY_LEN};
    pub use crate::param_adapter::{param_handle, param_handle_with_options};
    pub use crate::skin::{accents, Skin};
    pub use fts_audio_ui::prelude::*;
}
