//! Changes, on their way to the engine.
//!
//! The window does not own the session — `daw-standalone` does. A fader
//! drag is not "move the fader", it is "tell the engine this track's
//! volume is now this", and the fader moves because the engine says so.
//!
//! That ordering is the whole point. It is what will let a second
//! window, or a real REAPER over `daw-sync`, see the same change: the
//! engine is the thing both are looking at, and a UI that mutated its
//! own copy would be a UI that disagrees with everyone else the moment
//! anything else touches the session.
//!
//! # Why the edits queue rather than being sent
//!
//! A fader drag fires an event per mouse move — a hundred a second on a
//! trackpad. Sent one RPC each, that is a hundred round trips to
//! deliver one number, ninety-nine of which are already stale when they
//! arrive.
//!
//! So continuous edits COALESCE: the queue keeps the latest value per
//! track and control, and the worker sends what is current when it gets
//! to it. Discrete edits — a mute, a rename — never coalesce, because
//! two mute toggles are two different intentions and collapsing them
//! into one would silently drop half of a double-click.

use std::collections::VecDeque;

/// Something the user did that the engine needs to know about.
#[derive(Clone, PartialEq, Debug)]
pub enum Edit {
    ToggleMute(String),
    ToggleSolo(String),
    ToggleArm(String),
    /// Select this track and nothing else — what a plain click means
    /// everywhere, and what makes "focus the selected track" a focus
    /// rather than an accumulation.
    Select(String),
    /// Add this track to the selection, leaving the rest alone. The
    /// modified click: REAPER's Ctrl, and the same modifier that makes
    /// a drag fine — they apply to different gestures, so one key does
    /// both without ambiguity.
    AddToSelection(String),
    /// A gain, not a dB value — `Track::volume`'s own unit.
    SetVolume(String, f64),
    SetPan(String, f64),
    Rename(String, String),
    /// Polarity. The one control on a strip that changes the signal
    /// without changing a level, which is why it sits on its own in
    /// the corner rather than in the button column.
    ///
    /// Carries the value rather than being a toggle, because the engine
    /// has a setter and not a toggle for it — and because the value the
    /// window is already SHOWING is the one the user meant. Re-reading
    /// the state to invert it would let a click disagree with the
    /// control it landed on.
    SetPhase(String, bool),
    /// Which of the track's fixed lanes play (bit `n`, lane `n`): the
    /// panel's lane steps play one lane at a time.
    SetLanePlays(String, u64),
    /// Whether the track's input is monitored, and when.
    ///
    /// Three states rather than a toggle, because REAPER's is three:
    /// off, on, and on-except-while-playing. A click walks them.
    SetInputMonitor(String, daw_proto::track::InputMonitoringMode),
    /// Whether the track feeds its parent at all.
    ///
    /// A folder's children normally sum into it; a track with this off
    /// is heard only through its own sends, which is how a parallel
    /// path is built. The routing widget's first lane says which.
    SetParentSend(String, bool),
    /// A parameter of an FX on a track's chain, found by its name
    /// (`FTS Comp`): index and normalized value (0..1). What the strip's
    /// Tone rack sends as it is turned (`tone::Store::flush`).
    SetFxParam(String, String, u32, f64),
    /// An FX on a track's chain switched on or bypassed, by its name.
    SetFxEnabled(String, String, bool),
    /// Send this track to that one.
    ///
    /// Two guids, because a send is a relation and not a property: the
    /// source owns it and the destination is a track the engine has to
    /// resolve. The index it comes back with is REAPER's, and the
    /// window does not guess it — the read that follows says what it
    /// was.
    AddSend(String, String),
    /// Take one of this track's routes away.
    ///
    /// Removing renumbers every route after it in the same list, which
    /// is why nothing here patches a list after one: the panel forgets
    /// the track and reads it again.
    RemoveRoute(String, crate::routes::At),
    /// How much goes down one of the track's routes.
    ///
    /// A gain, the way the track's own volume is: 1.0 is unity. Works
    /// on all three kinds — a receive is the same object as the send
    /// feeding it, so turning one down turns the other down, and a
    /// hardware output is how loud the track leaves the box.
    SetRouteVolume(String, crate::routes::At, f64),
    /// Where the route sits, -1.0 to 1.0.
    SetRoutePan(String, crate::routes::At, f64),
    /// Whether the route passes anything at all.
    SetRouteMute(String, crate::routes::At, bool),
    /// Where a SEND is tapped from: post-fader, pre-FX, post-FX.
    ///
    /// The difference between a reverb that follows the fader and one
    /// that does not, which is the first thing anybody changes about a
    /// send and the thing REAPER hides in a menu.
    ///
    /// Sends only, because that is what the service takes: a receive's
    /// mode is the mode of the send feeding it, set on the track that
    /// owns that send.
    SetSendMode(String, u32, daw_proto::routing::SendMode),
    /// The track's colour, as REAPER stores it (0xRRGGBB).
    ///
    /// Carries the value rather than cycling, because a colour is
    /// chosen and not stepped through, and the window is the thing
    /// holding the palette the user picked from.
    SetColor(String, u32),
    /// Trim, read, touch, write, latch, latch-preview.
    ///
    /// Governs whether moving a control writes envelope points while
    /// the transport runs, so it changes what every OTHER edit on this
    /// track means. That is why it is a value and not a toggle: there
    /// is no safe "next" mode to guess at.
    SetAutomationMode(String, daw_proto::primitives::AutomationMode),
    /// What the track records from — a hardware channel, or a MIDI
    /// device and channel.
    ///
    /// The one an engineer changes while tracking, which is exactly
    /// when a second screen showing the old answer is worst.
    SetRecordInput(String, daw_proto::track::RecordInput),
    /// Whether the track is drawn in the arrange panel, the mixer, or
    /// both. Both flags travel together because REAPER sets them
    /// together, and sending one would silently re-assert the other.
    SetVisibility(String, bool, bool),
    /// How tall the track's panel is drawn, in pixels.
    ///
    /// A view concern that lives in the project, so a height set here
    /// is a height REAPER opens with — which is the whole point of a
    /// scene that arranges the session rather than only this window.
    SetHeight(String, u32),
    /// Folder depth: positive opens a folder, negative closes one or
    /// more levels, zero is an ordinary track.
    ///
    /// The one edit that changes the SHAPE of the session rather than a
    /// value in it, so a window applying it has to re-read rather than
    /// patch — every depth below it moves.
    SetFolderDepth(String, i32),
    /// Join or leave one of REAPER's 128 group slots, in every family
    /// at once.
    SetGroupMembership(String, u32, bool),
    /// The track's role — lead, follow, neither — in ONE family of one
    /// slot. What a VCA actually is.
    SetGroupFlags(
        String,
        u32,
        daw_proto::track::GroupFamily,
        daw_proto::track::GroupRole,
    ),
    /// One of the per-slot modifiers that is not a lead/follow pair:
    /// reversed volume, no-lead-when-follow, and the rest.
    SetGroupModifier(String, u32, daw_proto::track::GroupModifier, bool),
    // ── The ruler ────────────────────────────────────────────────
    //
    // Markers and regions are addressed by REAPER's own number rather
    // than by a guid, because that is the only identity the live API
    // offers — there IS a guid in the project file and the live API
    // never fills it. The number is not stable across REAPER's
    // renumber action, which is why the event stream reports a
    // renumbering as one rather than as a deletion: a window holding
    // one of these can follow it rather than lose it.
    //
    // The guid field every other edit carries is empty here. These are
    // not about a track.
    /// Put a marker at a time, in a ruler lane.
    AddMarker(String, f64, u32),
    /// Move a marker to a time.
    MoveMarker(String, u32, f64),
    /// Rename one.
    RenameMarker(String, u32, String),
    /// Take it away.
    RemoveMarker(String, u32),
    /// Make a region spanning two times, in a ruler lane.
    ///
    /// A region in the SECTIONS lane IS a song section — the song
    /// model reads the arrangement from that lane — so this is how a
    /// verse comes into being.
    AddRegion(String, f64, f64, u32),
    /// Set both of a region's bounds. Both together because REAPER's
    /// setter takes both, and because dragging one end of a band the
    /// user can see is a change to the band.
    SetRegionBounds(String, u32, f64, f64),
    RenameRegion(String, u32, String),
    RemoveRegion(String, u32),
    /// Set the tempo of the stretch starting at a time, adding a tempo
    /// marker there if there is not one.
    ///
    /// What tempo mapping writes. The guid is empty: a tempo belongs to
    /// the project, not to a track.
    SetTempo(String, f64, f64),
    /// An ITEM's fade-in: its length in seconds and its shape. The guid
    /// is the item's, not a track's.
    SetFadeIn(String, f64, daw_proto::item::FadeShape),
    /// And its fade-out.
    SetFadeOut(String, f64, daw_proto::item::FadeShape),
    /// Select an item — alone, or added to the selection.
    SelectItem(String, bool),
    /// Take one item back OUT of the selection.
    ///
    /// The other half of the modified click. `SelectItem(_, false)`
    /// adds and `DeselectAllItems` empties; neither can remove one,
    /// which is why a Ctrl-click used to be a one-way gesture — you
    /// could build a selection up a piece at a time and not correct it.
    DeselectItem(String),
    /// Nothing selected. Carries no guid; the string is empty.
    DeselectAllItems(String),
    /// Open an undo step. Everything between this and `EndUndo` is ONE
    /// entry in the session's undo history.
    ///
    /// Carries the label the history will show. A gesture on a folded
    /// row is one gesture however many mics it reaches, and an undo
    /// that took back a third of it would be worse than one that took
    /// back none.
    BeginUndo(String),
    /// Close the undo step `BeginUndo` opened. Same label — REAPER's
    /// block API takes it at both ends.
    EndUndo(String),
    /// Everything selected. The same.
    SelectAllItems(String),
    /// Move an item to a position, in seconds.
    MoveItem(String, f64),
    /// Trim an item to a position and a length, in seconds.
    TrimItem(String, f64, f64),
    /// Split an item at a time: the item keeps the left, and a new one
    /// on the same track (its guid is the third field) takes the right.
    SplitItem(String, f64, String),
    /// Slip an item's contents: how far into the source it starts, in
    /// seconds. The item does not move; the audio inside it does.
    SlipItem(String, f64),
    /// Copy an item to a position on its own track, leaving the
    /// original where it is. The third field is the copy's guid.
    ///
    /// Invented by the window, the way `SplitItem`'s is, and local to
    /// it: the engine makes its own and the next read replaces the
    /// prediction. What the guid buys is a picture that is right on the
    /// next frame rather than whenever the session answers.
    CopyItem(String, f64, String),
    /// Put an item on a fixed lane (REAPER 7's comping lanes): a comp's
    /// piece onto the comp lane.
    SetItemLane(String, u32),
    /// An item's volume (linear): its dial, or gain staging.
    SetItemVolume(String, f64),
    /// An item's active take's stretch markers, `(take time, source
    /// time)` each — a slice's decay drawn out over the gap a slip left.
    SetStretchMarkers(String, Vec<(f64, f64)>),
    /// Where an item's hit is, seconds from its start: a drum slice's
    /// snap offset, which the next edit reads to know where the attack
    /// it must not play twice is.
    SetSnapOffset(String, f64),
    DeleteItem(String),
    /// An item's active take's volume curve, all its points (seconds into
    /// the item, linear gain): a point dragged on the item.
    SetTakeVolume(String, Vec<daw_proto::automation::EnvelopePoint>),
    /// A track's envelope's points, all of them: a point drawn, dragged
    /// or deleted in its lane ([`crate::automation`]).
    SetEnvelope(
        String,
        daw_proto::automation::EnvelopeRef,
        Vec<daw_proto::automation::EnvelopePoint>,
    ),
    /// An automation item's curve — its pool's, so every instance's.
    SetItemCurve(
        String,
        daw_proto::automation::EnvelopeRef,
        u32,
        Vec<daw_proto::automation::EnvelopePoint>,
    ),
    /// An automation item moved to a position, in seconds.
    MoveAutomationItem(String, daw_proto::automation::EnvelopeRef, u32, f64),
    /// An automation item made: at a position, a length (seconds), in a
    /// pool (`-1`: a fresh one, its curve the points given, seconds into
    /// it), its curve its pool's otherwise.
    AddAutomationItem(
        String,
        daw_proto::automation::EnvelopeRef,
        (f64, f64, i32),
        Vec<daw_proto::automation::EnvelopePoint>,
    ),
    /// An automation item's bounds: its index, position, length and how
    /// far into its source it starts (seconds) — an edge dragged.
    SetAutomationItemBounds(
        String,
        daw_proto::automation::EnvelopeRef,
        u32,
        (f64, f64, f64),
    ),
    /// An automation item deleted, by index (the envelope's points under
    /// it play again).
    DeleteAutomationItem(String, daw_proto::automation::EnvelopeRef, u32),
    /// Show or hide a track's envelope in a lane of its own.
    ShowEnvelope(String, daw_proto::automation::EnvelopeRef, bool),
    /// One envelope's automation mode.
    SetEnvelopeMode(
        String,
        daw_proto::automation::EnvelopeRef,
        daw_proto::primitives::AutomationMode,
    ),
    /// An envelope lane's height, in pixels.
    SetLaneHeight(String, daw_proto::automation::EnvelopeRef, u32),
    /// A track's fader or pan knob taken hold of (`true`) or let go: what
    /// Touch and Latch automation record by ([`Touched`]).
    Touch(String, Touched, bool),
}

/// Which of a track's controls a [`Edit::Touch`] is about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Touched {
    Volume,
    Pan,
}

impl Touched {
    /// The control a turn is on, if it is one automation writes.
    #[must_use]
    pub const fn of(volume: bool, pan: bool) -> Option<Self> {
        if volume {
            Some(Self::Volume)
        } else if pan {
            Some(Self::Pan)
        } else {
            None
        }
    }
}

