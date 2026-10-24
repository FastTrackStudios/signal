//! Common macro configuration templates.
//!
//! Pre-built macro bank patterns for common use cases,
//! helping developers get started with the macro system quickly.
//!
//! # Example
//!
//! ```ignore
//! use signal_live::macro_templates;
//!
//! // Create a standard EQ macro bank
//! let eq_bank = macro_templates::eq_3band();
//!
//! // Create a compression macro bank
//! let comp_bank = macro_templates::compressor();
//!
//! // Use in block
//! block.macro_bank = Some(eq_bank);
//! ```

use signal_macromod::{MacroBank, MacroBinding, MacroKnob};

/// Simple 3-band EQ macro configuration.
///
/// Provides individual knobs for Low, Mid, and High frequencies with
/// typical frequency and gain ranges.
///
/// # Knobs
/// - `low`: 20 Hz to 500 Hz, ±12 dB
/// - `mid`: 500 Hz to 5 kHz, ±12 dB
/// - `high`: 5 kHz to 20 kHz, ±12 dB
#[must_use]
pub fn eq_3band() -> MacroBank {
    let mut bank = MacroBank::new();

    // Low frequency band
    let mut low = MacroKnob::new("eq_low", "Low");
    low.bindings.push(MacroBinding::from_ids(
        "eq", "low_freq", 0.0, // 20 Hz
        1.0, // 500 Hz
    ));
    low.bindings.push(MacroBinding::from_ids(
        "eq", "low_gain", 0.0, // -12 dB
        1.0, // +12 dB
    ));
    bank.add(low);

    // Mid frequency band
    let mut mid = MacroKnob::new("eq_mid", "Mid");
    mid.bindings.push(MacroBinding::from_ids(
        "eq", "mid_freq", 0.0, // 500 Hz
        1.0, // 5 kHz
    ));
    mid.bindings.push(MacroBinding::from_ids(
        "eq", "mid_gain", 0.0, // -12 dB
        1.0, // +12 dB
    ));
    bank.add(mid);

    // High frequency band
    let mut high = MacroKnob::new("eq_high", "High");
    high.bindings.push(MacroBinding::from_ids(
        "eq",
        "high_freq",
        0.0, // 5 kHz
        1.0, // 20 kHz
    ));
    high.bindings.push(MacroBinding::from_ids(
        "eq",
        "high_gain",
        0.0, // -12 dB
        1.0, // +12 dB
    ));
    bank.add(high);

    bank
}

/// Simple compressor macro configuration.
///
/// Provides individual knobs for threshold, ratio, and makeup gain.
/// Typical compressor parameter ranges.
///
/// # Knobs
/// - `threshold`: -60 dB to 0 dB
/// - `ratio`: 1:1 to 20:1
/// - `makeup`: 0 dB to 24 dB
#[must_use]
pub fn compressor() -> MacroBank {
    let mut bank = MacroBank::new();

    // Threshold
    let mut threshold = MacroKnob::new("comp_threshold", "Threshold");
    threshold.bindings.push(MacroBinding::from_ids(
        "compressor",
        "threshold",
        0.0, // -60 dB
        1.0, // 0 dB
    ));
    bank.add(threshold);

    // Ratio
    let mut ratio = MacroKnob::new("comp_ratio", "Ratio");
    ratio.bindings.push(MacroBinding::from_ids(
        "compressor",
        "ratio",
        0.0, // 1:1
        1.0, // 20:1
    ));
    bank.add(ratio);

    // Makeup Gain
    let mut makeup = MacroKnob::new("comp_makeup", "Makeup");
    makeup.bindings.push(MacroBinding::from_ids(
        "compressor",
        "makeup_gain",
        0.0, // 0 dB
        1.0, // 24 dB
    ));
    bank.add(makeup);

    bank
}

/// Simple reverb macro configuration.
///
/// Provides individual knobs for room size, decay, and wet/dry mix.
///
/// # Knobs
/// - `room`: Room size (small to large)
/// - `decay`: Decay time (short to long)
/// - `mix`: Wet/Dry mix (dry to wet)
#[must_use]
pub fn reverb() -> MacroBank {
    let mut bank = MacroBank::new();

    // Room Size
    let mut room = MacroKnob::new("reverb_room", "Room");
    room.bindings.push(MacroBinding::from_ids(
        "reverb",
        "room_size",
        0.0, // Small
        1.0, // Large
    ));
    bank.add(room);

    // Decay Time
    let mut decay = MacroKnob::new("reverb_decay", "Decay");
    decay.bindings.push(MacroBinding::from_ids(
        "reverb",
        "decay_time",
        0.0, // 0.1s
        1.0, // 5s
    ));
    bank.add(decay);

    // Wet/Dry Mix
    let mut mix = MacroKnob::new("reverb_mix", "Mix");
    mix.bindings.push(MacroBinding::from_ids(
        "reverb", "wet_dry", 0.0, // Dry
        1.0, // Wet
    ));
    bank.add(mix);

    bank
}

/// Master volume macro.
///
/// Single knob controlling output level.
/// Useful for live mixing without touching individual fader.
///
/// # Knobs
/// - `level`: Output level (-∞ to 0 dB)
#[must_use]
pub fn master_level() -> MacroBank {
    let mut bank = MacroBank::new();

    let mut level = MacroKnob::new("master_level", "Level");
    level.bindings.push(MacroBinding::from_ids(
        "output", "level", 0.0, // Silent
        1.0, // Nominal
    ));
    bank.add(level);

    bank
}
