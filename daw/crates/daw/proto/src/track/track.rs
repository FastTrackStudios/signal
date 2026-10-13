//! Track data types
//!
//! A track represents an audio or MIDI channel in the DAW mixer.

use facet::Facet;

/// Reference to a track - how to identify a track for operations
///
/// Tracks can be identified by GUID (stable across sessions), index (position-based),
/// or the special Master track designation.
#[repr(u8)]
#[derive(Clone, Debug, Facet)]
pub enum TrackRef {
    /// Track GUID - stable across sessions
    Guid(String),
    /// Track index (0-based position in track list)
    Index(u32),
    /// The master track
    Master,
}

/// Behavior to use when moving selected tracks to a new index.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub enum ReorderTracksBehavior {
    /// Move selected tracks without changing folder membership.
    Normal,
    /// Make moved tracks children of the track immediately above the destination.
    MakeChildOfPreviousTrack,
    /// Extend the folder above the destination when applicable.
    ExtendFolder,
}

impl TrackRef {
    /// Create a reference by GUID
    pub fn guid(guid: impl Into<String>) -> Self {
        Self::Guid(guid.into())
    }

    /// Create a reference by index
    pub fn index(index: u32) -> Self {
        Self::Index(index)
    }

    /// Create a reference to the master track
    pub fn master() -> Self {
        Self::Master
    }
}

impl From<u32> for TrackRef {
    fn from(index: u32) -> Self {
        Self::Index(index)
    }
}

impl From<&str> for TrackRef {
    fn from(guid: &str) -> Self {
        Self::Guid(guid.to_string())
    }
}

impl From<String> for TrackRef {
    fn from(guid: String) -> Self {
        Self::Guid(guid)
    }
}

