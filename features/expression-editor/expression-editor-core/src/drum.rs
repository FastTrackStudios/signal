/// One hand edit leaving the stack, in seconds — the host decides what
/// it means on the daw. `Slip` cuts and slides (SPLIT), `Stretch`
/// writes a marker map (WARP); `Add`/`Remove` edit the hit list only.
///
/// One enum rather than one callback per gesture, because every arm
/// shares a fate: they land on the *group*, as one undo step, through
/// the drum host — and a host that takes one takes them all.
#[derive(Clone, Debug, PartialEq)]
pub enum HitGesture {
    /// r[impl drums.manual.slip]
    Slip {
        hit: f64,
        /// `f64::INFINITY` when the hit is the lane's last — the host
        /// clamps to its take length.
        next: f64,
        delta: f64,
    },
    /// r[impl drums.manual.stretch]
    Stretch {
        hit: f64,
        /// `f64::NEG_INFINITY` when the hit is the lane's first.
        prev: f64,
        next: f64,
        delta: f64,
        /// The BothStretch law: pin the take's ends, not the
        /// neighbours.
        both: bool,
    },
    /// Cut every member of the kit at `at` seconds.
    ///
    /// Unlike the others this moves nothing: it only puts an item
    /// boundary where the user asked for one, so the piece either side
    /// can then be dragged, deleted or replaced.
    // r[impl drums.manual.split]
    Split { at: f64 },
    /// r[impl drums.manual.add-remove]
    Add { lane: String, at: f64 },
    /// r[impl drums.manual.add-remove]
    Remove { lane: String, hit: f64 },
}
