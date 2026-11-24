//! Parameters. The model (a spec in plain units that parses what it
//! prints) is the plugins' (`plugin_processors::param`), so a block's
//! parameters and the instance's are one kind of thing in every host.
//! This module adds the instance's own.

pub use plugin_processors::param::{ParamSpec, Unit, ValueText};

/// How many macro knobs an instance has.
pub const MACROS: usize = 8;

/// The instance's own parameters, the same in every host: what the CLAP
/// shows a DAW and what Studio's engine automates. Fixed, so automation
/// survives a preset change; what a macro moves is the preset's
/// ([`crate::state::MacroSpec`]).
pub mod host {
    use super::{ParamSpec, Unit};

    pub const INPUT_GAIN: ParamSpec =
        ParamSpec::new("in_gain", "Input Gain", Unit::Db, -60.0, 24.0, 0.0);
    pub const OUTPUT_GAIN: ParamSpec =
        ParamSpec::new("out_gain", "Output Gain", Unit::Db, -60.0, 24.0, 0.0);
    pub const BYPASS: ParamSpec = ParamSpec::new("bypass", "Bypass", Unit::Toggle, 0.0, 1.0, 0.0);

    /// Macro `index` (0-based): 0–1, shown as a percentage.
    #[must_use]
    pub const fn macro_spec(index: usize) -> ParamSpec {
        const IDS: [&str; super::MACROS] = [
            "macro_1", "macro_2", "macro_3", "macro_4", "macro_5", "macro_6", "macro_7", "macro_8",
        ];
        const NAMES: [&str; super::MACROS] = [
            "Macro 1", "Macro 2", "Macro 3", "Macro 4", "Macro 5", "Macro 6", "Macro 7", "Macro 8",
        ];
        let i = if index < super::MACROS {
            index
        } else {
            super::MACROS - 1
        };
        ParamSpec::new(IDS[i], NAMES[i], Unit::Percent, 0.0, 1.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_instance_params_parse_what_they_print() {
        let specs: alloc::vec::Vec<ParamSpec> = [host::INPUT_GAIN, host::OUTPUT_GAIN, host::BYPASS]
            .into_iter()
            .chain((0..MACROS).map(host::macro_spec))
            .collect();
        for spec in &specs {
            for step in 0..=20u8 {
                let v = spec.denormalize(f32::from(step) / 20.0);
                let text = ValueText::of(spec, v);
                let again = spec.parse(text.as_str()).map(|b| ValueText::of(spec, b));
                assert_eq!(
                    again.as_ref().map(ValueText::as_str),
                    Some(text.as_str()),
                    "{}",
                    spec.id
                );
            }
        }
        assert_eq!(host::INPUT_GAIN.parse("-3 dB"), Some(-3.0));
    }
}
