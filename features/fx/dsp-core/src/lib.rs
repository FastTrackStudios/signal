//! Shared primitives for the DSP cores.
//!
//! The fx DSP crates each reinvented the same two things, and each reinvention
//! is a place the workspace lint policy has something true to say:
//!
//! - **Numeric conversion.** Every crate had its own scattering of `as`, and
//!   `clippy::as_conversions` is right that a silent conversion in DSP code is
//!   how an index wraps or a length loses precision. [`num`] is the one place
//!   a conversion may happen, with the precondition written down.
//! - **Per-channel state.** Every crate held `[f32; MAX_CHANNELS]` arrays and
//!   clamped the channel index at each entry point, which is both repetitive
//!   and exactly the pattern `clippy::indexing_slicing` cannot verify.
//!   [`channel`] makes an out-of-range channel unconstructible instead, so the
//!   clamp happens once and the accesses are total.
//!
//! `no_std`, no dependencies, no allocation: these must work on native, in an
//! `AudioWorklet`, and on an embedded target alike.

#![no_std]

pub mod channel;
pub mod num;

pub use channel::{Channel, PerChannel};
pub use num::{count_to_f32, count_to_f64, count_to_i32, i64_to_f64, u64_to_f64, f32_to_index, f64_to_index, floor_f64, floor_to_i32, i32_to_f32, narrow, trunc_to_i32, trunc_to_i64, u32_to_f32, u32_to_index};