impl Edit {
    /// The track this is about.
    #[must_use]
    pub fn guid(&self) -> &str {
        match self {
            Self::ToggleMute(g)
            | Self::ToggleSolo(g)
            | Self::ToggleArm(g)
            | Self::Select(g)
            | Self::AddToSelection(g)
            | Self::SetVolume(g, _)
            | Self::SetPan(g, _)
            | Self::Rename(g, _)
            | Self::SetPhase(g, _)
            | Self::SetLanePlays(g, _)
            | Self::SetInputMonitor(g, _)
            | Self::SetParentSend(g, _)
            | Self::SetFxParam(g, ..)
            | Self::SetFxEnabled(g, ..)
            | Self::AddSend(g, _)
            | Self::RemoveRoute(g, _)
            | Self::SetRouteVolume(g, ..)
            | Self::SetRoutePan(g, ..)
            | Self::SetRouteMute(g, ..)
            | Self::SetSendMode(g, ..)
            | Self::SetColor(g, _)
            | Self::SetAutomationMode(g, _)
            | Self::SetRecordInput(g, _)
            | Self::SetVisibility(g, ..)
            | Self::SetHeight(g, _)
            | Self::SetFolderDepth(g, _)
            | Self::SetGroupMembership(g, ..)
            | Self::SetGroupFlags(g, ..)
            | Self::SetGroupModifier(g, ..)
            | Self::AddMarker(g, ..)
            | Self::MoveMarker(g, ..)
            | Self::RenameMarker(g, ..)
            | Self::RemoveMarker(g, ..)
            | Self::AddRegion(g, ..)
            | Self::SetRegionBounds(g, ..)
            | Self::RenameRegion(g, ..)
            | Self::RemoveRegion(g, ..)
            | Self::SetTempo(g, ..)
            | Self::SetFadeIn(g, ..)
            | Self::SetFadeOut(g, ..)
            | Self::SelectItem(g, _)
            | Self::DeselectItem(g)
            | Self::DeselectAllItems(g)
            | Self::BeginUndo(g)
            | Self::EndUndo(g)
            | Self::SelectAllItems(g)
            | Self::MoveItem(g, _)
            | Self::TrimItem(g, ..)
            | Self::SplitItem(g, ..)
            | Self::CopyItem(g, ..)
            | Self::SetItemLane(g, _)
            | Self::SetItemVolume(g, _)
            | Self::SetStretchMarkers(g, _)
            | Self::SetSnapOffset(g, _)
            | Self::SetTakeVolume(g, _)
            | Self::SlipItem(g, _)
            | Self::DeleteItem(g)
            | Self::SetEnvelope(g, ..)
            | Self::SetItemCurve(g, ..)
            | Self::MoveAutomationItem(g, ..)
            | Self::AddAutomationItem(g, ..)
            | Self::SetAutomationItemBounds(g, ..)
            | Self::DeleteAutomationItem(g, ..)
            | Self::ShowEnvelope(g, ..)
            | Self::SetEnvelopeMode(g, ..)
            | Self::SetLaneHeight(g, ..)
            | Self::Touch(g, ..) => g,
        }
    }

    /// Whether this edit is about the ruler rather than a track or an
    /// item. Its guid field is empty; it is addressed by number.
    #[must_use]
    pub const fn is_ruler(&self) -> bool {
        matches!(
            self,
            Self::SetTempo(..)
                | Self::AddMarker(..)
                | Self::MoveMarker(..)
                | Self::RenameMarker(..)
                | Self::RemoveMarker(..)
                | Self::AddRegion(..)
                | Self::SetRegionBounds(..)
                | Self::RenameRegion(..)
                | Self::RemoveRegion(..)
        )
    }

    /// Whether this is about an item rather than a track.
    #[must_use]
    pub const fn is_item(&self) -> bool {
        matches!(
            self,
            Self::SetFadeIn(..)
                | Self::SetFadeOut(..)
                | Self::SelectItem(..)
                | Self::DeselectItem(_)
                | Self::DeselectAllItems(_)
                | Self::SelectAllItems(_)
                | Self::MoveItem(..)
                | Self::TrimItem(..)
                | Self::SplitItem(..)
                | Self::CopyItem(..)
                | Self::SetItemLane(..)
                | Self::SetItemVolume(..)
                | Self::SetStretchMarkers(..)
                | Self::SetSnapOffset(..)
                | Self::SetTakeVolume(..)
                | Self::SlipItem(..)
                | Self::DeleteItem(_)
        )
    }

    /// Whether a newer edit of this kind replaces this one.
    ///
    /// True for the things a drag produces a stream of, false for the
    /// things a click produces one of. Two volumes are one volume; two
    /// mutes are two mutes, and collapsing them would eat half of a
    /// double-click.
    #[must_use]
    pub const fn coalesces(&self) -> bool {
        matches!(
            self,
            Self::SetVolume(..)
                | Self::SetPan(..)
                | Self::SetRouteVolume(..)
                | Self::SetRoutePan(..)
                | Self::SetFadeIn(..)
                | Self::SetFadeOut(..)
                | Self::MoveItem(..)
                | Self::TrimItem(..)
                | Self::SlipItem(..)
                | Self::SetEnvelope(..)
                | Self::SetItemCurve(..)
                | Self::MoveAutomationItem(..)
                | Self::SetAutomationItemBounds(..)
                | Self::SetLaneHeight(..)
                | Self::SetTakeVolume(..)
        )
    }

    /// Which envelope (and automation item) this edit is about, if any —
    /// part of a control's identity like [`Self::route`]: two lanes on
    /// one track are two controls.
    #[must_use]
    pub(crate) fn lane(&self) -> Option<String> {
        match self {
            Self::SetEnvelope(_, target, _)
            | Self::ShowEnvelope(_, target, _)
            | Self::SetEnvelopeMode(_, target, _)
            | Self::SetLaneHeight(_, target, _) => Some(crate::automation::target_key(target)),
            Self::SetItemCurve(_, target, item, _)
            | Self::MoveAutomationItem(_, target, item, _)
            | Self::SetAutomationItemBounds(_, target, item, _)
            | Self::DeleteAutomationItem(_, target, item) => {
                Some(format!("{}#{item}", crate::automation::target_key(target)))
            }
            _ => None,
        }
    }

    /// Whether this is about a track's automation.
    #[must_use]
    pub const fn is_automation(&self) -> bool {
        matches!(
            self,
            Self::SetEnvelope(..)
                | Self::SetItemCurve(..)
                | Self::MoveAutomationItem(..)
                | Self::AddAutomationItem(..)
                | Self::SetAutomationItemBounds(..)
                | Self::DeleteAutomationItem(..)
                | Self::ShowEnvelope(..)
                | Self::SetEnvelopeMode(..)
                | Self::SetLaneHeight(..)
        )
    }

    /// Which of the track's routes this edit is about, if any — part of a
    /// control's identity: two sends on one track are two controls, and a
    /// receive numbered as a send is not that send ([`Queue`]).
    #[must_use]
    pub(crate) const fn route(&self) -> Option<crate::routes::At> {
        match self {
            Self::RemoveRoute(_, at)
            | Self::SetRouteVolume(_, at, _)
            | Self::SetRoutePan(_, at, _)
            | Self::SetRouteMute(_, at, _) => Some(*at),
            Self::SetSendMode(_, index, _) => Some(crate::routes::At::new(
                daw_proto::routing::RouteType::Send,
                *index,
            )),
            _ => None,
        }
    }
}

/// Edits waiting to go out.
///
/// A drag's positions collapse to its latest (see [`Edit::coalesces`]),
/// found by index, not by a scan — a kit's slip edit queues a hundred
/// thousand edits at once, and a scan per push was minutes of the worker
/// comparing edits before it sent one. And never across an edit that
/// depends on the one replaced: a move of an item queued before a split
/// of it, and another after, are two moves — collapsed, the later
/// position would be applied before the split.
#[derive(Debug, Default)]
pub struct Queue {
    pending: VecDeque<Edit>,
    /// How many edits have gone out: an edit's number less this is where
    /// it is in `pending`.
    popped: u64,
    /// The number of the latest queued coalescing edit, per control.
    control: std::collections::HashMap<ControlKey, u64>,
    /// The number of the latest queued edit that does not coalesce, per
    /// item or track: nothing before it may take a later value.
    barrier: std::collections::HashMap<String, u64>,
}

/// A control's identity: its item or track, which edit, which route.
type ControlKey = (
    String,
    std::mem::Discriminant<Edit>,
    Option<crate::routes::At>,
    Option<String>,
);

impl Queue {
    /// Add an edit, replacing a superseded one if there is one.
    pub fn push(&mut self, edit: Edit) {
        let number = self
            .popped
            .saturating_add(u64::try_from(self.pending.len()).unwrap_or(u64::MAX));
        let guid = edit.guid().to_owned();
        if edit.coalesces() {
            let key: ControlKey = (
                guid,
                std::mem::discriminant(&edit),
                edit.route(),
                edit.lane(),
            );
            let live = self.control.get(&key).copied().filter(|&at| {
                at >= self.popped && self.barrier.get(&key.0).is_none_or(|&wall| wall < at)
            });
            if let Some(at) = live
                && let Some(slot) = usize::try_from(at.saturating_sub(self.popped))
                    .ok()
                    .and_then(|i| self.pending.get_mut(i))
            {
                *slot = edit;
                return;
            }
            self.control.insert(key, number);
        } else {
            self.barrier.insert(guid, number);
        }
        self.pending.push_back(edit);
    }

    /// The next edit to send.
    pub fn pop(&mut self) -> Option<Edit> {
        let next = self.pending.pop_front();
        if next.is_some() {
            self.popped = self.popped.saturating_add(1);
        }
        if self.pending.is_empty() {
            // Nothing queued refers to them now.
            self.control.clear();
            self.barrier.clear();
        }
        next
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// What a gesture on a control means for the engine.
///
/// Returns `None` where the control has no engine-side effect yet — the
/// FX pill opens a chain nothing hosts, and routing opens a matrix that
/// does not exist. They are hit-testable and will mean something; they
/// do not mean anything now, and inventing an edit for them would be
/// worse than doing nothing.
#[must_use]
pub fn click(
    control: crate::mcp::Control,
    guid: &str,
    from: &daw_proto::Track,
    add: bool,
) -> Option<Edit> {
    let guid = guid.to_owned();
    match control {
        crate::mcp::Control::Mute => Some(Edit::ToggleMute(guid)),
        crate::mcp::Control::Solo => Some(Edit::ToggleSolo(guid)),
        crate::mcp::Control::RecArm => Some(Edit::ToggleArm(guid)),
        // Clicking the name selects the track; DOUBLE-clicking renames
        // it, which is a different gesture and a different edit.
        crate::mcp::Control::Name => Some(if add {
            Edit::AddToSelection(guid)
        } else {
            Edit::Select(guid)
        }),
        // The routing widget opens the panel, which is the window's
        // and not an edit at all — see `crate::routing`. The parent
        // send it used to toggle is the panel's first row, where it
        // sits beside the sends it shares a button with.
        crate::mcp::Control::Routing => None,
        // The FX button opens a chain window, and there is no chain and
        // no window — see `tone::placeholder` and `bin/chain-probe`.
        // Binding it to something else would be a button that does the
        // wrong thing rather than one that waits.
        crate::mcp::Control::Volume | crate::mcp::Control::Pan | crate::mcp::Control::Fx => None,
        // Clearing a clip changes nothing about the track — it is the
        // window forgetting something, not the engine being told
        // something. Handled where the latch lives.
        crate::mcp::Control::Clip => None,
        // Folding is the window's: it changes which strips exist, not
        // what any track is — see the window's fold.
        crate::mcp::Control::Folder => None,
        // The lamp is only drawn on an armed track, and what is not
        // drawn is not clicked.
        crate::mcp::Control::Monitor => from
            .armed
            .then(|| Edit::SetInputMonitor(guid, next_monitor(from.input_monitor))),
    }
}

/// The monitoring mode a click moves to.
///
/// Off → on → on-except-while-playing → off. REAPER's own order, and
/// the one that puts the mode you reach for most one click away.
#[must_use]
pub const fn next_monitor(
    from: daw_proto::track::InputMonitoringMode,
) -> daw_proto::track::InputMonitoringMode {
    use daw_proto::track::InputMonitoringMode as M;
    match from {
        M::Off => M::Normal,
        M::Normal => M::NotWhenPlaying,
        M::NotWhenPlaying => M::Off,
    }
}

/// What a drag on a control means, given how far it moved.
///
/// `fraction` is the drag as a share of the control's travel, already
/// signed so that up is more — see `gesture::drag_fraction`.
#[must_use]
pub fn drag(
    control: crate::mcp::Control,
    guid: &str,
    from: &daw_proto::Track,
    fraction: f64,
) -> Option<Edit> {
    match control {
        crate::mcp::Control::Volume => {
            // Through the fader's own taper, so dragging a third of the
            // way up moves it a third of the way up the SCALE rather
            // than a third of the way up the gain — which at the top of
            // a fader is most of its range and at the bottom is none.
            let at = daw_theme_art::paint::tcp::gain_norm(from.volume);
            let to = (at + fraction).clamp(0.0, 1.0);
            let db = daw_theme_art::paint::tcp::fader_db(to);
            Some(Edit::SetVolume(guid.to_owned(), db_to_gain(db)))
        }
        crate::mcp::Control::Pan => {
            // Pan is linear and two-sided, so the same drag is worth
            // twice as much of its range as it is of a fader's.
            let to = (from.pan + fraction * 2.0).clamp(-1.0, 1.0);
            Some(Edit::SetPan(guid.to_owned(), to))
        }
        _ => None,
    }
}

/// Where the transport is, polled off the event loop.
///
/// The engine is asked once per frame rather than subscribed to,
/// because a position is a LEVEL and not an event: missing one is
/// harmless (the next is along in a few milliseconds) and the
/// extrapolation covers the gap. A subscription would deliver a
/// backlog after a stall, which is the one thing a playhead must not
/// replay.
pub struct Transport {
    state: std::sync::Arc<std::sync::Mutex<Reading>>,
}

/// One read of the transport.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reading {
    /// The play position, in seconds.
    pub at: f64,
    pub playing: bool,
    pub looping: bool,
    pub recording: bool,
    /// The tempo at the play position.
    pub bpm: f64,
}

/// The one poller every panel reads — see [`Transport::shared`].
static SHARED: std::sync::OnceLock<Option<Transport>> = std::sync::OnceLock::new();

impl Transport {
    /// Start polling. `None` if the facade is not up.
    #[must_use]
    #[cfg(not(feature = "native"))]
    pub fn start() -> Option<Self> {
        // The browser: no thread to poll on, so a task on the page's
        // event loop, at about a frame.
        daw::rpc::Daw::try_get()?;
        let state = std::sync::Arc::new(std::sync::Mutex::new(Reading::default()));
        let writer = std::sync::Arc::clone(&state);
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                if let Some(mut read) = read_transport().await
                    && let Ok(mut slot) = writer.lock()
                {
                    // The engine renders ahead of the device; the playhead
                    // drawn is the one heard.
                    #[cfg(feature = "web")]
                    if read.playing {
                        read.at = (read.at - crate::web_audio::latency_seconds()).max(0.0);
                    }
                    *slot = read;
                }
                gloo_timers::future::TimeoutFuture::new(16).await;
            }
        });
        Some(Self { state })
    }

    /// Start polling. `None` if the facade is not up.
    #[cfg(feature = "native")]
    #[must_use]
    pub fn start() -> Option<Self> {
        let runtime = crate::open::runtime()?;
        let state = std::sync::Arc::new(std::sync::Mutex::new(Reading::default()));
        let writer = std::sync::Arc::clone(&state);
        std::thread::Builder::new()
            .name("session-daw-transport".into())
            .spawn(move || {
                loop {
                    let read = runtime.block_on(read_transport());
                    if let Some(read) = read {
                        if let Ok(mut slot) = writer.lock() {
                            *slot = read;
                        }
                    }
                    // Faster than an audio block, slower than a frame:
                    // polling per frame would ask the engine 240 times
                    // a second for a number that changes 40 times, and
                    // the extrapolation exists precisely so it does not
                    // have to be asked more often than it moves.
                    std::thread::sleep(std::time::Duration::from_millis(8));
                }
            })
            .ok()?;
        Some(Self { state })
    }

    /// The process's poller, started on first use. Every panel reads this
    /// one rather than starting its own: a transport is one thing, and a
    /// poll thread per panel asks the engine the same question N times.
    /// `None` if the facade is not up.
    #[must_use]
    pub fn shared() -> Option<&'static Self> {
        SHARED.get_or_init(Self::start).as_ref()
    }

    /// The last position read, and whether it is moving.
    #[must_use]
    pub fn read(&self) -> (f64, bool) {
        let reading = self.reading();
        (reading.at, reading.playing)
    }

    /// Everything the last poll read.
    #[must_use]
    pub fn reading(&self) -> Reading {
        self.state
            .lock()
            .map_or_else(|_| Reading::default(), |slot| *slot)
    }
}

