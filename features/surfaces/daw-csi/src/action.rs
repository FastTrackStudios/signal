//! The bindable action registry — every name a zone file can bind a
//! widget to. CSI's `actions_` map, as a Facet enum so Styx parses
//! actions directly: bare variants are `@TrackVolume`-style tags,
//! parameterized ones carry a payload (`@Bank{amount -8}`).

use facet::Facet;

/// Automation modes nameable from zone files (maps onto
/// `daw_proto::AutomationMode`).
#[repr(u8)]
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoMode {
    Trim,
    Read,
    Touch,
    Write,
    Latch,
}

/// One bindable action. Strip-context actions resolve "which track"
/// from the widget's strip via the navigator; global actions ignore
/// the strip.
#[repr(u8)]
#[derive(Facet, Clone, Debug, PartialEq)]
pub enum Action {
    // ── Strip: control ──────────────────────────────────────────────
    /// Fader ↔ track volume (motor feedback follows).
    TrackVolume,
    /// V-pot ↔ track pan (ring feedback follows).
    TrackPan,
    /// Recenter pan (v-pot press convention).
    TrackPanReset,
    /// Toggle mute (LED follows).
    TrackMute,
    /// Toggle solo (LED follows).
    TrackSolo,
    /// Toggle record arm (LED follows).
    TrackRecordArm,
    /// Exclusive-select the strip's track (LED follows selection).
    /// CSI's `TrackUniqueSelect`.
    TrackSelect,
    /// Additive select (CSI's `TrackSelect`).
    TrackSelectAdditive,
    /// CSI `TrackToggleFolderSpill`: drill into a folder / pop back
    /// out of the current spill. Plain tracks fall through to
    /// [`Action::TrackSelect`].
    FolderSpill,
    /// CSI `TrackToggleVCASpill`: spill a VCA lead's followers across
    /// the strips / pop back out. Non-leads fall through to select.
    VcaSpill,
    /// Toggle polarity / phase invert (LED lit = inverted).
    TrackTogglePolarity,
    /// Additive range select from the last exclusive selection to the
    /// pressed strip (CSI's `TrackRangeSelect`).
    TrackRangeSelect,

    // ── Strip: sends (strip = send slot of the selected track) ─────
    /// Fader/v-pot ↔ send level of the strip's send slot.
    SendVolume,
    /// V-pot ↔ send pan.
    SendPan,
    /// Toggle send mute.
    SendMute,

    // ── Strip: receives (strip = receive slot of the selected track) ─
    ReceiveVolume,
    ReceivePan,
    ReceiveMute,

    // ── Strip: FX menu (strip = FX slot of the selected track) ─────
    /// Focus the strip's FX and hop to its param zone (CSI's
    /// `GoFXSlot`). `zone` names the param zone to enter.
    FxMenuSelect {
        zone: String,
    },
    /// Toggle the strip's FX bypass (LED lit = enabled).
    FxMenuBypass,

    // ── Strip: FX params (strip = param slot of the focused FX) ────
    /// V-pot/fader ↔ the strip's parameter of the focused FX.
    FxParam,
    /// Next / previous preset of the focused FX.
    FxNextPreset,
    FxPrevPreset,
    /// V-pot ↔ the LAST-TOUCHED FX parameter, from anywhere (CSI's
    /// `LastTouchedFXParam`).
    LastTouchedFxParam,

    // ── Strip: displays ─────────────────────────────────────────────
    /// Track name on an LCD row.
    TrackName,
    /// Pan position ("12L" / "C" / "40R").
    PanDisplay,
    /// Volume in dB.
    VolumeDisplay,
    /// "FOLDR>" on folder tracks, pan otherwise (folder-mode bottom
    /// row).
    FolderIndicator,
    /// Send destination track name (sends zones).
    SendNameDisplay,
    /// Send level in dB.
    SendVolumeDisplay,
    /// Send pan position.
    SendPanDisplay,
    /// Receive source track name.
    ReceiveNameDisplay,
    /// Receive level in dB.
    ReceiveVolumeDisplay,
    /// Receive pan position.
    ReceivePanDisplay,
    /// FX name (FX-menu zones).
    FxMenuNameDisplay,
    /// Parameter name of the focused FX (FX param zones).
    FxParamNameDisplay,
    /// Plugin-formatted parameter value ("−12.0 dB").
    FxParamValueDisplay,
    /// A literal string.
    Fixed {
        text: String,
    },
    /// Blank the cell.
    Blank,

    // ── Master / global control ─────────────────────────────────────
    /// Master fader ↔ master volume.
    MasterVolume,
    Play,
    Stop,
    Record,
    ToggleLoop,
    /// Move the edit position by a fixed amount (transport rewind /
    /// fast-forward buttons).
    NudgePosition {
        seconds: f64,
    },
    /// Jog wheel: seconds per encoder tick (sign follows rotation).
    JogPosition {
        seconds_per_tick: f64,
    },
    /// Bank the navigator window by `amount` strips (±1 channel,
    /// ±strip-count page). Oversized amounts clamp — CSI's
    /// `Bank Track ±999` = jump to first/last page.
    Bank {
        amount: i32,
    },
    /// Bank the SEND-slot window (sends zones).
    BankSends {
        amount: i32,
    },
    /// Bank the RECEIVE-slot window (receives zones).
    BankReceives {
        amount: i32,
    },
    /// Bank the FX-PARAM window (FX param zones; plugins routinely
    /// expose more than 8 parameters).
    BankFxParams {
        amount: i32,
    },
    /// Un-solo every track (CSI `ClearAllSolo`).
    ClearAllSolo,
    /// Un-mute every track.
    UnmuteAll,
    /// Activate another zone by name (CSI `GoZone`).
    GoZone {
        zone: String,
    },
    /// Save the project.
    SaveProject,
    Undo,
    Redo,
    /// Run a registered command by id (CSI's `Reaper NNNN` analog).
    Command {
        id: String,
    },
    /// Drop a marker at the playhead.
    AddMarker,
    /// Jump to the previous / next marker.
    PrevMarker,
    NextMarker,
    /// Latch the FLIP layer on/off (zone bindings under `flip+` take
    /// over; LED follows the latch).
    Flip,
    /// Latch the TOGGLE layer (CSI's `Toggle+` second function).
    Toggle,
    /// Surface bank follows UI track selection (CSI ScrollLink).
    ToggleScrollLink,
    /// Set the SELECTED track's automation mode (CSI `TrackAutoMode`;
    /// the READ/WRITE/TRIM/TOUCH/LATCH buttons). LED lit when the
    /// selected track is in that mode.
    TrackAutoMode {
        mode: AutoMode,
    },
    /// Cycle the strip track's input monitoring (off → on → tape).
    CycleInputMonitor,
    /// Toggle the metronome / click (LED follows).
    ToggleMetronome,
    /// Cycle the 7-segment readout: time ↔ bars/beats.
    CycleTimeDisplay,
    /// Bound but intentionally inert (CSI `NoAction`).
    NoAction,
}