/// Complete track state returned from queries
///
/// Contains all relevant track information including identification,
/// state flags, levels, and structural information.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct Track {
    /// Unique GUID for stable identification across sessions
    pub guid: String,
    /// Track index (0-based position in track list)
    pub index: u32,
    /// Display name of the track
    pub name: String,
    /// Color in native format (0xRRGGBB, or None for default)
    pub color: Option<u32>,

    // === State Flags ===
    /// Whether the track is muted
    pub muted: bool,
    /// Whether the track is soloed
    pub soloed: bool,
    /// Whether the track is armed for recording
    pub armed: bool,
    /// Whether the track is selected
    pub selected: bool,

    // === Levels (normalized) ===
    /// Volume level (0.0 = -inf dB, 1.0 = 0 dB)
    pub volume: f64,
    /// Pan position (-1.0 = left, 0.0 = center, 1.0 = right)
    pub pan: f64,
    /// Polarity/phase invert (REAPER `IPHASE`): flip the signal's sign.
    pub phase_inverted: bool,
    /// Track automation mode (REAPER `I_AUTOMODE`): trim/read/touch/
    /// write/latch. Governs whether control moves record envelope
    /// points during playback.
    pub automation_mode: crate::primitives::AutomationMode,
    /// Record input monitoring (REAPER `I_RECMON`).
    pub input_monitor: InputMonitoringMode,
    /// What the track records from (REAPER `I_RECINPUT`).
    ///
    /// On the track rather than behind a getter of its own, deliberately:
    /// a strip needs it, and building a strip has to stay *one* bulk read.
    /// A per-track getter would make an N-track mixer cost N extra round
    /// trips to show something every strip displays.
    pub record_input: RecordInput,
    /// Does the track send to its parent / master (REAPER `B_MAINSEND`)?
    ///
    /// Here for the same reason as `record_input`: it was a separate
    /// per-track routing call, so a mixer paid N round trips for a flag its
    /// IO indicator has to show on every strip.
    pub parent_send: bool,

    // === Structure ===
    /// GUID of the parent folder track, if any
    pub parent_guid: Option<String>,
    /// Folder depth (positive = start folder, negative = end folder levels)
    pub folder_depth: i32,
    /// Whether this track is a folder track
    pub is_folder: bool,

    // === Fixed item lanes (REAPER 7 comping) ===
    /// Number of fixed item lanes (0 = lanes disabled).
    pub lane_count: u32,
    /// Bitmask of lanes that PLAY (bit n = lane n audible). Lanes
    /// outside the mask hold alternate takes that stay silent.
    pub lane_play_mask: u64,
    /// Lane display names, one per lane (may be shorter than
    /// `lane_count`; missing entries default to 1-based numbers).
    pub lane_names: Vec<String>,
    /// How fixed lanes are displayed (REAPER's one/small/big cycle).
    pub lane_display: LaneDisplay,

    // === Grouping ===
    /// Track-grouping matrix membership (control ganging + VCA).
    pub grouping: TrackGrouping,

    // === Visibility ===
    /// Whether the track is visible in the TCP (track control panel / arrange view)
    pub visible_in_tcp: bool,
    /// Whether the track is visible in the MCP (mixer control panel)
    pub visible_in_mixer: bool,

    // === Routing ===
    /// How many sends this track has to other tracks, and how many
    /// receives it has from them.
    ///
    /// On the track for the same reason as `record_input` and
    /// `parent_send`: the IO indicator on every strip shows whether
    /// there are any, and building a strip has to stay ONE bulk read.
    /// Asking the routing service per track would make an N-track mixer
    /// cost N extra round trips for two numbers.
    ///
    /// Counts, not the routes. A strip lights a lane; it does not draw
    /// the destinations, and carrying them here would put a list that
    /// changes independently inside the snapshot of something else.
    pub send_count: u32,
    /// See [`send_count`](Self::send_count).
    pub receive_count: u32,

    // === FX Info ===
    /// Number of FX in the main FX chain
    pub fx_count: u32,
    /// Number of FX in the input/recording FX chain
    pub input_fx_count: u32,
    /// How tall the track's panel is drawn, in pixels — REAPER's
    /// `TRACKHEIGHT`. See also [`Track::width`], its mixer counterpart.
    ///
    /// A view concern living on the track on purpose. It is not what a
    /// track IS, and nothing about playback or routing reads it; but it
    /// is stored per track in the project file, it round-trips through
    /// save and load, and every surface that draws a track list needs it
    /// at the same moment it needs the name and the colour. Modelling it
    /// anywhere else would mean a second lookup keyed by GUID on every
    /// row of every panel, for a number the project already carries next
    /// to the rest of the track.
    ///
    /// `None` = the host's default. Not zero: a track that is nought
    /// pixels tall is a different claim from one that has never been
    /// resized, and the UI's minimum is the UI's business.
    pub height: Option<u32>,
    /// How wide the track's mixer strip is drawn, in pixels.
    ///
    /// The counterpart of [`Track::height`], and modelled the same way —
    /// but with no host field behind it, because REAPER has no such
    /// thing: every strip there is one width. It is ours, and it
    /// persists in the project's `<EXTSTATE>` block, which is where
    /// REAPER keeps what it does not model itself.
    ///
    /// It exists because a mixer is read by scanning across it, and a
    /// session has tracks that deserve very different amounts of that
    /// scan: a trigger or a reverb return needs its name and its mute,
    /// while the track being worked on wants room for an embedded FX
    /// display. One width for all of them spends the same space on both.
    ///
    /// `None` = the default width, which is a user setting like the
    /// default height.
    pub width: Option<u32>,
}

impl Track {
    /// Create a new track with default values
    pub fn new(guid: String, index: u32, name: String) -> Self {
        Self {
            guid,
            index,
            name,
            color: None,
            muted: false,
            soloed: false,
            armed: false,
            selected: false,
            volume: 1.0,
            pan: 0.0,
            phase_inverted: false,
            automation_mode: crate::primitives::AutomationMode::TrimRead,
            input_monitor: InputMonitoringMode::Off,
            parent_guid: None,
            folder_depth: 0,
            is_folder: false,
            lane_count: 0,
            lane_play_mask: 0,
            lane_names: Vec::new(),
            lane_display: LaneDisplay::default(),
            grouping: TrackGrouping::default(),
            visible_in_tcp: true,
            visible_in_mixer: true,
            send_count: 0,
            receive_count: 0,
            fx_count: 0,
            input_fx_count: 0,
            height: None,
            width: None,
            record_input: RecordInput::None,
            // Sending, because that is what a new track does — a default of
            // "cut off from the master" would put a disabled badge on every
            // strip the moment anything constructed a Track without asking.
            parent_send: true,
        }
    }

