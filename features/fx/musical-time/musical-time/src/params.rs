//! The two parameters a synced time control needs.
//!
//! Every control that can be either free-running or tempo-locked wants the
//! same pair: a switch for the mode, and a stepped choice of note value. The
//! range, the labels and — the part everyone forgets — the *parser* are all
//! determined by [`MusicalTime`], so they are built here once. There are
//! already six of these across the delay (L and R), the reverb (pre-delay)
//! and the compressor (attack and release), and every copy is a chance for
//! one of them to end up with a different table.
//!
//! Behind the `params` feature so the core stays `no_std` and free of
//! nice-plug for the DSP crates that only want the arithmetic.

use std::sync::Arc;

use nice_plug::params::range::IntRange;
use nice_plug::params::{BoolParam, IntParam};

use crate::MusicalTime;

/// The mode switch: free-running milliseconds, or locked to the tempo.
///
/// A `BoolParam` rather than an enum because that is what it is — a host
/// shows it as a button, automates it as 0/1, and "Sync"/"Free" is the whole
/// vocabulary.
#[must_use]
pub fn sync_param(name: impl Into<String>, default_synced: bool) -> BoolParam {
    BoolParam::new(name, default_synced)
        .with_value_to_string(Arc::new(|v| if v { "Sync" } else { "Free" }.to_string()))
        .with_string_to_value(Arc::new(|s| {
            let s = s.trim();
            if s.eq_ignore_ascii_case("sync") || s.eq_ignore_ascii_case("on") || s == "1" {
                Some(true)
            } else if s.eq_ignore_ascii_case("free") || s.eq_ignore_ascii_case("off") || s == "0" {
                Some(false)
            } else {
                None
            }
        }))
}

/// The note-value picker, stored as an index into [`MusicalTime::ALL`].
///
/// Stepped, so hosts render it as a list rather than a slider someone has to
/// land exactly on — there are 21 entries and no meaning between them.
#[must_use]
pub fn division_param(name: impl Into<String>, default: MusicalTime) -> IntParam {
    IntParam::new(
        name,
        i32::try_from(default.index()).unwrap_or(0),
        IntRange::Linear {
            min: 0,
            max: i32::try_from(MusicalTime::COUNT - 1).unwrap_or(0),
        },
    )
    .with_value_to_string(Arc::new(|v| MusicalTime::from_param(v).label().to_string()))
    // Without this the picker is read-only in a host's parameter list: it
    // would show "1/8D" and refuse "1/8D" back.
    .with_string_to_value(Arc::new(|s| {
        let s = s.trim();
        MusicalTime::ALL
            .iter()
            .position(|t| t.label().eq_ignore_ascii_case(s))
            .and_then(|i| i32::try_from(i).ok())
    }))
}

#[cfg(test)]
mod tests {
    use nice_plug::prelude::Param;

    use super::*;
    use crate::{Flavour, NoteValue};

    /// Every label the picker can print has to come back as the same choice —
    /// this is the parameter a host's text field is editing.
    #[test]
    fn the_division_picker_parses_every_label_it_can_print() {
        let p = division_param("Div", MusicalTime::default());
        for (i, t) in MusicalTime::ALL.iter().enumerate() {
            let shown = p
                .normalized_value_to_string(p.preview_normalized(i32::try_from(i).unwrap()), false);
            assert_eq!(shown, t.label());

            let parsed = p
                .string_to_normalized_value(&shown)
                .unwrap_or_else(|| panic!("{shown} did not parse"));
            assert_eq!(
                p.preview_plain(parsed),
                i32::try_from(i).unwrap(),
                "{shown} parsed back as a different note"
            );
        }
    }

    #[test]
    fn the_picker_defaults_to_the_note_it_was_given() {
        let p = division_param("Div", MusicalTime::new(NoteValue::Eighth, Flavour::Dotted));
        assert_eq!(
            p.normalized_value_to_string(p.default_normalized_value(), false),
            "1/8D"
        );
    }

    #[test]
    fn the_mode_switch_reads_both_spellings() {
        let p = sync_param("Sync", false);
        assert_eq!(p.normalized_value_to_string(0.0, false), "Free");
        assert_eq!(p.normalized_value_to_string(1.0, false), "Sync");
        assert_eq!(p.string_to_normalized_value("Sync"), Some(1.0));
        assert_eq!(p.string_to_normalized_value("free"), Some(0.0));
        assert_eq!(p.string_to_normalized_value("1"), Some(1.0));
        assert_eq!(p.string_to_normalized_value("banana"), None);
    }
}