/// Whether the transport is moving (playing or recording): what keeps the
/// panels drawing frames with no hand on them.
#[must_use]
pub fn moving() -> bool {
    Transport::shared().is_some_and(|t| {
        let reading = t.reading();
        reading.playing || reading.recording
    })
}

/// What the transport has been asked to do.
///
/// A command, not a state: the engine owns whether it is playing, and
/// this window asks. Sending "play" while it is already playing has to
/// be harmless, which is why these are the engine's own verbs rather
/// than a boolean this side would have to keep in step.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Move {
    /// Play if stopped, stop if playing.
    PlayStop,
    /// Back to the start, whether or not it is moving.
    Home,
    /// Move the play position to a time — what a click on the ruler
    /// means once the transport is following it.
    Seek,
    /// Stop, wherever it is.
    Stop,
    /// To the end of the project.
    End,
    /// Loop on or off.
    ToggleLoop,
    /// Record on or off.
    ToggleRecord,
    /// Move to a time and play from it — one command, in order, so the
    /// play cannot land before the move (what rolling into the next song
    /// of a set does).
    PlayFrom,
}

/// Send a transport command, off the event loop.
///
/// Fire and forget, like [`Applier`]: the transport's own poll is what
/// tells the window what happened, so waiting for the call to return
/// would be waiting for news the window is already subscribed to.
pub fn transport(command: Move, seconds: f64) {
    // A press by a person: playing together, everyone's.
    #[cfg(any(feature = "native", feature = "web"))]
    if !matches!(command, Move::ToggleLoop | Move::ToggleRecord) {
        crate::collab::transport_pressed();
    }
    // Record is an action (`TRANSPORT_RECORD`): the button and the key
    // run what a model runs — off the event loop, as the rest are, since
    // a stop writes the takes. A window with no song of its own (a
    // remote) sends it to the engine it talks to.
    #[cfg(feature = "native")]
    if matches!(command, Move::ToggleRecord) && crate::open::local_engine().is_some() {
        let _ = std::thread::Builder::new()
            .name("session-daw-record".into())
            .spawn(crate::record::toggle_through_the_list);
        return;
    }
    transport_following(command, seconds);
}

/// [`transport`], for the shared transport moving this engine: not a press
/// here, so not sent back to the others.
pub fn transport_following(command: Move, seconds: f64) {
    #[cfg(not(feature = "native"))]
    wasm_bindgen_futures::spawn_local(run_transport(command, seconds));
    #[cfg(feature = "native")]
    {
        let Some(runtime) = crate::open::runtime() else {
            return;
        };
        std::thread::Builder::new()
            .name("session-daw-transport-cmd".into())
            .spawn(move || runtime.block_on(run_transport(command, seconds)))
            .ok();
    }
}

/// A transport command, against the engine.
async fn run_transport(command: Move, seconds: f64) {
    let Some(daw) = daw::rpc::Daw::try_get() else {
        return;
    };
    let Ok(project) = daw.current_project().await else {
        return;
    };
    let transport = project.transport();
    let outcome = match command {
        Move::PlayStop => {
            let playing = transport.is_playing().await.unwrap_or(false);
            let at = transport.get_position().await.unwrap_or(0.0);
            let behaviour = crate::play_return::get();
            let step = crate::play_return::with_locate(|l| l.press(behaviour, playing, at));
            run_step(&transport, step.unwrap_or(crate::play_return::Step::Stop)).await
        }
        Move::Home => transport.goto_start().await,
        Move::Seek => {
            let playing = transport.is_playing().await.unwrap_or(false);
            crate::play_return::with_locate(|l| l.moved(playing, seconds.max(0.0)));
            transport.set_position(seconds.max(0.0)).await
        }
        Move::Stop => {
            let playing = transport.is_playing().await.unwrap_or(false);
            let behaviour = crate::play_return::get();
            let step = crate::play_return::with_locate(|l| l.stop(behaviour, playing));
            run_step(&transport, step.unwrap_or(crate::play_return::Step::Stop)).await
        }
        Move::End => transport.goto_end().await,
        Move::ToggleLoop => transport.toggle_loop().await,
        Move::ToggleRecord => transport.toggle_recording().await,
        Move::PlayFrom => {
            crate::play_return::with_locate(|l| l.started_from(seconds.max(0.0)));
            match transport.set_position(seconds.max(0.0)).await {
                Ok(()) => transport.play().await,
                Err(error) => Err(error),
            }
        }
    };
    if let Err(error) = outcome {
        tracing::warn!(error = %error, command = ?command, "the transport refused");
    }
}

/// What [`crate::play_return`] decided a play/stop press does.
async fn run_step(
    transport: &daw_control::Transport,
    step: crate::play_return::Step,
) -> daw_control::Result<()> {
    match step {
        crate::play_return::Step::Play => transport.play().await,
        crate::play_return::Step::Stop => transport.stop().await,
        crate::play_return::Step::StopAt(to) => {
            transport.stop().await?;
            transport.set_position(to).await
        }
    }
}

/// One read of the transport.
async fn read_transport() -> Option<Reading> {
    let daw = daw::rpc::Daw::try_get()?;
    let project = daw.current_project().await.ok()?;
    let transport = project.transport();
    let at = transport.get_position().await.ok()?;
    let playing = transport.is_playing().await.ok()?;
    Some(Reading {
        at,
        playing,
        looping: transport.is_looping().await.unwrap_or(false),
        recording: transport.is_recording().await.unwrap_or(false),
        bpm: transport.get_tempo().await.unwrap_or(0.0),
    })
}

/// A dB value as a linear gain. `Track::volume`'s unit.
#[must_use]
pub fn db_to_gain(db: f64) -> f64 {
    // Silence rather than an infinitesimal gain: the bottom of the
    // fader means off, and a gain of 1e-14 is off with extra steps.
    if db <= daw_theme_art::paint::tcp::FADER_BOTTOM_DB {
        return 0.0;
    }
    10.0_f64.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::Control;

    fn track(volume: f64, pan: f64) -> daw_proto::Track {
        daw_proto::Track {
            volume,
            pan,
            ..daw_proto::Track::default()
        }
    }

    /// The point of the queue: a hundred fader positions are one fader
    /// position by the time anything is sent.
    #[test]
    fn a_drag_collapses_to_its_latest_value() {
        let mut q = Queue::default();
        for i in 1..=100 {
            q.push(Edit::SetVolume("kick".into(), f64::from(i) / 100.0));
        }
        assert_eq!(q.len(), 1);
        assert_eq!(q.pop(), Some(Edit::SetVolume("kick".into(), 1.0)));
    }

    /// A move of an item before a split of it and another after are two
    /// moves, in their order: collapsed, the later position would land
    /// before the split it was made after.
    #[test]
    fn coalescing_never_jumps_a_split() {
        let mut q = Queue::default();
        q.push(Edit::MoveItem("a".into(), 1.0));
        q.push(Edit::SplitItem("a".into(), 2.0, "a-right".into()));
        q.push(Edit::MoveItem("a".into(), 3.0));
        q.push(Edit::MoveItem("a".into(), 4.0));
        assert_eq!(
            q.len(),
            3,
            "the moves after the split collapse, not across it"
        );
        assert_eq!(q.pop(), Some(Edit::MoveItem("a".into(), 1.0)));
        assert!(matches!(q.pop(), Some(Edit::SplitItem(..))));
        assert_eq!(q.pop(), Some(Edit::MoveItem("a".into(), 4.0)));
    }

    /// A kit's slip edit is a hundred thousand edits at once: queued in a
    /// moment, not minutes.
    #[test]
    fn a_hundred_thousand_edits_queue_at_once() {
        let mut q = Queue::default();
        let started = std::time::Instant::now();
        for i in 0..100_000_u32 {
            let item = format!("item{}", i / 8);
            q.push(Edit::SplitItem(
                item.clone(),
                f64::from(i),
                format!("{item}-{i}"),
            ));
            q.push(Edit::MoveItem(item, f64::from(i)));
        }
        assert!(
            started.elapsed().as_secs_f64() < 2.0,
            "{:?}",
            started.elapsed()
        );
        assert_eq!(q.len(), 200_000);
    }

    /// But two mutes are two mutes. Collapsing them would eat half of a
    /// double-click, and a mute that toggles once for two clicks is
    /// worse than one that lags.
    #[test]
    fn clicks_never_collapse() {
        let mut q = Queue::default();
        q.push(Edit::ToggleMute("kick".into()));
        q.push(Edit::ToggleMute("kick".into()));
        assert_eq!(q.len(), 2);
    }

    /// Coalescing is per track AND per control: dragging the kick's
    /// fader must not swallow the snare's.
    #[test]
    fn coalescing_does_not_cross_tracks_or_controls() {
        let mut q = Queue::default();
        q.push(Edit::SetVolume("kick".into(), 0.5));
        q.push(Edit::SetVolume("snare".into(), 0.5));
        q.push(Edit::SetPan("kick".into(), 0.2));
        assert_eq!(q.len(), 3);
        q.push(Edit::SetVolume("kick".into(), 0.9));
        assert_eq!(q.len(), 3, "the kick's volume should have been replaced");
    }

    /// And per SEND. Two sends on one track are two controls: without
    /// the number in the identity, dragging the reverb send would eat
    /// the delay send's last position out of the queue, and the delay
    /// would end up wherever the reverb was left.
    #[test]
    fn coalescing_does_not_cross_sends() {
        let mut q = Queue::default();
        let send = |index| crate::routes::At::new(daw_proto::routing::RouteType::Send, index);
        q.push(Edit::SetRouteVolume("kick".into(), send(0), 0.5));
        q.push(Edit::SetRouteVolume("kick".into(), send(1), 0.5));
        assert_eq!(q.len(), 2);
        q.push(Edit::SetRouteVolume("kick".into(), send(0), 0.9));
        assert_eq!(q.len(), 2, "send 0 should have been replaced");
        assert_eq!(
            q.pop(),
            Some(Edit::SetRouteVolume("kick".into(), send(0), 0.9))
        );
        assert_eq!(
            q.pop(),
            Some(Edit::SetRouteVolume("kick".into(), send(1), 0.5))
        );

        // And a receive numbered the same as a send is not that send:
        // one track has send 0, receive 0 and output 0 at once, and
        // three different things they are.
        let mut q = Queue::default();
        q.push(Edit::SetRouteVolume("kick".into(), send(0), 0.5));
        q.push(Edit::SetRouteVolume(
            "kick".into(),
            crate::routes::At::new(daw_proto::routing::RouteType::Receive, 0),
            0.5,
        ));
        assert_eq!(q.len(), 2, "a receive collapsed into a send");
    }

    /// Taking a send away is a click, not a drag: two removes are two
    /// removes, because the second one is about a different send —
    /// removing renumbers everything after it.
    #[test]
    fn removing_a_send_never_collapses() {
        let mut q = Queue::default();
        let send = crate::routes::At::new(daw_proto::routing::RouteType::Send, 0);
        q.push(Edit::RemoveRoute("kick".into(), send));
        q.push(Edit::RemoveRoute("kick".into(), send));
        assert_eq!(q.len(), 2);
    }

    /// Order is kept for everything that did not coalesce — a mute
    /// before a solo must arrive before it.
    #[test]
    fn order_survives() {
        let mut q = Queue::default();
        q.push(Edit::ToggleMute("a".into()));
        q.push(Edit::ToggleSolo("a".into()));
        assert_eq!(q.pop(), Some(Edit::ToggleMute("a".into())));
        assert_eq!(q.pop(), Some(Edit::ToggleSolo("a".into())));
        assert!(q.is_empty());
    }

    /// A fader drag moves along the SCALE, not along the gain. A third
    /// of the way up is a third of the way up the dB scale — up near
    /// the top a third of the gain would be most of the range, and down
    /// near the bottom almost none of it.
    #[test]
    fn a_fader_drag_moves_in_decibels() {
        let unity = track(1.0, 0.0);
        let Some(Edit::SetVolume(_, gain)) = drag(Control::Volume, "k", &unity, -0.25) else {
            panic!("expected a volume edit");
        };
        let db = 20.0 * gain.log10();
        let span =
            daw_theme_art::paint::tcp::FADER_TOP_DB - daw_theme_art::paint::tcp::FADER_BOTTOM_DB;
        assert!(
            (db + span * 0.25).abs() < 0.01,
            "a quarter of the travel should be a quarter of the scale: {db} dB"
        );
    }

    /// The ends hold. A fader cannot be dragged past its ceiling or
    /// below silence however far the pointer goes.
    ///
    /// The ceiling is +12 dB, not unity: a fader that cannot add gain
    /// makes you reach for a plugin to do the most ordinary thing in a
    /// mix. In linear gain that is about 3.98.
    #[test]
    fn a_fader_stops_at_both_ends() {
        let unity = track(1.0, 0.0);
        let Some(Edit::SetVolume(_, up)) = drag(Control::Volume, "k", &unity, 5.0) else {
            panic!("expected a volume edit");
        };
        let ceiling = 10.0_f64.powf(daw_theme_art::paint::tcp::FADER_TOP_DB / 20.0);
        assert!(
            (up - ceiling).abs() < 1e-6,
            "should have stopped at +12 dB: {up}"
        );

        let Some(Edit::SetVolume(_, down)) = drag(Control::Volume, "k", &unity, -5.0) else {
            panic!("expected a volume edit");
        };
        assert!(down.abs() < 1e-9, "should have reached silence: {down}");
    }

    /// The bottom of the fader is OFF, not a gain too small to hear.
    #[test]
    fn the_bottom_of_the_fader_is_silence() {
        assert!(db_to_gain(daw_theme_art::paint::tcp::FADER_BOTTOM_DB).abs() < f64::EPSILON);
        assert!((db_to_gain(0.0) - 1.0).abs() < 1e-9);
    }

    /// Pan is two-sided and clamps at both ends.
    #[test]
    fn pan_runs_from_hard_left_to_hard_right() {
        let centred = track(1.0, 0.0);
        let Some(Edit::SetPan(_, right)) = drag(Control::Pan, "k", &centred, 9.0) else {
            panic!("expected a pan edit");
        };
        assert!((right - 1.0).abs() < f64::EPSILON);
        let Some(Edit::SetPan(_, left)) = drag(Control::Pan, "k", &centred, -9.0) else {
            panic!("expected a pan edit");
        };
        assert!((left + 1.0).abs() < f64::EPSILON);
    }

    /// The FX button opens a chain window, and there is neither a
    /// chain nor a window — see `bin/chain-probe`. It produces no edit
    /// rather than an invented one.
    #[test]
    fn the_fx_button_waits_for_a_chain() {
        assert!(click(Control::Fx, "k", &track(1.0, 0.0), false).is_none());
        assert!(drag(Control::Mute, "k", &track(1.0, 0.0), 0.5).is_none());
    }

    /// Routing is not an edit: it opens the panel, and the parent send
    /// it used to toggle is a row in there. A click that both opened a
    /// panel and changed the routing would change something you had
    /// not read yet.
    #[test]
    fn routing_opens_the_panel_rather_than_editing() {
        let mut sending = track(1.0, 0.0);
        sending.parent_send = true;
        assert_eq!(click(Control::Routing, "k", &sending, false), None);
        sending.parent_send = false;
        assert_eq!(click(Control::Routing, "k", &sending, false), None);
    }

    /// Clicking a name selects; renaming is a different gesture.
    #[test]
    fn clicking_a_name_selects_it() {
        assert_eq!(
            click(Control::Name, "k", &track(1.0, 0.0), false),
            Some(Edit::Select("k".into())),
            "a plain click selects one track"
        );
        assert_eq!(
            click(Control::Name, "k", &track(1.0, 0.0), true),
            Some(Edit::AddToSelection("k".into())),
            "the modifier adds to the selection instead"
        );
    }
}