    /// Check if this is the master track (index 0 with special characteristics)
    pub fn is_master(&self) -> bool {
        // Master track typically has no parent and special naming
        self.parent_guid.is_none() && self.name.to_lowercase().contains("master")
    }

    /// Get a TrackRef for this track by GUID
    pub fn as_ref(&self) -> TrackRef {
        TrackRef::Guid(self.guid.clone())
    }

    /// Get a TrackRef for this track by index
    pub fn as_index_ref(&self) -> TrackRef {
        TrackRef::Index(self.index)
    }
}

impl Default for Track {
    fn default() -> Self {
        Self::new(String::new(), 0, String::new())
    }
}

/// Track-grouping matrix membership (REAPER's Track Grouping
/// Parameters). Each field is a bitmask of group slots (bit n =
/// slot n+1) over all 128 slots REAPER 7 has: slots 1–32 come from
/// `GROUP_FLAGS`, 33–64 from `GROUP_FLAGS_HIGH`, 65–128 only from the
/// live membership API (no `.RPP` line for them has been observed).
///
/// Lead/follow gangs CONTROL GESTURES (touching a lead's control moves
/// followers' controls); VCA follow is the one that changes PLAYBACK —
/// a follower's effective gain is its fader × every shared-group VCA
/// lead's fader, and a muted VCA lead silences its followers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Facet)]
pub struct TrackGrouping {
    pub volume_lead: u128,
    pub volume_follow: u128,
    pub pan_lead: u128,
    pub pan_follow: u128,
    pub mute_lead: u128,
    pub mute_follow: u128,
    pub solo_lead: u128,
    pub solo_follow: u128,
    pub recarm_lead: u128,
    pub recarm_follow: u128,
    pub polarity_lead: u128,
    pub polarity_follow: u128,
    pub automode_lead: u128,
    pub automode_follow: u128,
    /// Follower moves opposite to the lead (volume).
    pub volume_reverse: u128,
    /// Follower moves opposite to the lead (pan).
    pub pan_reverse: u128,
    /// A track that follows in this slot does not also lead it.
    pub no_lead_when_follow: u128,
    /// Follower moves opposite to the lead (width).
    pub width_reverse: u128,
    pub width_lead: u128,
    pub width_follow: u128,
    pub vca_lead: u128,
    pub vca_follow: u128,
    /// VCA follow applied pre-FX instead of at the fader.
    pub vca_prefx_follow: u128,
    pub media_edit_lead: u128,
    pub media_edit_follow: u128,
}

/// The ten lead/follow flag families of a track-group slot.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Facet)]
pub enum GroupFamily {
    Volume,
    /// VCA: the lead's fader and mute act on followers at playback
    /// without moving their controls.
    Vca,
    Pan,
    Width,
    Mute,
    Solo,
    RecArm,
    Polarity,
    AutoMode,
    MediaEdit,
}

impl GroupFamily {
    pub const ALL: [Self; 10] = [
        Self::Volume,
        Self::Vca,
        Self::Pan,
        Self::Width,
        Self::Mute,
        Self::Solo,
        Self::RecArm,
        Self::Polarity,
        Self::AutoMode,
        Self::MediaEdit,
    ];
}

/// A track's part in one family of one slot.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Facet)]
pub enum GroupRole {
    Lead,
    Follow,
    /// Not a member of the family in that slot.
    #[default]
    None,
}

/// The per-slot modifiers that are not a lead/follow pair.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Facet)]
pub enum GroupModifier {
    VolumeReverse,
    PanReverse,
    WidthReverse,
    NoLeadWhenFollow,
    VcaFollowPreFx,
}

