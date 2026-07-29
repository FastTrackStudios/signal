//! Heuristic one-shot classification — deterministic rules over the analyzer
//! scalars (the trigger-dsp multiband intuition, offline): kick lives in the
//! sub band, hats/cymbals in the top octave with noisy spectra, snares carry
//! a mid body plus broadband noise, toms are tonal low-mid. Filename hints
//! break ties (sample libraries almost always name their one-shots).
//!
//! An optional ONNX classifier (YAMNet-style) can replace this later; the
//! class vocabulary is the contract, not the rules.

use crate::analyze::Analysis;

pub const CLASSES: &[&str] = &[
    "kick", "snare", "clap", "hat-closed", "hat-open", "cymbal", "tom", "perc", "fx", "other",
];

/// Classify one analyzed asset. `name` = lowercase filename (hint source).
pub fn classify(a: &Analysis, name: &str) -> &'static str {
    // Filename hints first — they encode ground truth more reliably than
    // any acoustic rule when present.
    const HINTS: &[(&str, &str)] = &[
        ("kick", "kick"),
        ("bd_", "kick"),
        ("808", "kick"),
        ("snare", "snare"),
        ("snr", "snare"),
        ("rim", "snare"),
        ("clap", "clap"),
        ("hihat", "hat-closed"),
        ("hi-hat", "hat-closed"),
        ("hat", "hat-closed"),
        ("hh", "hat-closed"),
        ("open", "hat-open"),
        ("ride", "cymbal"),
        ("crash", "cymbal"),
        ("cym", "cymbal"),
        ("china", "cymbal"),
        ("stack", "cymbal"),
        ("splash", "cymbal"),
        ("tom", "tom"),
        ("perc", "perc"),
        ("shaker", "perc"),
        ("tamb", "perc"),
        ("conga", "perc"),
        ("bongo", "perc"),
        ("cow", "perc"),
        ("fx", "fx"),
        ("riser", "fx"),
        ("sweep", "fx"),
        ("impact", "fx"),
    ];
    let mut hint: Option<&'static str> = None;
    for (pat, class) in HINTS {
        if name.contains(pat) {
            hint = Some(class);
            break;
        }
    }
    // "open" only refines a hat hint.
    if hint == Some("hat-open") && !(name.contains("hat") || name.contains("hh")) {
        hint = None;
    }

    // Non-percussive / long content → not a drum one-shot. Cymbals are the
    // exception: they legitimately ring for many seconds.
    if a.duration_s > 4.0 || a.percussiveness < 0.25 {
        if hint == Some("cymbal") || (a.centroid_hz > 2000.0 && a.band_energy[2] > 0.4) {
            return "cymbal";
        }
        return hint.filter(|h| *h == "fx").unwrap_or("other");
    }
    if let Some(h) = hint {
        // Trust the filename unless it's acoustically absurd.
        let sane = match h {
            "kick" => a.band_energy[0] > 0.05,
            "hat-closed" | "hat-open" | "cymbal" => a.centroid_hz > 1200.0,
            _ => true,
        };
        if sane {
            // Acoustically split closed vs open hats by ring time.
            if h == "hat-closed" && a.decay_ms > 350.0 {
                return "hat-open";
            }
            return h;
        }
    }

    // Acoustic rules.
    let sub = a.band_energy[0];
    let body = a.band_energy[1];
    let top = a.band_energy[2];
    if sub > 0.5 && a.centroid_hz < 900.0 {
        return "kick";
    }
    if a.centroid_hz > 3000.0 && top > 0.5 {
        return if a.decay_ms > 400.0 { "cymbal" } else { "hat-closed" };
    }
    if a.centroid_hz > 1800.0 && a.flatness > 0.25 {
        return if a.decay_ms > 350.0 { "hat-open" } else { "hat-closed" };
    }
    if body > 0.35 && top > 0.15 && a.flatness > 0.08 {
        return if a.attack_ms > 8.0 { "clap" } else { "snare" };
    }
    if sub + body > 0.6 && a.flatness < 0.1 {
        return "tom";
    }
    "perc"
}