/// Applies edits to the engine, off the event loop.
///
/// The window's thread must not block: a `set_volume` that waited for
/// the engine would stall the frame, and stalling the frame while
/// dragging a fader is the one moment a user notices a stall most. So
/// edits go down a channel and a worker applies them.
///
/// One worker, not a task per edit, so edits arrive in the order they
/// were made. Two mutes racing would be a mute that ends up in the
/// wrong state and no way to tell which.
pub struct Applier {
    #[cfg(feature = "native")]
    edits: std::sync::mpsc::Sender<Edit>,
    /// The browser's: a queue, and whether a task is draining it.
    #[cfg(not(feature = "native"))]
    queue: std::rc::Rc<std::cell::RefCell<(Queue, bool)>>,
}

impl Applier {
    /// Start the worker. `None` if the facade is not up.
    #[cfg(not(feature = "native"))]
    #[must_use]
    pub fn start() -> Option<Self> {
        daw::rpc::Daw::try_get()?;
        Some(Self {
            queue: std::rc::Rc::default(),
        })
    }

    /// Send an edit: queued, and drained in order by one task, so edits
    /// land in the order they were made (and coalesce, as natively).
    #[cfg(not(feature = "native"))]
    pub fn send(&self, edit: Edit) {
        let mut slot = self.queue.borrow_mut();
        slot.0.push(edit);
        if slot.1 {
            return;
        }
        slot.1 = true;
        let queue = std::rc::Rc::clone(&self.queue);
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                let next = {
                    let mut slot = queue.borrow_mut();
                    let next = slot.0.pop();
                    if next.is_none() {
                        slot.1 = false;
                    }
                    next
                };
                let Some(edit) = next else { break };
                apply_reported(&edit).await;
            }
        });
    }

    /// Start the worker. `None` if the facade is not up, in which case
    /// the window runs read-only rather than pretending to edit.
    #[cfg(feature = "native")]
    #[must_use]
    pub fn start() -> Option<Self> {
        let runtime = crate::open::runtime()?;
        let (tx, rx) = std::sync::mpsc::channel::<Edit>();
        std::thread::Builder::new()
            .name("session-daw-edits".into())
            .spawn(move || {
                // Drain, coalesce, apply. Draining first is what makes
                // the coalescing work: a hundred queued fader positions
                // become one before any of them is sent.
                let mut queue = Queue::default();
                while let Ok(first) = rx.recv() {
                    queue.push(first);
                    while let Ok(more) = rx.try_recv() {
                        queue.push(more);
                    }
                    let mut items = false;
                    while let Some(edit) = queue.pop() {
                        items |= edit.is_item();
                        runtime.block_on(apply_reported(&edit));
                    }
                    // What the engine plays now is what the window drew:
                    // the tracks' detail and the hit markers read again.
                    if items {
                        crate::wave_detail::engine_changed();
                        crate::hit_overlay::refresh();
                    }
                }
            })
            .ok()?;
        Some(Self { edits: tx })
    }

    /// Send an edit. Dropped if the worker has gone, because a window
    /// that could not edit is better than one that panics trying.
    #[cfg(feature = "native")]
    pub fn send(&self, edit: Edit) {
        let _ = self.edits.send(edit);
    }
}

/// The items the window named before the engine made them: its guid for
/// a split's right half or a copy, and the real one. Edits run one at a
/// time, in order (see `Engine::send`), so a name is always remembered
/// before an edit that uses it.
static MADE: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> =
    std::sync::Mutex::new(None);

fn remember(predicted: &str, real: &str) {
    MADE.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_or_insert_with(std::collections::HashMap::new)
        .insert(predicted.to_owned(), real.to_owned());
}

/// The real guid of an item the window may have named itself: the
/// engine's own name for a piece the window split off and named, until the
/// song is read back — what anything asking the engine about the window's
/// items (a waveform's detail) asks it by.
pub(crate) fn made(guid: &str) -> String {
    MADE.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .and_then(|m| m.get(guid).cloned())
        .unwrap_or_else(|| guid.to_owned())
}

/// Set the stretch markers of the item `guid`'s active take, on this
/// window's own engine — the one place a take's markers are written from
/// here (see `Edit::SetStretchMarkers`). Nothing to do without one.
fn stretch(guid: &str, markers: &[(f64, f64)]) {
    #[cfg(feature = "native")]
    {
        use daw_proto::StretchMarkers as _;
        let (Some(daw), Some(song)) = (crate::open::local_engine(), crate::open::current_song())
        else {
            return;
        };
        let take = daw_proto::StretchTakeRef::new(
            daw::service::ProjectContext::Project(song),
            daw::service::ItemRef::Guid(guid.to_owned()),
            daw::service::TakeRef::Active,
        );
        let markers = markers
            .iter()
            .map(|&(at, source)| daw_proto::StretchMarker::new(at, source))
            .collect();
        if let Err(error) = daw.set_stretch_markers(take, markers) {
            tracing::warn!(error = %error, "the engine refused the stretch markers");
        }
    }
    #[cfg(not(feature = "native"))]
    let _ = (guid, markers);
}

/// Split `item` at `split_at`, keeping the audio on both sides (see
/// `Edit::SplitItem`).
async fn split(item: &daw::rpc::ItemHandle, split_at: f64, new_guid: &str) -> daw::rpc::Result<()> {
    let secs = daw_proto::primitives::Duration::from_seconds;
    let info = item.info().await?;
    let start = info.position.as_seconds();
    let end = start + info.length.as_seconds();
    if split_at <= start || split_at >= end {
        return Ok(());
    }
    let right = item.duplicate().await?;
    remember(new_guid, right.guid());
    right
        .set_position(daw_proto::primitives::PositionInSeconds::from_seconds(
            split_at,
        ))
        .await?;
    right.set_length(secs(end - split_at)).await?;
    right
        .takes()
        .active()
        .await?
        .set_start_offset(secs(info.start_offset.as_seconds() + (split_at - start)))
        .await?;
    right.set_fade_in(secs(0.0), info.fade_in_shape).await?;
    item.set_fade_out(secs(0.0), info.fade_out_shape).await?;
    item.set_length(secs(split_at - start)).await
}

/// An item edit, applied to this window's own engine directly (its
/// services are synchronous in-process); `false` for anything else, or
/// with no song open here, which then goes through the facade. The same
/// edits the facade path makes, call for call.
#[cfg(feature = "native")]
fn apply_locally(edit: &Edit) -> bool {
    use daw_proto::{ItemRef, Items as _, TakeRef, Takes as _};
    if !edit.is_item() {
        return false;
    }
    let (Some(daw), Some(song)) = (crate::open::local_engine(), crate::open::current_song()) else {
        return false;
    };
    let project = daw_proto::ProjectContext::Project(song);
    let secs = daw_proto::primitives::Duration::from_seconds;
    let at = daw_proto::primitives::PositionInSeconds::from_seconds;
    let outcome = match edit {
        Edit::DeselectAllItems(_) => daw.select_all_items(project, false),
        Edit::SelectAllItems(_) => daw.select_all_items(project, true),
        _ => {
            let guid = made(edit.guid());
            let item = ItemRef::Guid(guid.clone());
            let Some(info) = daw.get_item(project.clone(), item.clone()) else {
                return true;
            };
            match edit {
                Edit::SetFadeIn(_, s, shape) => daw.set_fade_in(project, item, secs(*s), *shape),
                Edit::SetFadeOut(_, s, shape) => daw.set_fade_out(project, item, secs(*s), *shape),
                Edit::SelectItem(_, exclusive) => {
                    if *exclusive {
                        let _ = daw.select_all_items(project.clone(), false);
                    }
                    daw.set_selected(project, item, true)
                }
                Edit::DeselectItem(_) => daw.set_selected(project, item, false),
                Edit::SlipItem(_, offset) => {
                    daw.set_start_offset(project, item, TakeRef::Active, secs(*offset))
                }
                Edit::MoveItem(_, position) => daw.set_position(project, item, at(*position)),
                Edit::TrimItem(_, position, length) => daw
                    .set_position(project.clone(), item.clone(), at(*position))
                    .and_then(|()| daw.set_length(project, item, secs(*length))),
                Edit::SplitItem(_, split_at, new_guid) => {
                    let start = info.position.as_seconds();
                    let end = start + info.length.as_seconds();
                    if *split_at <= start || *split_at >= end {
                        return true;
                    }
                    let Some(right) = daw.duplicate_item(project.clone(), item.clone()) else {
                        return true;
                    };
                    remember(new_guid, &right);
                    let half = ItemRef::Guid(right);
                    daw.set_position(project.clone(), half.clone(), at(*split_at))
                        .and_then(|()| {
                            daw.set_length(project.clone(), half.clone(), secs(end - split_at))
                        })
                        .and_then(|()| {
                            daw.set_start_offset(
                                project.clone(),
                                half.clone(),
                                TakeRef::Active,
                                secs(info.start_offset.as_seconds() + (split_at - start)),
                            )
                        })
                        .and_then(|()| {
                            daw.set_fade_in(project.clone(), half, secs(0.0), info.fade_in_shape)
                        })
                        .and_then(|()| {
                            daw.set_fade_out(
                                project.clone(),
                                item.clone(),
                                secs(0.0),
                                info.fade_out_shape,
                            )
                        })
                        .and_then(|()| daw.set_length(project, item, secs(split_at - start)))
                }
                Edit::CopyItem(_, to, new_guid) => daw
                    .duplicate_item(project.clone(), item)
                    .map_or(Ok(()), |copy| {
                        remember(new_guid, &copy);
                        daw.set_position(project, ItemRef::Guid(copy), at(*to))
                    }),
                Edit::SetItemLane(_, lane) => daw.set_fixed_lane(project, item, *lane),
                Edit::SetItemVolume(_, volume) => {
                    daw_proto::Items::set_volume(daw, project, item, *volume)
                }
                Edit::SetStretchMarkers(_, markers) => {
                    stretch(&guid, markers);
                    Ok(())
                }
                Edit::SetSnapOffset(_, s) => daw.set_snap_offset(project, item, secs(*s)),
                Edit::SetTakeVolume(_, points) => {
                    take_volume_locally(daw, &project, &guid, points);
                    Ok(())
                }
                Edit::DeleteItem(_) => daw.delete_item(project, item),
                _ => return false,
            }
        }
    };
    if let Err(error) = outcome {
        tracing::warn!(error = %error, edit = ?edit, "the engine refused an item edit");
    }
    true
}

