//! The view settings the right rail switches.

/// How a focused track is sized, and what it costs its neighbours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Whether clicking a track opens it wide enough to edit a
    /// plugin's parameters on.
    pub focus_selected: bool,
    /// Whether a focused track takes its width FROM the other tracks or
    /// adds to the mixer's total.
    ///
    /// On, the mixer stays exactly as wide as it was and everything
    /// else gives up a few pixels — nothing moves off the screen you
    /// were looking at. Off, the other tracks keep their width and are
    /// simply pushed along, which is what you want when their widths
    /// are the thing you are comparing and must not change under you.
    pub take_focus_width: bool,
    /// How much of a 16:9 screen one focused track takes — this
    /// window's override of `scenes::TABLES.focus.fraction`.
    pub focus_fraction: f64,
    /// Whether folding a phase's container folds it on every track.
    ///
    /// On, the chain stays in register across the mixer: fold Rescue
    /// away and every strip's Tone panels sit at the same height, which
    /// is what makes a row of racks comparable at all. Off, each track
    /// folds on its own — for when you are working one track rather
    /// than comparing them, and the other strips' chains are just in
    /// the way.
    pub fold_phases_together: bool,
    /// Whether a folder the view has shut shows its children's takes on
    /// its own row.
    ///
    /// On, a kit reads as one row per drum: the kick's three mics are
    /// one item in the kick's colour, under the kick's fader, and the
    /// mics are a fold away when you want them. That is what an
    /// arrangement is for — the routing that makes a drum a folder is a
    /// mix concern, and mixing happens on the strip.
    ///
    /// Off, a shut folder is empty, which is what REAPER draws. Worth
    /// keeping as a switch rather than a decision: the fold is a claim
    /// that the children are one performance, and on a folder where
    /// they are not — a bus with a bass and a vocal under it — the row
    /// says something true but useless, and you want to see the tree
    /// instead.
    pub folded_takes: bool,
    /// Compact mixer strips: no input section, the coloured band only as
    /// tall as the pan knob and the record arm, and the height they gave
    /// up handed to the fader. The FX row stays. See `strip::shape`.
    ///
    /// Live mode's strips first, and now every mode's: the input section
    /// was a tall empty band over every fader for a record-input picker
    /// set once per session. Off is REAPER's full collapse layout.
    pub live_strips: bool,
    /// Touchscreen strips: mute and solo grown to fill their column
    /// (`strip::Strip::touched`).
    pub touch_strips: bool,
    /// The mode the strips are laid out for, where a theme asks: a strip
    /// in Mix mode has no use for an arm, one in Record mode has the most
    /// for its routing (`skin`'s `ui.mixing` / `ui.recording`).
    pub mixing: bool,
    pub recording: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            focus_selected: true,
            take_focus_width: true,
            focus_fraction: dynamic_template::scenes::TABLES.focus.fraction,
            fold_phases_together: true,
            folded_takes: true,
            live_strips: true,
            touch_strips: false,
            mixing: false,
            recording: false,
        }
    }
}

impl Settings {
    /// How wide a focused track opens, for a panel `height` tall.
    ///
    /// The scene module's own focus parameters, with this window's
    /// `focus_fraction` over the top — the switch on the right rail is
    /// an override of the table, not a second table. See
    /// `dynamic_template::scenes::surface::Focus` for why the width
    /// comes off the height.
    #[must_use]
    pub fn focus_width(self, height: f64) -> f64 {
        dynamic_template::scenes::Focus {
            fraction: self.focus_fraction,
            ..dynamic_template::scenes::TABLES.focus
        }
        .width(height)
    }
}

#[cfg(test)]
mod tests {
    use super::Settings;

    /// Four across the PANEL of a 16:9 screen, whatever its resolution.
    #[test]
    fn a_quarter_of_sixteen_by_nine() {
        let settings = Settings::default();
        for (w, h) in [(1920.0, 1080.0), (2560.0, 1440.0), (3840.0, 2160.0)] {
            let focus = settings.focus_width(h);
            let w = w - crate::rails::SIDE * 2.0;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::as_conversions,
                reason = "a count of strips across a screen"
            )]
            let fits = (w / focus) as u32;
            assert_eq!(fits, 4, "{w}x{h} fits {fits} at {focus}px");
        }
    }

    /// And a wider aspect gets MORE of them, not wider ones — the whole
    /// reason this comes off the height.
    #[test]
    fn a_superwide_gets_more_tracks_not_bigger_ones() {
        let settings = Settings::default();
        let sixteen_nine = settings.focus_width(1440.0);
        let superwide = settings.focus_width(1440.0);
        assert!(
            (sixteen_nine - superwide).abs() < f64::EPSILON,
            "the same height must give the same width"
        );
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::as_conversions,
            reason = "a count of strips across a screen"
        )]
        let fits = ((5120.0 - crate::rails::SIDE * 2.0) / superwide) as u32;
        assert_eq!(fits, 8, "a 32:9 screen should hold eight at {superwide}px");
    }

    /// Never narrower than a rack can be read at, whatever the setting.
    #[test]
    fn a_focused_track_is_always_legible() {
        let tiny = Settings {
            focus_fraction: 0.01,
            ..Settings::default()
        };
        assert!(tiny.focus_width(1440.0) >= crate::tone::LEGIBLE);
    }
}