impl GroupModifier {
    pub const ALL: [Self; 5] = [
        Self::VolumeReverse,
        Self::PanReverse,
        Self::WidthReverse,
        Self::NoLeadWhenFollow,
        Self::VcaFollowPreFx,
    ];
}

/// One family of one slot set to a role — the argument of
/// `Tracks::set_group_flags` (bundled: the RPC surface allows four
/// parameters after `self`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct GroupFlagChange {
    /// 1-based slot, 1..=128.
    pub slot: u32,
    pub family: GroupFamily,
    pub role: GroupRole,
}

/// One modifier of one slot switched on or off — the argument of
/// `Tracks::set_group_modifier`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct GroupModifierChange {
    /// 1-based slot, 1..=128.
    pub slot: u32,
    pub modifier: GroupModifier,
    pub enabled: bool,
}

/// Number of track-group slots (REAPER 7.23+).
pub const GROUP_SLOTS: u32 = 128;

/// Bit for a 1-based slot in a grouping mask; `0` for a slot outside
/// `1..=GROUP_SLOTS`.
pub fn group_slot_bit(slot: u32) -> u128 {
    if (1..=GROUP_SLOTS).contains(&slot) {
        1u128 << (slot - 1)
    } else {
        0
    }
}

/// Reject a slot outside `1..=GROUP_SLOTS`, so a caller that computed
/// one wrong hears about it instead of writing to nothing. Shared by
/// every backend's group methods.
pub fn check_group_slot(slot: u32) -> crate::DawResult<()> {
    if (1..=GROUP_SLOTS).contains(&slot) {
        Ok(())
    } else {
        Err(crate::DawError::out_of_range(
            slot,
            GROUP_SLOTS,
            "track-group slot",
        ))
    }
}

impl TrackGrouping {
    /// The 25 `GROUP_FLAGS` fields in REAPER's order, each as
    /// (lead-or-flag mask, follow mask) accessors. Field n (1-based)
    /// is `RPP_FIELDS[n - 1]`.
    const RPP_FIELDS: [fn(&Self) -> u128; 25] = [
        |g| g.volume_lead,
        |g| g.volume_follow,
        |g| g.pan_lead,
        |g| g.pan_follow,
        |g| g.mute_lead,
        |g| g.mute_follow,
        |g| g.solo_lead,
        |g| g.solo_follow,
        |g| g.recarm_lead,
        |g| g.recarm_follow,
        |g| g.polarity_lead,
        |g| g.polarity_follow,
        |g| g.automode_lead,
        |g| g.automode_follow,
        |g| g.volume_reverse,
        |g| g.pan_reverse,
        |g| g.no_lead_when_follow,
        |g| g.width_reverse,
        |g| g.width_lead,
        |g| g.width_follow,
        |g| g.vca_lead,
        |g| g.vca_follow,
        |g| g.vca_prefx_follow,
        |g| g.media_edit_lead,
        |g| g.media_edit_follow,
    ];

    const RPP_FIELDS_MUT: [fn(&mut Self) -> &mut u128; 25] = [
        |g| &mut g.volume_lead,
        |g| &mut g.volume_follow,
        |g| &mut g.pan_lead,
        |g| &mut g.pan_follow,
        |g| &mut g.mute_lead,
        |g| &mut g.mute_follow,
        |g| &mut g.solo_lead,
        |g| &mut g.solo_follow,
        |g| &mut g.recarm_lead,
        |g| &mut g.recarm_follow,
        |g| &mut g.polarity_lead,
        |g| &mut g.polarity_follow,
        |g| &mut g.automode_lead,
        |g| &mut g.automode_follow,
        |g| &mut g.volume_reverse,
        |g| &mut g.pan_reverse,
        |g| &mut g.no_lead_when_follow,
        |g| &mut g.width_reverse,
        |g| &mut g.width_lead,
        |g| &mut g.width_follow,
        |g| &mut g.vca_lead,
        |g| &mut g.vca_follow,
        |g| &mut g.vca_prefx_follow,
        |g| &mut g.media_edit_lead,
        |g| &mut g.media_edit_follow,
    ];