/// An automation edit, through the facade: the envelope by its track and
/// reference, its points replaced whole.
async fn apply_automation(project: &daw_control::Project, edit: &Edit) {
    let Ok(Some(track)) = project.tracks().by_guid(edit.guid()).await else {
        return;
    };
    let outcome = match edit {
        Edit::SetEnvelope(_, target, points) => {
            let envelope = track.envelopes().of(target.clone());
            match envelope.clear().await {
                Ok(()) => {
                    let mut done = Ok(());
                    for p in points {
                        if let Err(error) = envelope.add_point(p.time, p.value, p.shape).await {
                            done = Err(error);
                            break;
                        }
                    }
                    done
                }
                Err(error) => Err(error),
            }
        }
        Edit::SetItemCurve(_, target, item, points) => {
            track
                .envelopes()
                .of(target.clone())
                .set_automation_item_points(*item, points.clone())
                .await
        }
        Edit::MoveAutomationItem(_, target, item, at) => {
            let mut params = daw_proto::automation::SetAutomationItemParams::at(*item);
            params.position = Some(daw_proto::primitives::PositionInSeconds::from_seconds(*at));
            track
                .envelopes()
                .of(target.clone())
                .set_automation_item(params)
                .await
        }
        Edit::AddAutomationItem(_, target, (at, length, pool), curve) => {
            let envelope = track.envelopes().of(target.clone());
            let (at, length) = (
                daw_proto::primitives::PositionInSeconds::from_seconds(*at),
                daw_proto::primitives::Duration::from_seconds(*length),
            );
            let made = if *pool >= 0 {
                envelope.add_pooled_automation_item(*pool, at, length).await
            } else {
                envelope.add_automation_item(at, length).await
            };
            let outcome = match made {
                Ok(index) if *pool < 0 && !curve.is_empty() => {
                    envelope
                        .set_automation_item_points(index, curve.clone())
                        .await
                }
                Ok(_) => Ok(()),
                Err(error) => Err(error),
            };
            // The window drew a stand-in: the engine's own, read back.
            #[cfg(feature = "native")]
            crate::studio::request_resync();
            outcome
        }
        Edit::SetAutomationItemBounds(_, target, index, (at, length, offset)) => {
            let mut params = daw_proto::automation::SetAutomationItemParams::at(*index);
            params.position = Some(daw_proto::primitives::PositionInSeconds::from_seconds(*at));
            params.length = Some(daw_proto::primitives::Duration::from_seconds(*length));
            params.start_offset = Some(daw_proto::primitives::Duration::from_seconds(*offset));
            track
                .envelopes()
                .of(target.clone())
                .set_automation_item(params)
                .await
        }
        Edit::DeleteAutomationItem(_, target, index) => {
            let outcome = track
                .envelopes()
                .of(target.clone())
                .delete_automation_item(*index)
                .await;
            // The items after it are numbered again.
            #[cfg(feature = "native")]
            crate::studio::request_resync();
            outcome
        }
        Edit::ShowEnvelope(_, target, shown) => {
            track
                .envelopes()
                .of(target.clone())
                .set_visible(*shown)
                .await
        }
        Edit::SetEnvelopeMode(_, target, mode) => {
            track
                .envelopes()
                .of(target.clone())
                .set_automation_mode(*mode)
                .await
        }
        Edit::SetLaneHeight(_, target, height) => {
            track
                .envelopes()
                .of(target.clone())
                .show_in_lane(*height)
                .await
        }
        _ => Ok(()),
    };
    if let Err(error) = outcome {
        tracing::warn!(error = %error, "the engine refused an automation edit");
    }
}

/// An item's take volume replaced by `points` in this process's engine —
/// and its waveform, drawn through it, read again.
#[cfg(feature = "native")]
fn take_volume_locally(
    daw: &daw::standalone::Standalone,
    project: &daw_proto::ProjectContext,
    item: &str,
    points: &[daw_proto::automation::EnvelopePoint],
) {
    use daw_proto::automation::{Automation as _, TakeEnvelopeKind, TimeRangeParams};
    use daw_proto::{ItemRef, Takes as _};
    let Some(take) = daw.get_active_take(project.clone(), ItemRef::Guid(item.to_owned())) else {
        return;
    };
    let at = daw_proto::EnvelopeLocation::new(
        daw_proto::TrackRef::Index(0),
        daw_proto::EnvelopeRef::Take {
            item_guid: item.to_owned(),
            take_guid: take.guid,
            kind: TakeEnvelopeKind::Volume,
        },
    );
    daw.delete_points_in_range(
        project.clone(),
        at.clone(),
        TimeRangeParams::new(
            daw_proto::primitives::PositionInSeconds::from_seconds(f64::MIN / 4.0),
            daw_proto::primitives::PositionInSeconds::from_seconds(f64::MAX / 4.0),
        ),
    );
    for p in points {
        daw.add_point(
            project.clone(),
            at.clone(),
            daw_proto::automation::AddPointParams::new(p.time, p.value, p.shape),
        );
    }
    crate::midi::mark_stale([item.to_owned()]);
    crate::studio::request_resync();
}

/// One edit, against the engine, waited for: what a test or a tool that
/// needs the result before going on does instead of the queue. False when
/// there is no runtime to run it on.
#[cfg(feature = "native")]
#[must_use]
pub fn apply_now(edit: &Edit) -> bool {
    crate::open::runtime().is_some_and(|runtime| {
        runtime.block_on(apply_reported(edit));
        true
    })
}

/// One edit, against the engine — then said to the Action List, so
/// whoever watches the app (a model, through `architect-mcp`'s `watch`)
/// hears the session change as it changes.
async fn apply_reported(edit: &Edit) {
    apply(edit).await;
    #[cfg(feature = "native")]
    if !matches!(edit, Edit::BeginUndo(_) | Edit::EndUndo(_)) {
        let topic = if edit.is_ruler() {
            "markers"
        } else if edit.is_item() {
            "items"
        } else {
            "tracks"
        };
        crate::actions::list().changed(topic, edit.guid());
    }
}

/// One edit, against the engine.
async fn apply(edit: &Edit) {
    // An item edit on this window's own engine, straight to it: through
    // the facade each is a dozen round trips (a split is a lookup, a
    // duplicate, a move, a length, an offset and two fades), and a kit's
    // slip edit is a hundred thousand edits — half an hour, through it.
    #[cfg(feature = "native")]
    if apply_locally(edit) {
        return;
    }
    let Some(daw) = daw::rpc::Daw::try_get() else {
        return;
    };
    let Ok(project) = daw.current_project().await else {
        return;
    };
    // The brackets first: they are about the history rather than about
    // anything in the session, so they belong to neither the item path
    // nor the track one.
    match edit {
        Edit::BeginUndo(label) => {
            if let Err(error) = project.begin_undo_block(label).await {
                tracing::warn!(error = %error, "the engine refused to open an undo step");
            }
            return;
        }
        Edit::EndUndo(label) => {
            if let Err(error) = project.end_undo_block(label).await {
                tracing::warn!(error = %error, "the engine refused to close an undo step");
            }
            return;
        }
        _ => {}
    }
    if edit.is_automation() {
        apply_automation(&project, edit).await;
        return;
    }
    if edit.is_item() {
        if let Edit::DeselectAllItems(_) = edit {
            if let Err(error) = project.items().deselect_all().await {
                tracing::warn!(error = %error, "the engine refused to deselect");
            }
            return;
        }
        if let Edit::SelectAllItems(_) = edit {
            if let Err(error) = project.items().select_all().await {
                tracing::warn!(error = %error, "the engine refused to select all");
            }
            return;
        }
        let Ok(Some(item)) = project.items().by_guid(&made(edit.guid())).await else {
            return;
        };
        let secs = daw_proto::primitives::Duration::from_seconds;
        let at = daw_proto::primitives::PositionInSeconds::from_seconds;
        let outcome = match edit {
            Edit::SetFadeIn(_, s, shape) => item.set_fade_in(secs(*s), *shape).await,
            Edit::SetFadeOut(_, s, shape) => item.set_fade_out(secs(*s), *shape).await,
            Edit::SelectItem(_, exclusive) => {
                if *exclusive && let Err(error) = project.items().deselect_all().await {
                    tracing::warn!(error = %error, "the engine refused to deselect");
                }
                item.select().await
            }
            // The active take's offset into its source. Addressed
            // through the take because that is where the offset lives —
            // `Item::start_offset` is the read side of the same value.
            Edit::SlipItem(_, offset) => match item.takes().active().await {
                Ok(take) => take.set_start_offset(secs(*offset)).await,
                Err(error) => Err(error),
            },
            Edit::DeselectItem(_) => item.deselect().await,
            Edit::MoveItem(_, position) => item.set_position(at(*position)).await,
            Edit::TrimItem(_, position, length) => match item.set_position(at(*position)).await {
                Ok(()) => item.set_length(secs(*length)).await,
                Err(error) => Err(error),
            },
            // The engine has no split of its own: the right half is the
            // item duplicated — source, take, fades and all — moved to
            // the cut and slipped as far into its source, and the left
            // is the item shortened. The window named the right half;
            // the name is remembered against the real one, so a later
            // edit (a quantize moves every slice it cut) finds it.
            Edit::SplitItem(_, split_at, new_guid) => split(&item, *split_at, new_guid).await,
            // The copy the engine makes is its own, with its own guid:
            // the one in the edit was the window's prediction, and it is
            // remembered against the real one for a later edit.
            Edit::CopyItem(_, to, new_guid) => match item.duplicate().await {
                Ok(copy) => {
                    remember(new_guid, copy.guid());
                    copy.set_position(at(*to)).await
                }
                Err(error) => Err(error),
            },
            Edit::SetItemLane(_, lane) => item.set_fixed_lane(*lane).await,
            Edit::SetItemVolume(_, volume) => item.set_volume(*volume).await,
            Edit::SetStretchMarkers(_, markers) => {
                stretch(item.guid(), markers);
                Ok(())
            }
            Edit::DeleteItem(_) => item.delete().await,
            Edit::SetTakeVolume(_, points) => match item.active_take().envelopes().await {
                Ok(envelopes) => {
                    let volume = envelopes.volume();
                    match volume.clear().await {
                        Ok(()) => {
                            for p in points {
                                if let Err(error) = volume.add_point(p.time, p.value, p.shape).await
                                {
                                    tracing::warn!(error = %error, "the engine refused a take volume point");
                                    break;
                                }
                            }
                            Ok(())
                        }
                        Err(error) => Err(error),
                    }
                }
                Err(error) => Err(error),
            },
            _ => Ok(()),
        };
        if let Err(error) = outcome {
            tracing::warn!(error = %error, edit = ?edit, "the engine refused an item edit");
        }
        return;
    }
    if edit.is_ruler() {
        let markers = project.markers();
        let regions = project.regions();
        let outcome = match edit {
            Edit::AddMarker(_, at, lane) => match markers.add(*at, "").await {
                // The lane is a second call because REAPER's add takes
                // no lane. A marker that landed in the wrong lane would
                // be a mark in the sections row, which is a section
                // that is not one.
                Ok(id) => markers.set_lane(id, Some(*lane)).await,
                Err(error) => Err(error),
            },
            Edit::MoveMarker(_, id, at) => markers.move_to(*id, at.max(0.0)).await,
            Edit::RenameMarker(_, id, name) => markers.rename(*id, name).await,
            Edit::RemoveMarker(_, id) => markers.remove(*id).await,
            Edit::AddRegion(_, from, to, lane) => {
                let (from, to) = ordered(*from, *to);
                match regions.add(from, to, "").await {
                    Ok(id) => regions.set_lane(id, Some(*lane)).await,
                    Err(error) => Err(error),
                }
            }
            Edit::SetRegionBounds(_, id, from, to) => {
                let (from, to) = ordered(*from, *to);
                regions.set_bounds(*id, from, to).await
            }
            Edit::RenameRegion(_, id, name) => regions.rename(*id, name).await,
            Edit::RemoveRegion(_, id) => regions.remove(*id).await,
            Edit::SetTempo(_, at, bpm) => set_tempo(&project, *at, *bpm).await,
            _ => Ok(()),
        };
        if let Err(error) = outcome {
            tracing::warn!(error = %error, edit = ?edit, "the engine refused a ruler edit");
        }
        return;
    }
    let Ok(Some(track)) = project.tracks().by_guid(edit.guid()).await else {
        // The track went away between the click and the apply — a
        // project reload, or another client removing it. Nothing to
        // report: the session simply no longer has what was clicked.
        return;
    };
    let outcome = match edit {
        Edit::ToggleMute(_) => track.toggle_mute().await,
        Edit::ToggleSolo(_) => track.toggle_solo().await,
        Edit::ToggleArm(_) => track.toggle_arm().await,
        Edit::Select(_) => track.select_exclusive().await,
        Edit::AddToSelection(_) => track.select().await,
        // r[impl automation.write]
        // Through the automation writer: it sets the fader, and records a
        // point where the envelope's mode (or the track's) says to —
        // Touch while held, Latch from the first touch, Write always —
        // while the song plays.
        Edit::SetVolume(_, v) => match track.envelopes().volume().write(*v).await {
            Ok(()) => Ok(()),
            Err(_) => track.set_volume(*v).await,
        },
        Edit::SetPan(_, p) => match track.envelopes().pan().write(p.mul_add(0.5, 0.5)).await {
            Ok(()) => Ok(()),
            Err(_) => track.set_pan(*p).await,
        },
        Edit::Touch(_, which, held) => {
            let envelope = match which {
                Touched::Volume => track.envelopes().volume(),
                Touched::Pan => track.envelopes().pan(),
            };
            if *held {
                envelope.touch().await
            } else {
                envelope.release().await
            }
        }
        Edit::Rename(_, name) => track.rename(name).await,
        Edit::SetPhase(_, inverted) => track.set_phase_inverted(*inverted).await,
        Edit::SetLanePlays(_, mask) => track.set_lane_play_mask(*mask).await,
        Edit::SetInputMonitor(_, mode) => track.set_input_monitor(*mode).await,
        Edit::SetParentSend(_, enabled) => track.set_parent_send(*enabled).await,
        Edit::SetFxParam(_, fx, index, value) => match track.fx_chain().by_name(fx).await {
            Ok(Some(fx)) => fx.param(*index).set(*value).await,
            other => other.map(|_| ()),
        },
        Edit::SetFxEnabled(_, fx, enabled) => match track.fx_chain().by_name(fx).await {
            Ok(Some(fx)) if *enabled => fx.enable().await,
            Ok(Some(fx)) => fx.disable().await,
            other => other.map(|_| ()),
        },
        // A send is addressed through the track that owns it, by the
        // number REAPER gave it. `by_index` costs a round trip of its
        // own to prove the route is still there — worth it, because
        // the alternative is writing a level into whatever now holds
        // that number after somebody removed the one before it.
        Edit::AddSend(_, dest) => track.sends().add_to(dest).await.map(|_| ()),
        Edit::RemoveRoute(_, at) => {
            on_route(&track, *at, |route| async move { route.remove().await }).await
        }
        Edit::SetRouteVolume(_, at, v) => {
            let v = *v;
            on_route(&track, *at, move |route| async move {
                route.set_volume(v).await
            })
            .await
        }
        Edit::SetRoutePan(_, at, p) => {
            let p = *p;
            on_route(&track, *at, move |route| async move { route.set_pan(p).await }).await
        }
        Edit::SetRouteMute(_, at, muted) => {
            let muted = *muted;
            on_route(&track, *at, move |route| async move {
                if muted { route.mute().await } else { route.unmute().await }
            })
            .await
        }
        Edit::SetSendMode(_, index, mode) => {
            let mode = *mode;
            let at = crate::routes::At::new(daw_proto::routing::RouteType::Send, *index);
            on_route(&track, at, move |route| async move {
                route.set_send_mode(mode).await
            })
            .await
        }
        Edit::SetColor(_, color) => track.set_color(*color).await,
        Edit::SetAutomationMode(_, mode) => track.set_automation_mode(*mode).await,
        Edit::SetRecordInput(_, input) => track.set_record_input(*input).await,
        Edit::SetVisibility(_, tcp, mixer) => track.set_visibility(*tcp, *mixer).await,
        Edit::SetHeight(_, pixels) => track.set_tcp_height(*pixels).await,
        Edit::SetFolderDepth(_, depth) => track.set_folder_depth(*depth).await,
        Edit::SetGroupMembership(_, slot, member) => {
            track.set_group_membership(*slot, *member).await
        }
        Edit::SetGroupFlags(_, slot, family, role) => {
            track.set_group_flags(*slot, *family, *role).await
        }
        Edit::SetGroupModifier(_, slot, modifier, on) => {
            track.set_group_modifier(*slot, *modifier, *on).await
        }
        Edit::SetEnvelope(..)
        | Edit::AddAutomationItem(..)
        | Edit::SetAutomationItemBounds(..)
        | Edit::DeleteAutomationItem(..)
        | Edit::SetItemCurve(..)
        | Edit::MoveAutomationItem(..)
        | Edit::ShowEnvelope(..)
        | Edit::SetEnvelopeMode(..)
        | Edit::SetLaneHeight(..)
        | Edit::SetFadeIn(..)
        | Edit::SetFadeOut(..)
        | Edit::SelectItem(..)
        | Edit::DeselectItem(_)
        | Edit::DeselectAllItems(_)
        | Edit::SelectAllItems(_)
        | Edit::MoveItem(..)
        | Edit::TrimItem(..)
        | Edit::SplitItem(..)
        | Edit::CopyItem(..)
        | Edit::SetItemLane(..)
        | Edit::SetItemVolume(..)
        | Edit::SetStretchMarkers(..)
        | Edit::SetSnapOffset(..)
        | Edit::SetTakeVolume(..)
        | Edit::SlipItem(..)
        | Edit::DeleteItem(_)
        // Already handled above, where they did not need a track.
        | Edit::AddMarker(..)
        | Edit::MoveMarker(..)
        | Edit::RenameMarker(..)
        | Edit::RemoveMarker(..)
        | Edit::AddRegion(..)
        | Edit::SetRegionBounds(..)
        | Edit::RenameRegion(..)
        | Edit::RemoveRegion(..)
        | Edit::SetTempo(..)
        // Already handled above, where they needed no track either.
        | Edit::BeginUndo(_)
        | Edit::EndUndo(_) => Ok(()),
    };
    if let Err(error) = outcome {
        // One line, because a failed edit is a thing the user did that
        // did not happen — silence here is how a mixer starts lying.
        tracing::warn!(error = %error, edit = ?edit, "the engine refused an edit");
    }
}

