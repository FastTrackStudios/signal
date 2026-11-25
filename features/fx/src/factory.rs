//! Built-in FX factory — `daw::plugin::FxFactory` over the `Native*`
//! wrappers, so the standalone daw's `Effects::add` can instantiate
//! signal's built-in FX by name on any session mixer track.
//!
//! Idents are the wrappers' descriptor ids (`signal.fx.eq`, …);
//! `create` also matches the human display name ("EQ", "Compressor",
//! …) case-insensitively so `Effects::add(project, chain, "Reverb")`
//! just works. Install with
//! `standalone.set_fx_factory(Arc::new(NativeFxFactory))`.

use daw::plugin::{FxFactory, PluginInstance};
use daw::service::fx::InstalledFx;

use crate::{
    NativeComp, NativeDelay, NativeEq, NativeGain, NativeGate, NativeLevel, NativeMod, NativePitch,
    NativePreamp, NativeReverb, NativeSaturate, NativeTransient, NativeTrem, NativeTune,
};

/// `(ident, display name, constructor)` — one row per built-in.
type Ctor = fn(f64) -> Box<dyn PluginInstance>;
const CATALOG: &[(&str, &str, Ctor)] = &[
    ("signal.fx.eq", "EQ", |sr| Box::new(NativeEq::new(sr))),
    ("signal.fx.comp", "Compressor", |sr| {
        Box::new(NativeComp::new(sr))
    }),
    ("signal.fx.level", "Leveler", |sr| {
        Box::new(NativeLevel::new(sr))
    }),
    ("signal.fx.gate", "Gate", |sr| Box::new(NativeGate::new(sr))),
    ("signal.fx.transient", "Transient", |sr| {
        Box::new(NativeTransient::new(sr))
    }),
    ("signal.fx.reverb", "Reverb", |sr| {
        Box::new(NativeReverb::new(sr))
    }),
    ("signal.fx.delay", "Delay", |sr| {
        Box::new(NativeDelay::new(sr))
    }),
    ("signal.fx.chorus", "Chorus", |sr| {
        Box::new(NativeMod::chorus(sr))
    }),
    ("signal.fx.flanger", "Flanger", |sr| {
        Box::new(NativeMod::flanger(sr))
    }),
    ("signal.fx.vibrato", "Vibrato", |sr| {
        Box::new(NativeMod::vibrato(sr))
    }),
    ("signal.fx.trem", "Tremolo", |sr| {
        Box::new(NativeTrem::new(sr))
    }),
    ("signal.fx.gain", "Gain", |sr| Box::new(NativeGain::new(sr))),
    ("signal.fx.preamp", "Preamp", |sr| {
        Box::new(NativePreamp::new(sr))
    }),
    ("signal.fx.saturate", "Saturate", |sr| {
        Box::new(NativeSaturate::new(sr))
    }),
    ("signal.fx.tune", "Tune", |sr| Box::new(NativeTune::new(sr))),
    ("signal.fx.pitch", "Pitch", |sr| {
        Box::new(NativePitch::new(sr))
    }),
];

/// The stock built-in FX factory. Stateless; construct freely.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeFxFactory;

impl FxFactory for NativeFxFactory {
    fn installed(&self) -> Vec<InstalledFx> {
        CATALOG
            .iter()
            .map(|(ident, name, _)| InstalledFx {
                name: (*name).to_string(),
                ident: (*ident).to_string(),
            })
            .collect()
    }

    fn create(&self, name_or_ident: &str, sample_rate: f64) -> Option<Box<dyn PluginInstance>> {
        let want = name_or_ident.trim();
        CATALOG
            .iter()
            .find(|(ident, name, _)| {
                ident.eq_ignore_ascii_case(want) || name.eq_ignore_ascii_case(want)
            })
            .map(|(_, _, ctor)| ctor(sample_rate.max(1.0)))
    }
}
