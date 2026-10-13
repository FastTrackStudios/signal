//! A host's own views — a mixer strip, a track row, an item on a lane —
//! drawn from frame documents instead of hand-written painters, so the
//! look is data: swapped at runtime, edited while the app runs.
//!
//! # A theme
//!
//! A frame document. Each page is a **role** the host draws (`mcp.strip`,
//! `tcp.row`); the page's first frame is what one instance looks like at
//! the size it was drawn. Inside it:
//!
//! - **Tokens** — the document's [`Variables`](crate::document::proto::Variables):
//!   colours and numbers per mode, read in bindings as `$name`.
//! - **Bindings** — `bind:fill="mix($strip.ground, track.color, 0.25)"`,
//!   `bind:visible="w >= 60"`, `bind:y="(1 - track.fader) * (ph - 14)"`:
//!   properties computed from tokens, the box and the host's data
//!   ([`theme_expr`]).
//! - **Responsive layout** — `resize-x` / `resize-y` constraints, auto
//!   layout (`layout="column"`, `grow`), and bindings on `x y w h
//!   visible` over the box. A strip drawn at 86×400 is laid out again at
//!   54×300 or 300×900 by the same rules.
//! - **Hit targets** — a node named `@mute` is the `mute` control: the
//!   host asks [`Theme::hit`] what is under the pointer and gets `mute`.
//! - **Host slots** — a node named `%icon` is a place the host draws into
//!   itself (an icon it holds, a curve it computes): [`Theme::slots`]
//!   says where each shown one is, after the theme has laid it out.
//! - **Components** — a control drawn once (`<Component name="button"
//!   arg:on="false">`, its parts reading `arg.on`) and placed by name
//!   (`<Instance name="@mute" of="button" arg:on="track.mute">`). Placed
//!   when the theme compiles, so a component costs what drawing its parts
//!   by hand would.
//!
//! # Layers
//!
//! A theme can be several documents, each over the ones before it
//! ([`Theme::from_sources`], [`layer`]): pages replace pages by name,
//! components replace components by name wherever they are defined,
//! tokens replace tokens. A base theme is its files; a theme on it is the
//! files it changes — one control, one view, one colour.
//!
//! # What the host gives
//!
//! A [`Schema`] — the fields it can bind (`track.name`, `track.volume`)
//! and which of them are **live** (change every frame: a meter, a fader
//! being dragged) — and, per instance, the values ([`Fields`]).
//!
//! # Cost
//!
//! The work is split by how often its inputs change:
//!
//! | when | what | |
//! |---|---|---|
//! | theme load, mode switch | [`Theme::compile`] | tokens inlined, field names resolved, constants folded and applied |
//! | a new size | layout | constraints, auto layout and size bindings solved; everything that reads no host data recorded once |
//! | an instance's static data changes | [`Theme::record`] | the recorded picture replayed, plus the nodes bound to static data painted with this instance's values |
//! | every frame | [`Theme::live`] | the nodes bound to live data |
//!
//! Which pass a node is painted in falls out of what its bindings read: a
//! node reading `meter.peak` is live because the host said `meter.peak`
//! is; one reading `track.name` is recorded; one reading only `w` is baked
//! into the layout.

mod compile;
mod draw;
mod expand;
mod layer;
mod layout;
mod plan;
mod schema;

pub use compile::{Diagnostic, Theme};
pub use draw::Fields;
pub use layer::layer;
pub use schema::Schema;

pub use crate::render::canvas::{Canvas, Font, Recording, TextAlign, TextRun};
pub use crate::theme::expr::{Rgba, Value};

#[cfg(feature = "anyrender-0-11")]
pub use crate::render::canvas::AnyRender;