    /// 1-based `GROUP_FLAGS` field numbers of a family's (lead, follow).
    const fn family_fields(family: GroupFamily) -> (usize, usize) {
        match family {
            GroupFamily::Volume => (1, 2),
            GroupFamily::Pan => (3, 4),
            GroupFamily::Mute => (5, 6),
            GroupFamily::Solo => (7, 8),
            GroupFamily::RecArm => (9, 10),
            GroupFamily::Polarity => (11, 12),
            GroupFamily::AutoMode => (13, 14),
            GroupFamily::Width => (19, 20),
            GroupFamily::Vca => (21, 22),
            GroupFamily::MediaEdit => (24, 25),
        }
    }

    /// 1-based `GROUP_FLAGS` field number of a modifier.
    const fn modifier_field(modifier: GroupModifier) -> usize {
        match modifier {
            GroupModifier::VolumeReverse => 15,
            GroupModifier::PanReverse => 16,
            GroupModifier::NoLeadWhenFollow => 17,
            GroupModifier::WidthReverse => 18,
            GroupModifier::VcaFollowPreFx => 23,
        }
    }

    /// Whether every mask is empty (track belongs to no groups).
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Slots the track is in through any family or modifier.
    pub fn member_mask(&self) -> u128 {
        Self::RPP_FIELDS.iter().fold(0, |acc, f| acc | f(self))
    }

    /// Lead mask of `family`.
    pub fn lead(&self, family: GroupFamily) -> u128 {
        Self::RPP_FIELDS[Self::family_fields(family).0 - 1](self)
    }

    /// Follow mask of `family`.
    pub fn follow(&self, family: GroupFamily) -> u128 {
        Self::RPP_FIELDS[Self::family_fields(family).1 - 1](self)
    }

    /// The track's role in `family` for a 1-based `slot`. A track
    /// that has both bits (REAPER allows it) reads as `Lead`.
    pub fn role(&self, family: GroupFamily, slot: u32) -> GroupRole {
        let bit = group_slot_bit(slot);
        if self.lead(family) & bit != 0 {
            GroupRole::Lead
        } else if self.follow(family) & bit != 0 {
            GroupRole::Follow
        } else {
            GroupRole::None
        }
    }

    /// Set the track's role in `family` for a 1-based `slot`; lead and
    /// follow are exclusive per slot, so the other bit is cleared.
    /// A slot outside `1..=GROUP_SLOTS` is a no-op.
    pub fn set_role(&mut self, family: GroupFamily, slot: u32, role: GroupRole) {
        let bit = group_slot_bit(slot);
        let (lead, follow) = Self::family_fields(family);
        let (set_lead, set_follow) = match role {
            GroupRole::Lead => (true, false),
            GroupRole::Follow => (false, true),
            GroupRole::None => (false, false),
        };
        Self::set_bit(Self::RPP_FIELDS_MUT[lead - 1](self), bit, set_lead);
        Self::set_bit(Self::RPP_FIELDS_MUT[follow - 1](self), bit, set_follow);
    }

    /// Make the track a mutual member of `family` in `slot` — both lead
    /// and follow bits — or clear both. The all-families form of this is
    /// `Tracks::set_group_membership`.
    pub fn set_member(&mut self, family: GroupFamily, slot: u32, member: bool) {
        let bit = group_slot_bit(slot);
        let (lead, follow) = Self::family_fields(family);
        Self::set_bit(Self::RPP_FIELDS_MUT[lead - 1](self), bit, member);
        Self::set_bit(Self::RPP_FIELDS_MUT[follow - 1](self), bit, member);
    }

    /// Whether `modifier` is on for a 1-based `slot`.
    pub fn modifier(&self, modifier: GroupModifier, slot: u32) -> bool {
        Self::RPP_FIELDS[Self::modifier_field(modifier) - 1](self) & group_slot_bit(slot) != 0
    }