/// Write a tempo at a time, moving the marker there if one is close
/// enough to be the same one.
///
/// "Close enough" is a millisecond. Tempo mapping puts a marker on a
/// bar line and then corrects it, and a correction that added a second
/// marker a thousandth of a second from the first would leave a tempo
/// map full of pairs — and pairs mean a bar that is two tempos, which
/// is a bar with no tempo at all.
async fn set_tempo(project: &daw_control::Project, at: f64, bpm: f64) -> daw_control::Result<()> {
    const SAME_POINT: f64 = 0.001;
    let map = project.tempo_map();
    let existing = map.points().await.unwrap_or_default();
    let found = existing.iter().position(|point| {
        point
            .position
            .time
            .as_ref()
            .is_some_and(|t| (t.as_seconds() - at).abs() < SAME_POINT)
    });
    match found {
        Some(index) => {
            map.set_tempo_at(u32::try_from(index).unwrap_or(0), bpm)
                .await
        }
        None => map.add_point(at.max(0.0), bpm).await.map(|_| ()),
    }
}

/// Two times, in order, with a floor at zero.
///
/// A region dragged right to left is the region the user drew; a
/// region with its end before its start is one REAPER will refuse.
fn ordered(a: f64, b: f64) -> (f64, f64) {
    let (from, to) = if a <= b { (a, b) } else { (b, a) };
    (from.max(0.0), to.max(0.0))
}

/// Do something to one of a track's routes.
///
/// The resolve is half the cost of a route edit — `by_index` reads the
/// route to prove it exists before handing back a handle — so it is in
/// one place rather than repeated per arm, and a route that has gone is
/// a warning rather than a silent nothing: a level written into a
/// number nobody holds any more is exactly the bug that would otherwise
/// look like the panel not working.
async fn on_route<F, Fut>(
    track: &daw_control::TrackHandle,
    at: crate::routes::At,
    act: F,
) -> daw_control::Result<()>
where
    F: FnOnce(daw_control::RouteHandle) -> Fut,
    Fut: std::future::Future<Output = daw_control::Result<()>>,
{
    use daw_proto::routing::RouteType;
    let found = match at.kind {
        RouteType::Send => track.sends().by_index(at.index).await,
        RouteType::Receive => track.receives().by_index(at.index).await,
        RouteType::HardwareOutput => track.hardware_outputs().by_index(at.index).await,
    };
    match found {
        Ok(Some(route)) => act(route).await,
        Ok(None) => {
            tracing::warn!(
                route.index = at.index,
                route.kind = ?at.kind,
                "the route this edit names is gone"
            );
            Ok(())
        }
        Err(error) => Err(error),
    }
}

/// The engine's account of the ROUTING, as it changes.
///
/// A second subscription, and it has to be: what a send is doing —
/// its level, its pan, its mute — travels on the routing stream, and
/// the track stream carries only how MANY there are. A strip that
/// draws a lane does not need the first; a panel that draws the sends
/// needs nothing else.
///
/// Taken from the cross-domain bus rather than a routing stream of its
/// own, because there is no routing stream client: the bus is the only
/// client-side path to a [`RoutingEvent`]. One filter, one domain — the
/// rest are dropped before they reach this channel.
pub struct Bus {
    changes: std::sync::mpsc::Receiver<daw_proto::routing::RoutingEvent>,
    alive: Alive,
}

impl Bus {
    /// Subscribe. `None` if the facade is not up.
    #[must_use]
    pub fn start() -> Option<Self> {
        let runtime = crate::open::runtime()?;
        let (tx, rx) = std::sync::mpsc::channel();
        let alive = Alive::new();
        let mine = alive.clone();
        std::thread::Builder::new()
            .name("session-daw-routing".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let _end = scopeguard(move || mine.ended());
                    let Some(daw) = daw::rpc::Daw::try_get() else {
                        return;
                    };
                    let filter = daw_proto::event_bus::BusFilter {
                        routing: true,
                        ..daw_proto::event_bus::BusFilter::default()
                    };
                    let Ok(mut stream) = daw.events().subscribe(filter).await else {
                        return;
                    };
                    while let Ok(Some(event)) = stream.recv().await {
                        let daw_proto::event_bus::DawEvent::Routing(routing) = event.get() else {
                            continue;
                        };
                        if tx.send(routing.clone()).is_err() {
                            break;
                        }
                    }
                });
            })
            .ok()?;
        Some(Self { changes: rx, alive })
    }

    /// Everything that has happened since the last frame.
    pub fn drain(&self) -> impl Iterator<Item = daw_proto::routing::RoutingEvent> + '_ {
        self.changes.try_iter()
    }

    /// Is this subscription still connected?
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.alive.is_live()
    }
}

/// Which tracks' FX chains changed in the engine, by track GUID.
///
/// A parameter moved, an FX added, removed, moved or switched: what the FX
/// row reads again ([`crate::tone::Store::sync`]). Whoever made the change:
/// the row itself, an action over MCP, the mix chains at prepare.
#[cfg(feature = "native")]
pub struct FxWatch {
    changes: std::sync::mpsc::Receiver<String>,
    alive: Alive,
}

#[cfg(feature = "native")]
impl FxWatch {
    /// Subscribe. `None` if the facade is not up.
    #[must_use]
    pub fn start() -> Option<Self> {
        let runtime = crate::open::runtime()?;
        let (tx, rx) = std::sync::mpsc::channel();
        let alive = Alive::new();
        let mine = alive.clone();
        std::thread::Builder::new()
            .name("session-daw-fx".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let _end = scopeguard(move || mine.ended());
                    let Some(daw) = daw::rpc::Daw::try_get() else {
                        return;
                    };
                    let filter = daw_proto::event_bus::BusFilter {
                        fx: true,
                        ..daw_proto::event_bus::BusFilter::default()
                    };
                    let Ok(mut stream) = daw.events().subscribe(filter).await else {
                        return;
                    };
                    while let Ok(Some(event)) = stream.recv().await {
                        let daw_proto::event_bus::DawEvent::Fx(fx) = event.get() else {
                            continue;
                        };
                        if let daw_proto::fx::FxChainContext::Track(track) = fx_context(&fx.event)
                            && tx.send(track.clone()).is_err()
                        {
                            break;
                        }
                    }
                });
            })
            .ok()?;
        Some(Self { changes: rx, alive })
    }

    /// The tracks whose chains changed since the last frame.
    pub fn drain(&self) -> impl Iterator<Item = String> + '_ {
        self.changes.try_iter()
    }

    /// Is this subscription still connected?
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.alive.is_live()
    }
}

/// The chain an FX event is about.
#[cfg(feature = "native")]
const fn fx_context(event: &daw_proto::fx::FxEvent) -> &daw_proto::fx::FxChainContext {
    use daw_proto::fx::FxEvent as E;
    match event {
        E::Added { context, .. }
        | E::Removed { context, .. }
        | E::EnabledChanged { context, .. }
        | E::Moved { context, .. }
        | E::ParameterChanged { context, .. }
        | E::PresetChanged { context, .. }
        | E::WindowChanged { context, .. }
        | E::ContainerCreated { context, .. }
        | E::ContainerRemoved { context, .. }
        | E::RoutingModeChanged { context, .. }
        | E::MovedToContainer { context, .. }
        | E::ContainerRenamed { context, .. }
        | E::TreeStructureChanged { context } => context,
    }
}

/// The engine's own account of the tracks, as it changes.
///
/// The window predicts an edit so the control moves under the hand —
/// see `apply_locally` — and this is what turns that prediction into a
/// correction. It is also the only way a change made ANYWHERE ELSE
/// reaches the window: another client, a control surface, a REAPER on
/// the other end of daw-sync. A UI that only ever saw its own edits
/// would be a UI that silently disagrees with everyone.
///
/// Events rather than polling, because a change IS an event: missing
/// one leaves the window wrong until something else happens to it,
/// where missing a transport position is covered by the next one a few
/// milliseconds later. That asymmetry is why `Transport` polls and this
/// does not.
pub struct Watch {
    changes: std::sync::mpsc::Receiver<daw_proto::track::TrackEvent>,
    alive: Alive,
}

impl Watch {
    /// Subscribe. `None` if the facade is not up.
    ///
    /// The browser: no thread to subscribe on, so a task on the page's
    /// event loop, as [`Meters::start`] does.
    #[cfg(not(feature = "native"))]
    #[must_use]
    pub fn start() -> Option<Self> {
        let daw = daw::rpc::Daw::try_get()?;
        let (tx, rx) = std::sync::mpsc::channel();
        let alive = Alive::new();
        let mine = alive.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let _end = scopeguard(move || mine.ended());
            let Ok(project) = daw.current_project().await else {
                return;
            };
            let Ok(mut stream) = project.tracks().subscribe().await else {
                return;
            };
            while let Ok(Some(event)) = stream.recv().await {
                if tx.send(event.get().event.clone()).is_err() {
                    break;
                }
            }
        });
        Some(Self { changes: rx, alive })
    }

    /// Subscribe. `None` if the facade is not up.
    #[cfg(feature = "native")]
    #[must_use]
    pub fn start() -> Option<Self> {
        let runtime = crate::open::runtime()?;
        let (tx, rx) = std::sync::mpsc::channel();
        let alive = Alive::new();
        let mine = alive.clone();
        std::thread::Builder::new()
            .name("session-daw-watch".into())
            .spawn(move || {
                runtime.block_on(async move {
                    // Every exit from here marks the handle dead,
                    // including the ones that never got a stream at
                    // all: a subscribe that failed is a window with no
                    // corrections coming, which looks exactly like a
                    // project where nothing is happening.
                    let _end = scopeguard(move || mine.ended());
                    let Some(daw) = daw::rpc::Daw::try_get() else {
                        return;
                    };
                    let Ok(project) = daw.current_project().await else {
                        return;
                    };
                    let Ok(mut stream) = project.tracks().subscribe().await else {
                        return;
                    };
                    while let Ok(Some(event)) = stream.recv().await {
                        // A closed channel means the window has gone;
                        // there is nothing left to correct.
                        if tx.send(event.get().event.clone()).is_err() {
                            break;
                        }
                    }
                });
            })
            .ok()?;
        Some(Self { changes: rx, alive })
    }

    /// Everything that has happened since the last frame.
    ///
    /// Drained rather than waited on: the window asks once per frame
    /// and applies whatever arrived. A frame that finds nothing has
    /// nothing to do, which is most of them.
    pub fn drain(&self) -> impl Iterator<Item = daw_proto::track::TrackEvent> + '_ {
        self.changes.try_iter()
    }

    /// Is this subscription still connected?
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.alive.is_live()
    }
}

/// Apply one engine event to the window's copy.
///
/// The engine is the authority, so this overwrites rather than merges —
/// including over a prediction that turned out wrong. A fader the user
/// is still dragging is the one exception a caller may want to make,
/// and it is the caller's to make because only it knows what is being
/// dragged.
/// The track a CONTINUOUS event is about — a volume or a pan.
///
/// These are the only edits a hand can still be making when the
/// engine's answer to an earlier one arrives, so they are the only
/// ones a caller may want to hold back while a drag is in flight.
/// Everything else is a toggle or a name: by the time the engine
/// answers, the gesture that caused it is over, and there is nothing
/// for its answer to fight with.
#[must_use]
pub fn continuous_for(event: &daw_proto::track::TrackEvent) -> Option<&str> {
    use daw_proto::track::TrackEvent as E;
    match event {
        E::VolumeChanged { guid, .. } | E::PanChanged { guid, .. } => Some(guid),
        _ => None,
    }
}

/// What an event changed.
///
/// Worth distinguishing because the two cost different things. A field
/// is a poke: the row it belongs to redraws and nothing else moves. The
/// LIST changing invalidates every row map, every offset and the scene
/// resolved against it, so the caller has to rebuild — and it needs
/// telling, because it cannot see inside this function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applied {
    /// One track's field. Redraw it.
    Field,
    /// The track list itself. Rebuild the rows.
    Structure,
    /// Nothing this window keeps. Most often an event for a field the
    /// window does not draw.
    Nothing,
}

