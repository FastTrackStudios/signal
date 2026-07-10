//! Live, lock-free tuning parameters for the City Grand voice.
//!
//! A global store of atomics the audio thread (`native::modal`) reads each
//! block and a UI (the keys TUI) writes on mouse drag — no allocation, no
//! locks, so it's realtime-safe. Personal research tooling; a global static is
//! the pragmatic path (the proper route would be host param automation through
//! `PluginEvents.params`, which is far more plumbing).

use std::sync::atomic::{AtomicU32, Ordering};

/// One tunable float parameter with a display range.
pub struct Param {
    bits: AtomicU32,
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub unit: &'static str,
}

impl Param {
    const fn new(name: &'static str, default: f32, min: f32, max: f32, unit: &'static str) -> Self {
        Self {
            bits: AtomicU32::new(default.to_bits()),
            name,
            min,
            max,
            unit,
        }
    }
    pub fn get(&self) -> f32 {
        f32::from_bits(self.bits.load(Ordering::Relaxed))
    }
    pub fn set(&self, v: f32) {
        self.bits
            .store(v.clamp(self.min, self.max).to_bits(), Ordering::Relaxed);
    }
    /// Value as 0..1 across [min, max] (for slider position).
    pub fn norm(&self) -> f32 {
        ((self.get() - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
    }
    /// Set from a 0..1 slider position.
    pub fn set_norm(&self, n: f32) {
        self.set(self.min + n.clamp(0.0, 1.0) * (self.max - self.min));
    }
}

pub struct Params {
    pub symp_mix: Param,
    pub symp_t60: Param,
    pub residual: Param,
}

/// The live City Grand tuning store. Defaults match the voice's built-in
/// values; `$CITY_GRAND_*` env vars still seed them at startup (see `init`).
pub static PARAMS: Params = Params {
    symp_mix: Param::new("Sympathetic", 0.4, 0.0, 1.5, ""),
    symp_t60: Param::new("Symp Ring", 2.5, 0.3, 8.0, "s"),
    residual: Param::new("Noise Body", 0.0, 0.0, 1.0, ""),
};

/// All tunables in display order (for the UI).
pub fn all() -> [&'static Param; 3] {
    [&PARAMS.symp_mix, &PARAMS.symp_t60, &PARAMS.residual]
}

/// Seed the live params from `$CITY_GRAND_*` env vars if present (so the CLI
/// knobs still work as initial values). Call once at startup.
pub fn init_from_env() {
    if let Ok(v) = std::env::var("CITY_GRAND_SYMPATHETIC").map(|s| s.parse::<f32>()) {
        if let Ok(v) = v {
            PARAMS.symp_mix.set(v);
        }
    }
    if let Ok(v) = std::env::var("CITY_GRAND_SYMP_T60").map(|s| s.parse::<f32>()) {
        if let Ok(v) = v {
            PARAMS.symp_t60.set(v);
        }
    }
    if let Ok(v) = std::env::var("CITY_GRAND_RESIDUAL").map(|s| s.parse::<f32>()) {
        if let Ok(v) = v {
            PARAMS.residual.set(v);
        }
    }
}