    /// Switch `modifier` on or off for a 1-based `slot`.
    pub fn set_modifier(&mut self, modifier: GroupModifier, slot: u32, enabled: bool) {
        let bit = group_slot_bit(slot);
        Self::set_bit(
            Self::RPP_FIELDS_MUT[Self::modifier_field(modifier) - 1](self),
            bit,
            enabled,
        );
    }

    fn set_bit(mask: &mut u128, bit: u128, on: bool) {
        if on {
            *mask |= bit;
        } else {
            *mask &= !bit;
        }
    }

    /// Decode `GROUP_FLAGS` (`low`, slots 1–32) and `GROUP_FLAGS_HIGH`
    /// (`high`, slots 33–64). Trailing zero fields are omitted by
    /// REAPER, so either slice may be short.
    pub fn from_rpp_fields(low: &[u32], high: &[u32]) -> Self {
        let mut g = Self::default();
        for (i, set) in Self::RPP_FIELDS_MUT.iter().enumerate() {
            let lo = u128::from(low.get(i).copied().unwrap_or(0));
            let hi = u128::from(high.get(i).copied().unwrap_or(0));
            *set(&mut g) = lo | (hi << 32);
        }
        g
    }

    /// Slots this track is in that no `.RPP` line can carry — 65–128.
    ///
    /// Nothing on disk and nothing in the SDK says how REAPER stores
    /// them, so [`Self::to_rpp_fields`] encodes 1–64 only and a caller
    /// that saves to a file asks this first rather than losing a group
    /// silently. `0` means the whole membership survives a save.
    pub fn slots_beyond_rpp(&self) -> u128 {
        const RPP_SLOTS: u128 = u64::MAX as u128;
        self.member_mask() & !RPP_SLOTS
    }

    /// Encode as (`GROUP_FLAGS`, `GROUP_FLAGS_HIGH`) fields, trailing
    /// zeros trimmed the way REAPER writes them; an empty Vec means
    /// the line is not written. Slots 65–128 have no known `.RPP` line
    /// and are dropped — [`Self::slots_beyond_rpp`] reports them.
    ///
    /// A track grouped only in 33–64 still gets a one-field
    /// `GROUP_FLAGS 0`: REAPER has never been observed writing a
    /// `GROUP_FLAGS_HIGH` with no `GROUP_FLAGS` above it.
    pub fn to_rpp_fields(&self) -> (Vec<u32>, Vec<u32>) {
        let mut low: Vec<u32> = Vec::with_capacity(25);
        let mut high: Vec<u32> = Vec::with_capacity(25);
        for get in Self::RPP_FIELDS {
            let v = get(self);
            low.push((v & u128::from(u32::MAX)) as u32);
            high.push(((v >> 32) & u128::from(u32::MAX)) as u32);
        }
        let trim = |v: &mut Vec<u32>| {
            while v.last() == Some(&0) {
                v.pop();
            }
        };
        trim(&mut low);
        trim(&mut high);
        if low.is_empty() && !high.is_empty() {
            low.push(0);
        }
        (low, high)
    }
}

/// Fixed-lane display mode (REAPER 7's lane-button cycle:
/// show-one-lane → small lanes → big lanes).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Facet)]
pub enum LaneDisplay {
    /// All lanes visible, compact rows.
    #[default]
    Small = 0,
    /// All lanes visible, each at full item height.
    Big = 1,
    /// Only the playing lane is shown, full height.
    One = 2,
}

/// Input monitoring mode for a track.
///
/// Controls whether the track's input signal is monitored (passed through
/// to FX and output) during recording/playback.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Facet)]
pub enum InputMonitoringMode {
    /// No input monitoring.
    #[default]
    Off,
    /// Always monitor input (even when not playing/recording).
    Normal,
    /// Only monitor when not playing (tape-style auto-monitoring).
    NotWhenPlaying,
}