/// Apply one event to the window's track list.
///
/// Takes the `Vec` rather than a slice, and that is the whole reason
/// structure events used to be dropped here: a slice cannot grow, so
/// `Added` had nowhere to go and was quietly ignored. A track created
/// in REAPER did not appear until the window was restarted, and nothing
/// said so.
pub fn apply_event(
    tracks: &mut Vec<daw_proto::Track>,
    event: &daw_proto::track::TrackEvent,
) -> Applied {
    use daw_proto::track::TrackEvent as E;
    let find = |tracks: &mut [daw_proto::Track], guid: &str| -> Option<usize> {
        tracks.iter().position(|t| t.guid == guid)
    };
    match event {
        E::Renamed { guid, name } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].name.clone_from(name);
            }
        }
        E::MuteChanged { guid, muted } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].muted = *muted;
            }
        }
        E::SoloChanged { guid, soloed } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].soloed = *soloed;
            }
        }
        E::ArmChanged { guid, armed } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].armed = *armed;
            }
        }
        E::SelectionChanged { guid, selected } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].selected = *selected;
            }
        }
        E::VolumeChanged { guid, volume } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].volume = *volume;
            }
        }
        E::PanChanged { guid, pan } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].pan = *pan;
            }
        }
        E::ColorChanged { guid, color } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].color = *color;
            }
        }
        E::PhaseInvertedChanged { guid, inverted } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].phase_inverted = *inverted;
            }
        }
        E::RouteCountsChanged {
            guid,
            send_count,
            receive_count,
        } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].send_count = *send_count;
                tracks[i].receive_count = *receive_count;
            }
        }
        E::FxCountChanged {
            guid,
            fx_count,
            input_fx_count,
        } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].fx_count = *fx_count;
                tracks[i].input_fx_count = *input_fx_count;
            }
        }
        // The fields this window WRITES and then never heard back
        // about. A prediction that is never confirmed is only right
        // while nothing rejects it: a parent send refused by the engine,
        // or toggled by somebody else in REAPER, stayed wrong here for
        // the life of the window with nothing to say so.
        E::ParentSendChanged { guid, enabled } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].parent_send = *enabled;
            }
        }
        E::InputMonitorChanged { guid, monitor } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].input_monitor = *monitor;
            }
        }
        E::RecordInputChanged { guid, input } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].record_input = *input;
            }
        }
        E::AutomationModeChanged { guid, mode } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].automation_mode = *mode;
            }
        }
        E::GroupingChanged { guid, grouping } => {
            if let Some(i) = find(tracks, guid) {
                tracks[i].grouping = grouping.clone();
            }
        }
        E::HeightChanged { guid, height } => {
            let Some(i) = find(tracks, guid) else {
                return Applied::Nothing;
            };
            tracks[i].height = *height;
            // A row's height is the LAYOUT, so every row below it moves
            // — the same reason a track appearing is structural. A
            // window that repainted only this row would leave every
            // other one drawn at its old offset.
            return Applied::Structure;
        }
        E::LanesChanged {
            guid,
            lane_count,
            lane_play_mask,
            lane_names,
            lane_display,
        } => {
            let Some(i) = find(tracks, guid) else {
                return Applied::Nothing;
            };
            let track = &mut tracks[i];
            track.lane_count = *lane_count;
            track.lane_play_mask = *lane_play_mask;
            track.lane_names.clone_from(lane_names);
            track.lane_display = *lane_display;
            // Lanes are rows too: turning them on gives a track three
            // times the height it had.
            return Applied::Structure;
        }
        E::FolderDepthChanged { guid, folder_depth } => {
            let Some(i) = find(tracks, guid) else {
                return Applied::Nothing;
            };
            tracks[i].folder_depth = *folder_depth;
            tracks[i].is_folder = *folder_depth > 0;
            // Depth is RELATIVE, so this one track's change re-parents
            // every track below it, and the parent guids this window
            // holds are now wrong for tracks it was told nothing about.
            // Only a re-read can fix that, which is what Structure asks
            // for — patching the tree from here would be guessing.
            return Applied::Structure;
        }
        // Visibility is the list as drawn rather than the list as
        // stored, so it re-derives the rows: hiding a track in REAPER
        // and leaving its row on screen is the same failure as leaving
        // a removed one there.
        E::TcpVisibilityChanged { guid, visible } => {
            let Some(i) = find(tracks, guid) else {
                return Applied::Nothing;
            };
            tracks[i].visible_in_tcp = *visible;
            return Applied::Structure;
        }
        E::MixerVisibilityChanged { guid, visible } => {
            let Some(i) = find(tracks, guid) else {
                return Applied::Nothing;
            };
            tracks[i].visible_in_mixer = *visible;
            return Applied::Structure;
        }
        // ── the list itself ──────────────────────────────────────
        E::Added(track) => {
            // Inserted at the track's own index rather than pushed:
            // REAPER numbers tracks by position, so a track added in the
            // middle would otherwise sit at the end and every row below
            // it would name the wrong track.
            let at = usize::try_from(track.index).unwrap_or(tracks.len());
            let at = at.min(tracks.len());
            tracks.insert(at, track.clone());
            reindex(tracks);
            return Applied::Structure;
        }
        E::Removed(guid) => {
            let Some(i) = find(tracks, guid) else {
                return Applied::Nothing;
            };
            tracks.remove(i);
            reindex(tracks);
            return Applied::Structure;
        }
        E::Moved {
            guid, new_index, ..
        } => {
            let Some(from) = find(tracks, guid) else {
                return Applied::Nothing;
            };
            let to = usize::try_from(*new_index)
                .unwrap_or(from)
                .min(tracks.len().saturating_sub(1));
            let track = tracks.remove(from);
            tracks.insert(to, track);
            reindex(tracks);
            return Applied::Structure;
        } // Deliberately exhaustive. The catch-all that used to sit here
          // meant a field added to the event upstream was dropped in
          // silence, which is exactly how five of the arms above went
          // missing for months. A new variant should break this build.
    }
    Applied::Field
}

/// Renumber after the list changed.
///
/// The index is a track's position, so every track below an insertion
/// or a removal has a new one. Leaving them stale is worse than it
/// sounds: the scene engine resolves rows by index, so one stale number
/// shows the wrong track at the right place — which reads as a
/// rendering bug rather than a bookkeeping one.
fn reindex(tracks: &mut [daw_proto::Track]) {
    for (at, track) in tracks.iter_mut().enumerate() {
        track.index = u32::try_from(at).unwrap_or(track.index);
    }
}

#[cfg(test)]
mod watch_tests {
    use super::apply_event;
    use daw_proto::Track;
    use daw_proto::track::TrackEvent as E;

    fn tracks() -> Vec<Track> {
        ["a", "b"]
            .into_iter()
            .map(|g| Track {
                guid: g.to_owned(),
                volume: 1.0,
                // As a track arrives from either backend, not as
                // `Default` leaves it: a fixture that starts with the
                // parent send off and the track hidden cannot show that
                // turning either one off works.
                parent_send: true,
                visible_in_tcp: true,
                visible_in_mixer: true,
                ..Track::default()
            })
            .collect()
    }

    /// The engine's word overwrites the window's guess — including a
    /// guess that was wrong, which is the entire point of subscribing.
    #[test]
    fn the_engine_corrects_a_bad_prediction() {
        let mut t = tracks();
        // The window predicted a mute.
        t[0].muted = true;
        // The engine says otherwise — a solo-exclusive elsewhere, say.
        apply_event(
            &mut t,
            &E::MuteChanged {
                guid: "a".into(),
                muted: false,
            },
        );
        assert!(!t[0].muted);
    }

    /// An event for a track this window does not have is ignored rather
    /// than panicking. The engine's list and the window's can differ for
    /// a frame after a change.
    #[test]
    fn an_unknown_track_is_ignored() {
        let mut t = tracks();
        apply_event(
            &mut t,
            &E::VolumeChanged {
                guid: "gone".into(),
                volume: 0.5,
            },
        );
        assert!((t[0].volume - 1.0).abs() < f64::EPSILON);
    }

    /// Every field the UI draws is reachable, or a change made
    /// elsewhere would never show.
    #[test]
    fn every_drawn_field_can_be_corrected() {
        let mut t = tracks();
        apply_event(
            &mut t,
            &E::Renamed {
                guid: "a".into(),
                name: "Kick".into(),
            },
        );
        apply_event(
            &mut t,
            &E::SoloChanged {
                guid: "a".into(),
                soloed: true,
            },
        );
        apply_event(
            &mut t,
            &E::ArmChanged {
                guid: "a".into(),
                armed: true,
            },
        );
        apply_event(
            &mut t,
            &E::VolumeChanged {
                guid: "a".into(),
                volume: 0.25,
            },
        );
        apply_event(
            &mut t,
            &E::PanChanged {
                guid: "a".into(),
                pan: -0.5,
            },
        );
        apply_event(
            &mut t,
            &E::SelectionChanged {
                guid: "a".into(),
                selected: true,
            },
        );
        assert_eq!(t[0].name, "Kick");
        assert!(t[0].soloed && t[0].armed && t[0].selected);
        assert!((t[0].volume - 0.25).abs() < f64::EPSILON);
        assert!((t[0].pan + 0.5).abs() < f64::EPSILON);
    }

    /// The fields this window writes and predicts locally.
    ///
    /// A prediction is only right until something rejects it, so the
    /// event that confirms it is what makes the local guess safe. This
    /// test is the negative control for that: every field here was
    /// already being written and drawn, and none of them could be
    /// corrected — a parent send toggled by anybody else stayed wrong
    /// for the life of the window.
    #[test]
    fn a_prediction_this_window_made_can_be_corrected() {
        use daw_proto::primitives::AutomationMode;
        use daw_proto::track::{InputMonitoringMode, RecordInput};

        let mut t = tracks();
        assert!(t[0].parent_send, "the control needs somewhere to fall from");

        apply_event(
            &mut t,
            &E::ParentSendChanged {
                guid: "a".into(),
                enabled: false,
            },
        );
        apply_event(
            &mut t,
            &E::InputMonitorChanged {
                guid: "a".into(),
                monitor: InputMonitoringMode::NotWhenPlaying,
            },
        );
        apply_event(
            &mut t,
            &E::RecordInputChanged {
                guid: "a".into(),
                input: RecordInput::Audio { channel: 7 },
            },
        );
        apply_event(
            &mut t,
            &E::AutomationModeChanged {
                guid: "a".into(),
                mode: AutomationMode::Latch,
            },
        );

        assert!(!t[0].parent_send);
        assert_eq!(t[0].input_monitor, InputMonitoringMode::NotWhenPlaying);
        assert_eq!(t[0].record_input, RecordInput::Audio { channel: 7 });
        assert_eq!(t[0].automation_mode, AutomationMode::Latch);
        assert!(t[1].parent_send, "the other track was not touched");
    }

    /// Hiding a track changes the rows, not just a field.
    ///
    /// The rows are derived from the list, so a visibility change has to
    /// say the list changed — otherwise a track hidden in REAPER keeps
    /// its row on screen, which is the same failure as leaving a removed
    /// one there.
    #[test]
    fn hiding_a_track_redraws_the_rows() {
        use super::Applied;
        let mut t = tracks();
        assert_eq!(
            apply_event(
                &mut t,
                &E::TcpVisibilityChanged {
                    guid: "a".into(),
                    visible: false,
                },
            ),
            Applied::Structure
        );
        assert!(!t[0].visible_in_tcp);

        // An event for a track this window never heard of changes
        // nothing and must not ask for a redraw.
        assert_eq!(
            apply_event(
                &mut t,
                &E::MixerVisibilityChanged {
                    guid: "nobody".into(),
                    visible: false,
                },
            ),
            Applied::Nothing
        );
    }

    /// One track's event does not touch another's.
    #[test]
    fn events_do_not_leak_between_tracks() {
        let mut t = tracks();
        apply_event(
            &mut t,
            &E::MuteChanged {
                guid: "b".into(),
                muted: true,
            },
        );
        assert!(!t[0].muted && t[1].muted);
    }
}

#[cfg(test)]
mod continuous_tests {
    use super::continuous_for;
    use daw_proto::track::TrackEvent as E;

    /// A fader and a pan are the two things a hand can still be holding
    /// when the engine answers, so they are the two the window may hold
    /// back.
    #[test]
    fn only_the_draggable_ones_name_a_track() {
        assert_eq!(
            continuous_for(&E::VolumeChanged {
                guid: "a".to_owned(),
                volume: 0.5,
            }),
            Some("a")
        );
        assert_eq!(
            continuous_for(&E::PanChanged {
                guid: "b".to_owned(),
                pan: -0.25,
            }),
            Some("b")
        );
    }

    /// A mute is over the moment it is clicked, so the engine's word on
    /// it is never in competition with a hand.
    #[test]
    fn a_toggle_is_not_continuous() {
        assert_eq!(
            continuous_for(&E::MuteChanged {
                guid: "a".to_owned(),
                muted: true,
            }),
            None
        );
        assert_eq!(
            continuous_for(&E::Renamed {
                guid: "a".to_owned(),
                name: "Kick".to_owned(),
            }),
            None
        );
    }
}

/// The engine's live meter levels.
///
/// A separate subscription from [`Watch`] because it is a different
/// KIND of fact. A track event is a change the window must not miss —
/// a mute it never heard about leaves the strip wrong until something
/// else happens. A meter frame is a measurement, and the next one is
/// thirty milliseconds away: missing one costs nothing, and queueing
/// them costs a backlog of readings that were true a second ago.
///
/// So this keeps the LATEST frame and drops the rest, where `Watch`
/// keeps every event in order. Same transport, opposite policy, and the
/// policy is the reason they are not one type.
pub struct Meters {
    latest: std::sync::Arc<std::sync::Mutex<Vec<daw_proto::TrackLevels>>>,
    alive: Alive,
}

