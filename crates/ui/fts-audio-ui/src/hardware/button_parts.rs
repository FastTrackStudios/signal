//! The kit: every panel button this crate can draw, as a [`ButtonSpec`] each.
//!
//! One const per part, with the unit it came off it named — see
//! [`button_kit`](crate::hardware::button_kit) for how to add one, and the
//! `button_sheet` test for how to look at it.

use super::button_kit::{ButtonSpec, Cap, CapFinish, Lit, Surround};

// ─────────────────────────────────────────────────────────────────────────
// Console — a latching rectangular cap with a jewel under it.
//
// The mixing-desk idiom, and the kit's default. The lamp is a separate part
// below the cap rather than in it, which is how you read a channel's state
// across a room without being able to make out the legend.
// ─────────────────────────────────────────────────────────────────────────
pub static CONSOLE: ButtonSpec = ButtonSpec {
    cap: Cap {
        radius: 3.0,
        finish: CapFinish::Matte,
        border: "rgba(0,0,0,0.62)",
    },
    lit: Lit::Jewel { d: 9.0, gap: 5.0 },
    surround: None,
    travel: 1.5,
    legend: 9.0,
};

// ─────────────────────────────────────────────────────────────────────────
// Illuminated — the cap itself lights, and the legend glows through it.
//
// An SSL bus compressor's IN button: an amber square in a dark surround,
// which is doing two jobs — stopping the glow bleeding into the panel, and
// giving the cap somewhere to sink into.
// ─────────────────────────────────────────────────────────────────────────
pub static ILLUMINATED: ButtonSpec = ButtonSpec {
    cap: Cap {
        radius: 3.5,
        finish: CapFinish::Gloss,
        border: "rgba(0,0,0,0.55)",
    },
    lit: Lit::Backlit { bloom: 11.0 },
    surround: Some(Surround {
        color: "linear-gradient(180deg, #4a3524 0%, #2e2116 100%)",
        pad: 5.0,
        radius: 5.0,
    }),
    travel: 1.4,
    legend: 9.5,
};

// ─────────────────────────────────────────────────────────────────────────
// Push-in — the 1176's ratio bank. No lamp at all: you read it by which one
// is *down*, which is why the travel is the deepest in the kit and why the
// cap is nearly square-cornered.
// ─────────────────────────────────────────────────────────────────────────
pub static PUSH_IN: ButtonSpec = ButtonSpec {
    cap: Cap {
        radius: 1.6,
        finish: CapFinish::Matte,
        border: "rgba(0,0,0,0.70)",
    },
    lit: Lit::Unlit,
    surround: Some(Surround {
        color: "linear-gradient(180deg, #23252a 0%, #15171a 100%)",
        pad: 3.2,
        radius: 2.2,
    }),
    travel: 2.6,
    legend: 8.5,
};

// ─────────────────────────────────────────────────────────────────────────
// Square — the SSL 4000 channel's small backlit switch. Harder-cornered than
// the illuminated one, no surround, and a tighter bloom: there are rows of
// these and a soft glow on each would smear into one band.
// ─────────────────────────────────────────────────────────────────────────
pub static SQUARE: ButtonSpec = ButtonSpec {
    cap: Cap {
        radius: 2.0,
        finish: CapFinish::Matte,
        border: "rgba(0,0,0,0.6)",
    },
    lit: Lit::Backlit { bloom: 6.0 },
    surround: None,
    travel: 1.3,
    legend: 9.0,
};

// ─────────────────────────────────────────────────────────────────────────
// Metal — a machined cap on outboard gear: a bare metal button with a jewel
// under it, brushed rather than moulded.
// ─────────────────────────────────────────────────────────────────────────
pub static METAL: ButtonSpec = ButtonSpec {
    cap: Cap {
        radius: 2.6,
        finish: CapFinish::Metal,
        border: "rgba(0,0,0,0.66)",
    },
    lit: Lit::Jewel { d: 8.0, gap: 5.0 },
    surround: None,
    travel: 1.7,
    legend: 8.5,
};

#[cfg(test)]
mod tests {
    use crate::hardware::button::ButtonStyle;

    /// The kit covers the idioms that actually differ, and each is a distinct
    /// drawing rather than a recolour of its neighbour. Two specs that are
    /// equal are one part with two names, which is the thing this kit exists
    /// to stop.
    #[test]
    fn no_two_buttons_in_the_kit_are_the_same_part() {
        let all: Vec<_> = ButtonStyle::ALL.iter().map(|s| s.spec()).collect();
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate().skip(i + 1) {
                assert_ne!(
                    a,
                    b,
                    "{:?} and {:?} are the same part",
                    ButtonStyle::ALL[i],
                    ButtonStyle::ALL[j],
                );
            }
        }
    }

    /// A button with no lamp has to be readable some other way, and the only
    /// other way is travel. The 1176's ratio bank is exactly this case, and
    /// it is why its throw is the deepest in the kit.
    #[test]
    fn an_unlit_button_travels_further_than_a_lit_one() {
        let lit_max = ButtonStyle::ALL
            .iter()
            .map(|s| s.spec())
            .filter(|s| s.is_lit())
            .map(|s| s.travel)
            .fold(0.0_f64, f64::max);
        for style in ButtonStyle::ALL {
            let spec = style.spec();
            if spec.is_lit() {
                continue;
            }
            assert!(
                spec.travel > lit_max,
                "{style:?} has no lamp and does not out-travel the lit buttons, \
                 so nothing reads its state",
            );
        }
    }
}