/// Record input source for a track.
///
/// Simplified representation of REAPER's I_RECINPUT encoding.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub enum RecordInput {
    /// No input (-1).
    None,
    /// MIDI input from a specific device on a specific channel.
    /// device_id: None = all devices. channel: None = all channels.
    /// device_id 62 = Virtual MIDI Keyboard (VKB).
    Midi {
        device_id: Option<u8>,
        channel: Option<u8>,
    },
    /// Live audio input from a hardware input channel.
    ///
    /// `channel` is the 0-based input-device channel the track records /
    /// monitors from. The engine taps this channel of its open input
    /// stream and feeds it into the track's bus (and FX chain).
    Audio { channel: u32 },
    /// Raw I_RECINPUT value for other input types.
    Raw(i32),
}

impl RecordInput {
    /// MIDI from the Virtual MIDI Keyboard on all channels.
    ///
    /// This is the input source needed for `StuffMIDIMessage` with
    /// `VirtualMidiKeyboard` target to reach the track's FX chain.
    pub fn midi_virtual_keyboard() -> Self {
        Self::Midi {
            device_id: Some(62),
            channel: None,
        }
    }

    /// MIDI from all devices on all channels.
    pub fn midi_all() -> Self {
        Self::Midi {
            device_id: None,
            channel: None,
        }
    }
}

#[cfg(test)]
mod grouping_tests {
    use super::{GroupFamily, GroupRole, TrackGrouping};

    /// The REAPER-saved fixture from `docs/research/track-groups.md`: a
    /// lead of everything in group 1 writes fields 1,3,5,7,9,11,13,19;
    /// its follower writes 2,4,…,14,20; VCA rides 21/22.
    #[test]
    fn rpp_fields_follow_reapers_order() {
        let lead = TrackGrouping::from_rpp_fields(
            &[1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1],
            &[],
        );
        for fam in GroupFamily::ALL {
            let expect = if fam == GroupFamily::Vca || fam == GroupFamily::MediaEdit {
                GroupRole::None
            } else {
                GroupRole::Lead
            };
            assert_eq!(lead.role(fam, 1), expect, "{fam:?} lead");
        }
        assert_eq!(
            lead.to_rpp_fields(),
            (
                vec![1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1],
                vec![]
            )
        );

        let vca_follow = TrackGrouping::from_rpp_fields(&[0; 21], &[0; 22]);
        assert!(vca_follow.is_empty());
        let mut g = TrackGrouping::default();
        g.set_role(GroupFamily::Vca, 3, GroupRole::Follow);
        assert_eq!(g.to_rpp_fields().0.len(), 22, "field 22 = VCA follow");
        assert_eq!(g.to_rpp_fields().0[21], 4, "group 3 = bit 2");
    }

    /// Groups 33–64 live in the `_HIGH` line; 65–128 have no line
    /// (see `to_rpp_fields`) but the masks still hold them.
    #[test]
    fn slots_span_all_128() {
        let mut g = TrackGrouping::default();
        g.set_role(GroupFamily::Mute, 33, GroupRole::Lead);
        g.set_role(GroupFamily::Mute, 128, GroupRole::Follow);
        assert_eq!(g.role(GroupFamily::Mute, 33), GroupRole::Lead);
        assert_eq!(g.role(GroupFamily::Mute, 128), GroupRole::Follow);
        assert_eq!(g.role(GroupFamily::Mute, 1), GroupRole::None);
        let (low, high) = g.to_rpp_fields();
        assert_eq!(high[4], 1, "slot 33 = bit 0 of the high line, field 5");
        assert_eq!(
            low,
            vec![0],
            "a high-only track still gets a GROUP_FLAGS line"
        );
        assert_eq!(
            g.slots_beyond_rpp(),
            1u128 << 127,
            "slot 128 cannot be written to a file; slot 33 can"
        );
        assert_eq!(g.member_mask(), (1u128 << 32) | (1u128 << 127));
        // Lead and follow of one family are exclusive per slot.
        g.set_role(GroupFamily::Mute, 33, GroupRole::Follow);
        assert_eq!(g.mute_lead, 0);
        g.set_role(GroupFamily::Mute, 33, GroupRole::None);
        assert_eq!(g.mute_follow, 1u128 << 127);
    }
}