impl Meters {
    /// Subscribe. `None` if the facade is not up, in which case the
    /// meters stay at rest rather than the page failing to open.
    ///
    /// The browser: no thread to subscribe on, so a task on the page's
    /// event loop, fed by the engine's meter pump (which the web audio
    /// loop's renders fill).
    #[cfg(not(feature = "native"))]
    #[must_use]
    pub fn start() -> Option<Self> {
        let daw = daw::rpc::Daw::try_get()?;
        let latest = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let into_task = std::sync::Arc::clone(&latest);
        let alive = Alive::new();
        let mine = alive.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let _end = scopeguard(move || mine.ended());
            let Ok(project) = daw.current_project().await else {
                return;
            };
            let mut stream = project.meter_events();
            while let Ok(Some(frame)) = stream.recv().await {
                let frame = frame.get();
                let Ok(mut slot) = into_task.lock() else {
                    break;
                };
                slot.clear();
                slot.extend_from_slice(&frame.tracks);
            }
        });
        Some(Self { latest, alive })
    }

    /// Subscribe. `None` if the facade is not up, in which case the
    /// meters stay at rest rather than the window failing to open.
    #[cfg(feature = "native")]
    #[must_use]
    pub fn start() -> Option<Self> {
        let runtime = crate::open::runtime()?;
        let latest = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let into_thread = std::sync::Arc::clone(&latest);
        let alive = Alive::new();
        let mine = alive.clone();
        std::thread::Builder::new()
            .name("session-daw-meters".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let _end = scopeguard(move || mine.ended());
                    let Some(daw) = daw::rpc::Daw::try_get() else {
                        return;
                    };
                    let Ok(project) = daw.current_project().await else {
                        return;
                    };
                    let mut stream = project.meter_events();
                    while let Ok(Some(frame)) = stream.recv().await {
                        let frame = frame.get();
                        // Overwrite rather than append: the only frame
                        // worth drawing is the one that just arrived.
                        let Ok(mut slot) = into_thread.lock() else {
                            break;
                        };
                        slot.clear();
                        slot.extend_from_slice(&frame.tracks);
                    }
                });
            })
            .ok()?;
        Some(Self { latest, alive })
    }

    /// Is this subscription still connected?
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.alive.is_live()
    }

    /// The most recent frame, copied out for this redraw.
    ///
    /// Copied rather than borrowed because the lock must not be held
    /// across a frame: the pump publishes at 30 Hz and a renderer
    /// holding its mutex would stall the engine's thread, which is the
    /// one thread in this program that has a deadline.
    #[must_use]
    pub fn levels(&self) -> Vec<daw_proto::TrackLevels> {
        self.latest
            .lock()
            .map(|frame| frame.clone())
            .unwrap_or_default()
    }
}

/// A meter's height, from a linear peak.
///
/// The meter is drawn from a fraction of its own height, so the level
/// has to be mapped the way an ear reads it rather than the way the
/// sample buffer holds it: linear `0..1` puts −20 dBFS — a perfectly
/// ordinary track — at two per cent of the well, which reads as
/// silence.
///
/// The same scale the fader uses, so a track sitting at unity with a
/// hot signal lights the meter to about where its own cap sits. That
/// correspondence is what makes a mixer readable at a glance.
#[must_use]
pub fn meter_fraction(peak: f32) -> f64 {
    let db = 20.0 * f64::from(peak.max(1e-6)).log10();
    // The METER's scale, which runs past unity: a signal over 0 dBFS
    // has to land above the 0 mark rather than pinning to the same full
    // bar a clean −1 draws.
    daw_theme_art::paint::tcp::meter_norm(db)
}

#[cfg(test)]
mod meter_tests {
    use super::meter_fraction;

    /// Silence empties it, 0 dBFS lands on the 0 mark rather than at
    /// the top, and there is room above for the overs — which is the
    /// one distinction a meter exists to make and the one a scale
    /// ending at unity cannot.
    #[test]
    fn the_meter_reads_in_decibels() {
        use daw_theme_art::paint::tcp::{METER_TOP_DB, meter_norm};
        assert!(meter_fraction(0.0) < 1e-6, "silence is empty");
        let unity = meter_fraction(1.0);
        assert!(
            (unity - meter_norm(0.0)).abs() < 1e-6,
            "0 dBFS landed at {unity}, not on the 0 mark"
        );
        assert!(
            unity < 1.0,
            "0 dBFS filled the column, leaving no room for an over"
        );
        // And an over goes ABOVE it rather than pinning to the same bar.
        assert!(
            meter_fraction(1.5) > unity,
            "an over did not rise past unity"
        );
        // Up to the ceiling, where it stops rather than running off.
        let way_over = meter_fraction(10.0);
        assert!(
            way_over <= 1.0,
            "a loud over ran off the column: {way_over}"
        );
        assert!((METER_TOP_DB - 12.0).abs() < f64::EPSILON);
    }

    /// A denormal or a zero must not produce a NaN height — the meter
    /// would vanish rather than read empty.
    #[test]
    fn silence_is_a_number() {
        assert!(meter_fraction(0.0).is_finite());
        assert!(meter_fraction(f32::MIN_POSITIVE).is_finite());
    }
}

#[cfg(test)]
mod structure_tests {
    use super::{Applied, apply_event};
    use daw_proto::Track;
    use daw_proto::track::TrackEvent as E;

    fn kit() -> Vec<Track> {
        ["Kick", "Snare", "OH"]
            .iter()
            .enumerate()
            .map(|(i, name)| {
                Track::new(
                    (*name).to_owned(),
                    u32::try_from(i).unwrap_or(0),
                    (*name).to_owned(),
                )
            })
            .collect()
    }

    /// **A track created in REAPER appears.** The applier used to take a
    /// slice, so `Added` had nowhere to go and was dropped — the window
    /// only caught up when something else triggered a full re-fetch.
    #[test]
    fn a_track_added_in_the_daw_lands_in_the_list() {
        let mut tracks = kit();
        let added = Track::new("Room".to_owned(), 3, "Room".to_owned());
        assert_eq!(
            apply_event(&mut tracks, &E::Added(added)),
            Applied::Structure
        );
        assert_eq!(tracks.len(), 4);
        assert_eq!(tracks[3].name, "Room");
    }

    /// **And at its own position, not the end.** REAPER numbers tracks
    /// by position, so a track inserted in the middle that got appended
    /// would leave every row below it naming the wrong track.
    #[test]
    fn a_track_added_in_the_middle_lands_in_the_middle() {
        let mut tracks = kit();
        let added = Track::new("Sub".to_owned(), 1, "Sub".to_owned());
        apply_event(&mut tracks, &E::Added(added));
        let names: Vec<&str> = tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Kick", "Sub", "Snare", "OH"]);
    }

    /// The index is a position, so everything below a change is
    /// renumbered. A stale index shows the wrong track at the right
    /// place, which reads as a rendering bug rather than a bookkeeping
    /// one.
    #[test]
    fn the_list_is_renumbered_after_it_changes() {
        let mut tracks = kit();
        apply_event(
            &mut tracks,
            &E::Added(Track::new("Sub".to_owned(), 1, "Sub".to_owned())),
        );
        let indices: Vec<u32> = tracks.iter().map(|t| t.index).collect();
        assert_eq!(indices, [0, 1, 2, 3]);

        apply_event(&mut tracks, &E::Removed("Kick".to_owned()));
        let indices: Vec<u32> = tracks.iter().map(|t| t.index).collect();
        assert_eq!(indices, [0, 1, 2], "removal left a hole in the numbering");
    }

    /// A track removed in the DAW goes.
    #[test]
    fn a_track_removed_in_the_daw_leaves_the_list() {
        let mut tracks = kit();
        assert_eq!(
            apply_event(&mut tracks, &E::Removed("Snare".to_owned())),
            Applied::Structure
        );
        let names: Vec<&str> = tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Kick", "OH"]);
    }

    /// A reorder in the DAW reorders here.
    #[test]
    fn a_moved_track_moves() {
        let mut tracks = kit();
        apply_event(
            &mut tracks,
            &E::Moved {
                guid: "OH".to_owned(),
                old_index: 2,
                new_index: 0,
            },
        );
        let names: Vec<&str> = tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["OH", "Kick", "Snare"]);
    }

    /// **An event for a track this window has never heard of is not an
    /// error.** Attaching to a REAPER mid-session means the first events
    /// can name anything, and a window that panicked or resynced on each
    /// one would be unusable for the first second of every attach.
    #[test]
    fn an_event_for_an_unknown_track_is_ignored() {
        let mut tracks = kit();
        let before = tracks.len();
        assert_eq!(
            apply_event(&mut tracks, &E::Removed("nonesuch".to_owned())),
            Applied::Nothing
        );
        assert_eq!(tracks.len(), before);
    }

    /// A field change says so, so the window redraws a row instead of
    /// rebuilding every row map it has.
    #[test]
    fn a_field_change_is_not_a_rebuild() {
        let mut tracks = kit();
        assert_eq!(
            apply_event(
                &mut tracks,
                &E::MuteChanged {
                    guid: "Kick".to_owned(),
                    muted: true,
                }
            ),
            Applied::Field
        );
    }
}

/// Run a closure when the scope ends, however it ends.
///
/// Every stream loop here has several ways out — no facade, no project,
/// a refused subscribe, a closed stream — and each of them has to mark
/// the handle dead. Writing that at four exits is writing it at three
/// and forgetting the fourth, which in this file means a window that
/// believes it is connected.
fn scopeguard<F: FnOnce()>(f: F) -> impl Drop {
    struct Guard<F: FnOnce()>(Option<F>);
    impl<F: FnOnce()> Drop for Guard<F> {
        fn drop(&mut self) {
            if let Some(f) = self.0.take() {
                f();
            }
        }
    }
    Guard(Some(f))
}

/// Whether a subscription is still connected.
///
/// Every stream in this file ends the same way: REAPER quits, the
/// socket closes, `recv` returns `None` and the thread falls out of its
/// loop. Nothing used to notice. The window kept its handle, the
/// handle's restart check only fires when the handle is *missing*, and
/// so a REAPER quit and reopened left a window drawing the last thing
/// it heard — connected in appearance and dead in fact, which is the
/// failure a mirror must never have.
///
/// Shared rather than returned, because the thread is the only one that
/// knows and the window is the only one that can act.
#[derive(Clone)]
pub struct Alive(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Alive {
    fn new() -> Self {
        Self(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
            true,
        )))
    }

    fn ended(&self) {
        self.0.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    /// Is the stream behind this handle still delivering?
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Everything in the project that is not a track.
///
/// Items, takes, markers, regions and the tempo map are all read as a
/// snapshot and drawn from it, and until now nothing told the window
/// they had moved. An item dragged in REAPER stayed where it used to be
/// until some unrelated track was added or removed and the resulting
/// re-read happened to bring it along — which is a mirror that is right
/// only by accident.
///
/// It is a FLAG rather than a queue of events, and that is the whole
/// design. The window does not keep items, takes, markers or the tempo
/// map as anything it could patch: they are derived, in one read, from
/// the project. So the useful thing to know is "the snapshot is old",
/// and knowing it twice is the same as knowing it once — which is
/// exactly what a flag says and a queue does not. A drag in REAPER
/// emits an event per frame and this coalesces every one of them into
/// a single re-read.
///
/// Tracks are the exception and keep their own [`Watch`], because the
/// window DOES hold them and can correct a single field without paying
/// for a whole read.
pub struct Refresh {
    stale: std::sync::Arc<std::sync::atomic::AtomicBool>,
    alive: Alive,
}

impl Refresh {
    /// Subscribe. `None` if the facade is not up.
    #[must_use]
    pub fn start() -> Option<Self> {
        use daw_proto::event_bus::BusFilter;

        let runtime = crate::open::runtime()?;
        let stale = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer = std::sync::Arc::clone(&stale);
        let alive = Alive::new();
        let mine = alive.clone();
        std::thread::Builder::new()
            .name("session-daw-refresh".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let _end = scopeguard(move || mine.ended());
                    let Some(daw) = daw::rpc::Daw::try_get() else {
                        return;
                    };
                    // Named field by field rather than `all()` minus a
                    // couple, because every domain switched on here
                    // costs a whole project read. Tracks are out
                    // because `Watch` has them and patches them in
                    // place. The position tick and the play state are
                    // out because they change constantly and change
                    // nothing that is drawn from the snapshot. FX and
                    // routing are out because nothing here draws
                    // them — an automated parameter would otherwise
                    // re-read the session at audio rate.
                    let filter = BusFilter {
                        items: true,
                        takes: true,
                        markers: true,
                        regions: true,
                        tempo_map: true,
                        projects: true,
                        tracks: false,
                        fx: false,
                        routing: false,
                        transport_state: false,
                        transport_position: false,
                        project_guid: None,
                    };
                    let Ok(mut stream) = daw.events().subscribe(filter).await else {
                        return;
                    };
                    while let Ok(Some(_)) = stream.recv().await {
                        writer.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                });
            })
            .ok()?;
        Some(Self { stale, alive })
    }

    /// Is this subscription still connected?
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.alive.is_live()
    }

    /// Is the snapshot old?
    ///
    /// Asking does not clear it. Reading and clearing in one step reads
    /// better and is wrong here: a caller that cannot act yet — because
    /// the last read is still in flight — would throw away the news
    /// that arrived while it was reading, and the last edit of a drag
    /// is exactly the one that would go missing.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.stale.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Say the snapshot has been re-read. Call this only when a read
    /// actually started.
    pub fn settled(&self) {
        self.stale
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod refresh_tests {
    use super::Refresh;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn refresh() -> (Refresh, Arc<AtomicBool>) {
        let stale = Arc::new(AtomicBool::new(false));
        (
            Refresh {
                stale: Arc::clone(&stale),
                alive: super::Alive::new(),
            },
            stale,
        )
    }

    /// Many events, one read.
    ///
    /// A drag in REAPER emits an event per frame, and every one of them
    /// says the same thing: the snapshot is old. Re-reading the project
    /// once per event would re-read it sixty times for one gesture.
    #[test]
    fn a_burst_of_changes_asks_for_one_read() {
        let (refresh, stale) = refresh();
        for _ in 0..60 {
            stale.store(true, Ordering::Relaxed);
        }
        assert!(refresh.pending());
        refresh.settled();
        assert!(!refresh.pending(), "one read answered all sixty");
    }

    /// A handle whose stream ended reads as dead.
    ///
    /// This is the whole of the REAPER-went-away detection. The guard
    /// fires however the thread leaves — no facade, no project, a
    /// refused subscribe, a closed stream — because writing the same
    /// line at four exits is writing it at three.
    #[test]
    fn a_stream_that_ends_marks_its_handle_dead() {
        let alive = super::Alive::new();
        assert!(alive.is_live());

        let seen = alive.clone();
        {
            let mine = alive.clone();
            let _end = super::scopeguard(move || mine.ended());
            assert!(seen.is_live(), "still connected inside the loop");
        }
        assert!(!seen.is_live(), "the window can see the connection go");
    }

    /// Asking does not clear.
    ///
    /// This is the whole reason `pending` and `settled` are two calls.
    /// A frame that cannot act — because the last read is still in
    /// flight — asks and does nothing, and the news has to still be
    /// there on the next frame. Collapsing them into one read-and-clear
    /// would lose exactly the change that arrives while the window is
    /// busy reading, which in a drag is the one that says where the
    /// item ended up.
    #[test]
    fn asking_does_not_clear() {
        let (refresh, stale) = refresh();
        stale.store(true, Ordering::Relaxed);
        assert!(refresh.pending());
        assert!(refresh.pending(), "a frame that could not act kept it");
        assert!(refresh.pending());
        refresh.settled();
        assert!(!refresh.pending(), "only a read that happened clears it");
    }
}
