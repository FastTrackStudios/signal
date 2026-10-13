//! Serialization trait for RPP chunk types.
//!
//! Provides a consistent interface for serializing REAPER data structures
//! back to valid RPP chunk text.

use super::envelope::Envelope;
use super::item::{
    ChannelMode, FadeCurveType, Item, PitchMode, SoloState, SourceBlock, SourceType, Take,
};
use super::marker_region::{MarkerRegion, MarkerRegionCollection};
use super::project::ReaperProject;
use super::time_tempo::{TempoTimeEnvelope, TempoTimePoint};
use super::track::{
    AutomationMode, FolderState, FreeMode, MonitorMode, RecordMode, Track, TrackSoloState,
};

/// Trait for types that can serialize themselves to valid REAPER RPP chunk text.
///
/// # Example
///
/// ```ignore
/// use dawfile_reaper::RppSerialize;
///
/// let track = Track::default();
/// let rpp_text = track.to_rpp_string();
/// // => "<TRACK\n  NAME \"\"\n  ...\n>\n"
/// ```
pub trait RppSerialize {
    /// Serialize to RPP text with no leading indentation.
    fn to_rpp_string(&self) -> String {
        let mut out = String::new();
        self.write_rpp(&mut out, "");
        out
    }

    /// Write RPP text to a buffer at the given indentation prefix.
    ///
    /// Implementations should prepend `indent` to each top-level line they emit,
    /// and pass a deeper indent (e.g. `format!("{}  ", indent)`) to nested children.
    fn write_rpp(&self, out: &mut String, indent: &str);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn b(v: bool) -> i32 {
    if v { 1 } else { 0 }
}

fn rpp_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Format a string as an RPP token: bare when it's a "simple" identifier
/// (alphanumeric / underscore / dash / dot, no spaces or special chars),
/// otherwise wrap in double quotes with `rpp_escape`. The official PT
/// Reaper Converter uses the same rule (`NAME ClickPrint_03` vs
/// `NAME "10 REASON WHY demo"`).
pub(crate) fn rpp_token(s: &str) -> String {
    let needs_quotes = s.is_empty()
        || s.chars()
            .any(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'));
    if needs_quotes {
        format!("\"{}\"", rpp_escape(s))
    } else {
        s.to_string()
    }
}

fn fade_curve_to_i32(c: &FadeCurveType) -> i32 {
    match c {
        FadeCurveType::Linear => 0,
        FadeCurveType::Square => 1,
        FadeCurveType::SlowStartEnd => 2,
        FadeCurveType::FastStart => 3,
        FadeCurveType::FastEnd => 4,
        FadeCurveType::Bezier => 5,
        FadeCurveType::Unknown(v) => *v,
    }
}

fn solo_state_to_i32(s: &SoloState) -> i32 {
    match s {
        SoloState::NotSoloed => 0,
        SoloState::Soloed => -1,
        SoloState::SoloOverridden => 1,
        SoloState::Unknown(v) => *v,
    }
}

fn pitch_mode_to_i32(p: &PitchMode) -> i32 {
    match p {
        PitchMode::ProjectDefault => -1,
        PitchMode::SoundTouchPreset1 => 0,
        PitchMode::SoundTouchPreset2 => 1,
        PitchMode::SoundTouchPreset3 => 2,
        PitchMode::DiracLE(n) => 65536 + *n as i32,
        PitchMode::LowQualityWindowed(n) => 131072 + *n as i32,
        PitchMode::ElastiquePro(n) => 196608 + *n as i32,
        PitchMode::ElastiqueEfficient(n) => 262144 + *n as i32,
        PitchMode::ElastiqueSoloist(n) => 327680 + *n as i32,
        PitchMode::Elastique21Pro(n) => 393216 + *n as i32,
        PitchMode::Elastique21Efficient(n) => 458752 + *n as i32,
        PitchMode::Elastique21Soloist(n) => 524288 + *n as i32,
        PitchMode::Unknown(v) => *v,
    }
}

fn automation_mode_to_i32(m: &AutomationMode) -> i32 {
    match m {
        AutomationMode::TrimRead => 0,
        AutomationMode::Read => 1,
        AutomationMode::Touch => 2,
        AutomationMode::Write => 3,
        AutomationMode::Latch => 4,
        AutomationMode::Unknown(v) => *v,
    }
}

fn track_solo_to_i32(s: &TrackSoloState) -> i32 {
    match s {
        TrackSoloState::NoSolo => 0,
        TrackSoloState::Solo => 1,
        TrackSoloState::SoloInPlace => 2,
        TrackSoloState::Unknown(v) => *v,
    }
}

fn folder_state_to_i32(f: &FolderState) -> i32 {
    match f {
        FolderState::Regular => 0,
        FolderState::FolderParent => 1,
        FolderState::LastInFolder => 2,
        FolderState::Unknown(v) => *v,
    }
}

fn free_mode_to_i32(f: &FreeMode) -> i32 {
    match f {
        FreeMode::Disabled => 0,
        FreeMode::FreeItemPositioning => 1,
        FreeMode::FixedItemLanes => 2,
        FreeMode::Unknown(v) => *v,
    }
}

fn monitor_mode_to_i32(m: &MonitorMode) -> i32 {
    match m {
        MonitorMode::Off => 0,
        MonitorMode::On => 1,
        MonitorMode::Auto => 2,
        MonitorMode::Unknown(v) => *v,
    }
}

fn record_mode_to_i32(r: &RecordMode) -> i32 {
    match r {
        RecordMode::Input => 0,
        RecordMode::OutputStereo => 1,
        RecordMode::DisableMonitor => 2,
        RecordMode::OutputStereoLatencyComp => 3,
        RecordMode::OutputMidi => 4,
        RecordMode::OutputMono => 5,
        RecordMode::OutputMonoLatencyComp => 6,
        RecordMode::MidiOverdub => 7,
        RecordMode::MidiReplace => 8,
        RecordMode::MidiTouchReplace => 9,
        RecordMode::OutputMultichannel => 10,
        RecordMode::OutputMultichannelLatencyComp => 11,
        RecordMode::Unknown(v) => *v,
    }
}

fn envelope_shape_to_i32(s: &super::envelope::EnvelopePointShape) -> i32 {
    use super::envelope::EnvelopePointShape;
    match s {
        EnvelopePointShape::Linear => 0,
        EnvelopePointShape::Square => 1,
        EnvelopePointShape::SlowStartEnd => 2,
        EnvelopePointShape::FastStart => 3,
        EnvelopePointShape::FastEnd => 4,
        EnvelopePointShape::Bezier => 5,
        EnvelopePointShape::Default => -1,
    }
}

/// Inverse of `ChannelMode::from(i32)` — the `CHANMODE` field value.
fn channel_mode_to_i32(m: &ChannelMode) -> i32 {
    match m {
        ChannelMode::Normal => 0,
        ChannelMode::ReverseStereo => 1,
        ChannelMode::MonoDownmix => 2,
        ChannelMode::MonoLeft => 3,
        ChannelMode::MonoRight => 4,
        // Mono channels 3..=64 encode as ch+2, 65..=128 as ch+66.
        ChannelMode::MonoChannel(ch) => {
            let ch = i32::from(*ch);
            if ch <= 64 { ch + 2 } else { ch + 66 }
        }
        // Stereo pairs 1..=64 encode as ch+66, 67..=128 as ch+128.
        ChannelMode::StereChannel(ch) => {
            let ch = i32::from(*ch);
            if ch <= 64 { ch + 66 } else { ch + 128 }
        }
        ChannelMode::Unknown(v) => *v,
    }
}

fn item_timebase_to_i32(t: &super::item::ItemTimebase) -> i32 {
    match t {
        super::item::ItemTimebase::ProjectDefault => -1,
        super::item::ItemTimebase::Time => 0,
        super::item::ItemTimebase::Beats => 1,
        super::item::ItemTimebase::Unknown(v) => *v,
    }
}

/// Re-indent a raw content block so each line gets the given indent prefix.
/// Structural lines (`<TAG` and `>`) get the base indent; content lines get one deeper.
fn write_raw_block(out: &mut String, raw: &str, indent: &str) {
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('<') || trimmed == ">" {
            out.push_str(indent);
            out.push_str(trimmed);
            out.push('\n');
        } else {
            out.push_str(indent);
            out.push_str("  ");
            out.push_str(trimmed);
            out.push('\n');
        }
    }
}

// ---------------------------------------------------------------------------
// SourceBlock
// ---------------------------------------------------------------------------

impl RppSerialize for SourceBlock {
    fn write_rpp(&self, out: &mut String, indent: &str) {
        // If we have raw content, use it for fidelity
        if !self.raw_content.is_empty() {
            write_raw_block(out, &self.raw_content, indent);
            return;
        }

        let tag = match &self.source_type {
            SourceType::Wave => "WAVE",
            SourceType::Mp3 => "MP3",
            SourceType::Midi => "MIDI",
            SourceType::Video => "VIDEO",
            SourceType::Section => "SECTION",
            SourceType::Empty => "EMPTY",
            SourceType::Flac => "FLAC",
            SourceType::Vorbis => "VORBIS",
            SourceType::OfflineWave => "_OFFLINE_WAVE",
            SourceType::Unknown(s) => s.as_str(),
        };

        let inner = format!("{}  ", indent);
        out.push_str(&format!("{}<SOURCE {}\n", indent, tag));

        if let Some(midi) = &self.midi_data {
            // Serialize MIDI source data
            out.push_str(&format!(
                "{}HASDATA {} {} {}\n",
                inner,
                if midi.has_data { 1 } else { 0 },
                midi.ticks_per_qn,
                midi.ticks_timebase.as_deref().unwrap_or("QN")
            ));
            if let Some(interp) = midi.cc_interp {
                out.push_str(&format!("{}CCINTERP {}\n", inner, interp));
            }
            if let Some(ref guid) = midi.pooled_evts_guid {
                out.push_str(&format!("{}POOLEDEVTS {}\n", inner, guid));
            }

            // Write events in stream order (preserves interleaving)
            if !midi.event_stream.is_empty() {
                for evt in &midi.event_stream {
                    match evt {
                        crate::types::item::MidiSourceEvent::Midi(e) => {
                            out.push_str(&format!("{}E {}", inner, e.delta_ticks));
                            for byte in &e.bytes {
                                out.push_str(&format!(" {:02x}", byte));
                            }
                            out.push('\n');
                        }
                        crate::types::item::MidiSourceEvent::Extended(x) => {
                            let tag_char = if x.selected { "x" } else { "X" };
                            out.push_str(&format!(
                                "{}<{} {}\n",
                                inner,
                                tag_char,
                                x.fields.join(" ")
                            ));
                            for line in &x.data_lines {
                                out.push_str(&format!("{}{}\n", inner, line));
                            }
                            out.push_str(&format!("{}>\n", inner));
                        }
                    }
                }
            } else {
                // Fallback: write from events vec directly
                for e in &midi.events {
                    out.push_str(&format!("{}E {}", inner, e.delta_ticks));
                    for byte in &e.bytes {
                        out.push_str(&format!(" {:02x}", byte));
                    }
                    out.push('\n');
                }
            }

            if let Some(ref ignt) = midi.ignore_tempo {
                out.push_str(&format!(
                    "{}IGNTEMPO {} {} {} {}\n",
                    inner,
                    if ignt.enabled { 1 } else { 0 },
                    ignt.bpm,
                    ignt.numerator,
                    ignt.denominator
                ));
            }
            for vl in &midi.vel_lanes {
                out.push_str(&format!(
                    "{}VELLANE {} {} {}\n",
                    inner, vl.lane_type, vl.midi_editor_height, vl.inline_editor_height
                ));
            }
            if let Some(ref bpf) = midi.bank_program_file {
                out.push_str(&format!("{}BANKPROGRAMFILE \"{}\"\n", inner, bpf));
            }
            if let Some(ref cev) = midi.cfg_edit_view {
                out.push_str(&format!("{}CFGEDITVIEW {}\n", inner, cev.join(" ")));
            }
            if let Some(ref ce) = midi.cfg_edit {
                out.push_str(&format!("{}CFGEDIT {}\n", inner, ce.join(" ")));
            }
            if let Some(ref ef) = midi.evt_filter {
                out.push_str(&format!("{}EVTFILTER {}\n", inner, ef.join(" ")));
            }
            if let Some(ref guid) = midi.guid {
                out.push_str(&format!("{}GUID {}\n", inner, guid));
            }
            for line in &midi.unknown_lines {
                out.push_str(&format!("{}{}\n", inner, line));
            }
        } else if !self.file_path.is_empty() {
            out.push_str(&format!(
                "{}FILE \"{}\"\n",
                inner,
                rpp_escape(&self.file_path)
            ));
        }
        out.push_str(&format!("{}>\n", indent));
    }
}

// ---------------------------------------------------------------------------
// Take
// ---------------------------------------------------------------------------

impl Take {
    /// Serialize this take. The `emit_marker` flag controls whether a
    /// `TAKE` / `TAKE SEL` line is emitted before the properties — this
    /// must be `false` for the first take of an item (REAPER reads the
    /// first take's properties directly off the ITEM block; emitting a
    /// `TAKE` marker for it would create a phantom empty take #0).
    fn write_take(&self, out: &mut String, indent: &str, emit_marker: bool) {
        if emit_marker {
            if self.is_selected {
                out.push_str(&format!("{}TAKE SEL\n", indent));
            } else {
                out.push_str(&format!("{}TAKE\n", indent));
            }
        }
        if !self.name.is_empty() {
            out.push_str(&format!("{}NAME {}\n", indent, rpp_token(&self.name)));
        }
        if let Some(vp) = &self.volpan {
            out.push_str(&format!(
                "{}VOLPAN {} {} {} {}\n",
                indent, vp.item_trim, vp.take_pan, vp.take_volume, vp.take_pan_law
            ));
        }
        if self.slip_offset != 0.0 {
            out.push_str(&format!("{}SOFFS {}\n", indent, self.slip_offset));
        }
        if let Some(pr) = &self.playrate {
            out.push_str(&format!(
                "{}PLAYRATE {} {} {} {} {} {}\n",
                indent,
                pr.rate,
                b(pr.preserve_pitch),
                pr.pitch_adjust,
                pitch_mode_to_i32(&pr.pitch_mode),
                pr.unknown_field_5,
                pr.unknown_field_6
            ));
        }
        out.push_str(&format!(
            "{}CHANMODE {}\n",
            indent,
            channel_mode_to_i32(&self.channel_mode)
        ));
        if let Some(color) = self.take_color {
            out.push_str(&format!("{}TAKECOLOR {}\n", indent, color));
        }
        if let Some(guid) = &self.take_guid {
            out.push_str(&format!("{}GUID {}\n", indent, guid));
        }
        if let Some(rp) = self.rec_pass {
            out.push_str(&format!("{}RECPASS {}\n", indent, rp));
        }
        // REAPER writes a take's stretch markers between its scalar fields
        // and its `<SOURCE>` block.
        for sm in &self.stretch_markers {
            out.push_str(&format!(
                "{}SM {} {}",
                indent, sm.position, sm.source_position
            ));
            if let Some(rate) = sm.rate {
                out.push_str(&format!(" {}", rate));
            }
            out.push('\n');
        }
        if let Some(source) = &self.source {
            source.write_rpp(out, indent);
        }
        // `<EXT>` and any other nested block read under this take.
        for block in &self.extra_blocks {
            write_raw_block(out, block, indent);
        }
    }
}

// ---------------------------------------------------------------------------
// Item
// ---------------------------------------------------------------------------

/// The item's first take, as ONE value ready to be written inline.
///
/// An RPP item stores take #0's fields at item level — that is the
/// format, not a quirk — so the parser fills both `Item::{name, volpan,
/// slip_offset, playrate, channel_mode, take_guid, rec_pass,
/// stretch_markers}` and `Item::takes[0]`. The serializer must therefore
/// pick ONE of the two to write. Item-level values win where present
/// (the builder sets `Item::name` while `takes[0].name` holds the source
/// path); the take supplies everything the item has no slot for
/// (`take_color`, `source`, `extra_blocks`).
fn first_take_of(item: &Item) -> Take {
    let base = item.takes.first().cloned().unwrap_or_default();
    Take {
        is_selected: base.is_selected,
        name: if item.name.is_empty() {
            base.name
        } else {
            item.name.clone()
        },
        volpan: item.volpan.clone().or(base.volpan),
        slip_offset: if item.slip_offset == 0.0 {
            base.slip_offset
        } else {
            item.slip_offset
        },
        playrate: item.playrate.clone().or(base.playrate),
        channel_mode: if matches!(item.channel_mode, ChannelMode::Normal) {
            base.channel_mode
        } else {
            item.channel_mode
        },
        take_color: base.take_color,
        take_guid: item.take_guid.clone().or(base.take_guid),
        rec_pass: item.rec_pass.or(base.rec_pass),
        source: base.source,
        stretch_markers: if item.stretch_markers.is_empty() {
            base.stretch_markers
        } else {
            item.stretch_markers.clone()
        },
        extra_blocks: base.extra_blocks,
    }
}

impl RppSerialize for Item {
    fn write_rpp(&self, out: &mut String, indent: &str) {
        // If raw_content is available, use it for round-trip fidelity
        if !self.raw_content.is_empty() {
            write_raw_block(out, &self.raw_content, indent);
            return;
        }

        let inner = format!("{}  ", indent);

        out.push_str(&format!("{}<ITEM\n", indent));
        out.push_str(&format!("{}POSITION {}\n", inner, self.position));
        out.push_str(&format!("{}SNAPOFFS {}\n", inner, self.snap_offset));
        out.push_str(&format!("{}LENGTH {}\n", inner, self.length));
        out.push_str(&format!("{}LOOP {}\n", inner, b(self.loop_source)));
        out.push_str(&format!("{}ALLTAKES {}\n", inner, b(self.play_all_takes)));
        if let Some(color) = self.color {
            out.push_str(&format!("{}COLOR {}\n", inner, color));
        }
        if let Some(beat) = &self.beat {
            out.push_str(&format!("{}BEAT {}\n", inner, item_timebase_to_i32(beat)));
        }
        out.push_str(&format!("{}SEL {}\n", inner, b(self.selected)));

        if let Some(fi) = &self.fade_in {
            out.push_str(&format!(
                "{}FADEIN {} {} {} {} {} {} {}\n",
                inner,
                fade_curve_to_i32(&fi.curve_type),
                fi.time,
                fi.unknown_field_3,
                fi.unknown_field_4,
                fi.unknown_field_5,
                fi.unknown_field_6,
                fi.unknown_field_7
            ));
        }
        if let Some(fo) = &self.fade_out {
            out.push_str(&format!(
                "{}FADEOUT {} {} {} {} {} {} {}\n",
                inner,
                fade_curve_to_i32(&fo.curve_type),
                fo.time,
                fo.unknown_field_3,
                fo.unknown_field_4,
                fo.unknown_field_5,
                fo.unknown_field_6,
                fo.unknown_field_7
            ));
        }
        if let Some(m) = &self.mute {
            out.push_str(&format!(
                "{}MUTE {} {}\n",
                inner,
                b(m.muted),
                solo_state_to_i32(&m.solo_state)
            ));
        }
        // Vertical geometry. REAPER has NO `LANE` token in an ITEM at any
        // version — the per-item lane lives in `YPOS <y> <height> [mode]`,
        // which is also what `Item::lane` is derived from at parse time.
        // Emitting `LANE` made REAPER reject the whole project with
        // "Project tokens not recognized: LANE".
        if let Some(yp) = self.y_pos {
            out.push_str(&format!(
                "{}YPOS {} {} {}\n",
                inner, yp.y, yp.height, yp.mode
            ));
        }
        if let Some(iguid) = &self.item_guid {
            out.push_str(&format!("{}IGUID {}\n", inner, iguid));
        }
        if let Some(iid) = self.item_id {
            out.push_str(&format!("{}IID {}\n", inner, iid));
        }

        // Takes — REAPER reads the FIRST take's properties directly off the
        // ITEM block, with no `TAKE` marker; subsequent takes get one
        // (`TAKE SEL` for the active one). The item-level NAME / VOLPAN /
        // SOFFS / PLAYRATE / CHANMODE / GUID / RECPASS / SM fields ARE
        // take #0's — writing them here *and* letting the loop write
        // `takes[0]` is what emitted every take twice.
        first_take_of(self).write_take(out, &inner, false);
        for take in self.takes.iter().skip(1) {
            take.write_take(out, &inner, true);
        }

        out.push_str(&format!("{}>\n", indent));
    }
}

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------

impl RppSerialize for Envelope {
    fn write_rpp(&self, out: &mut String, indent: &str) {
        let inner = format!("{}  ", indent);

        out.push_str(&format!("{}<{}\n", indent, self.envelope_type));
        out.push_str(&format!("{}EGUID {}\n", inner, self.guid));
        out.push_str(&format!("{}ACT {} -1\n", inner, b(self.active)));
        out.push_str(&format!(
            "{}VIS {} {} 1\n",
            inner,
            b(self.visible),
            b(self.show_in_lane)
        ));
        out.push_str(&format!("{}LANEHEIGHT {} 0\n", inner, self.lane_height));
        out.push_str(&format!("{}ARM {}\n", inner, b(self.armed)));
        out.push_str(&format!("{}DEFSHAPE {} -1 -1\n", inner, self.default_shape));

        for pt in &self.points {
            out.push_str(&format!(
                "{}PT {:.6} {:.6} {}",
                inner,
                pt.position,
                pt.value,
                envelope_shape_to_i32(&pt.shape)
            ));
            if let Some(ts) = pt.time_sig {
                out.push_str(&format!(" {}", ts));
            }
            if let Some(sel) = pt.selected {
                out.push_str(&format!(" {}", b(sel)));
            }
            if let Some(unk) = pt.unknown_field_6 {
                out.push_str(&format!(" {}", unk));
            }
            if let Some(tension) = pt.bezier_tension {
                out.push_str(&format!(" {}", tension));
            }
            out.push('\n');
        }

        for ai in &self.automation_items {
            out.push_str(&format!(
                "{}POOLEDENVINST {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}\n",
                inner,
                ai.pool_index,
                ai.position,
                ai.length,
                ai.start_offset,
                ai.play_rate,
                b(ai.selected),
                ai.baseline,
                ai.amplitude,
                b(ai.loop_enabled),
                ai.position_qn,
                ai.length_qn,
                ai.instance_index,
                b(ai.muted),
                ai.start_offset_qn,
                ai.transition_time,
                ai.volume_envelope_max
            ));
        }

        for ext in &self.extension_data {
            write_raw_block(out, &ext.raw_content, &inner);
        }

        out.push_str(&format!("{}>\n", indent));
    }
}

// ---------------------------------------------------------------------------
// Track
// ---------------------------------------------------------------------------

impl RppSerialize for Track {
    fn write_rpp(&self, out: &mut String, indent: &str) {
        // If raw_content is available and non-empty, prefer it for fidelity
        if !self.raw_content.is_empty() {
            write_raw_block(out, &self.raw_content, indent);
            return;
        }

        let inner = format!("{}  ", indent);

        // Inline GUID form on the `<TRACK` line itself is what the official
        // PT Reaper Converter emits (and what REAPER produces when saving).
        // The redundant `TRACKID` line that follows is also emitted by
        // REAPER and keeps backwards-compatibility with older tooling that
        // looks only at the line form.
        if let Some(id) = &self.track_id {
            out.push_str(&format!("{}<TRACK {}\n", indent, id));
        } else {
            out.push_str(&format!("{}<TRACK\n", indent));
        }
        out.push_str(&format!("{}NAME {}\n", inner, rpp_token(&self.name)));
        if let Some(id) = &self.track_id {
            out.push_str(&format!("{}TRACKID {}\n", inner, id));
        }
        if let Some(pc) = self.peak_color {
            out.push_str(&format!("{}PEAKCOL {}\n", inner, pc));
        }
        if let Some(beat) = self.beat {
            out.push_str(&format!("{}BEAT {}\n", inner, beat));
        }
        // Only emit SEL when it's non-default. REAPER treats absent SEL as 0
        // (track not in the editor selection); we don't need to bake the
        // default in to every track and the official PT Reaper Converter
        // also omits it for unselected tracks.
        if self.selected {
            out.push_str(&format!("{}SEL {}\n", inner, b(self.selected)));
        }
        if self.locked {
            out.push_str(&format!("{}LOCK {}\n", inner, b(self.locked)));
        }
        out.push_str(&format!(
            "{}AUTOMODE {}\n",
            inner,
            automation_mode_to_i32(&self.automation_mode)
        ));

        if let Some(vp) = &self.volpan {
            // REAPER's TRACK-level VOLPAN takes 5 fields:
            //   vol pan pan_law pan_mode_2 show_pan_widget
            // The trailing two are reserved/UI flags; the official
            // PT Reaper Converter (and our setlist/convert paths)
            // emit `-1 1` as the defaults. The dawfile-reaper parser
            // (io.rs:226) reads only the first three and ignores
            // extras, so round-trip is lossless.
            out.push_str(&format!(
                "{}VOLPAN {} {} {} -1 1\n",
                inner, vp.volume, vp.pan, vp.pan_law
            ));
        }
        // Always emit MUTESOLO, even at the default (`0 0 0`). REAPER tolerates
        // its absence, but matching the official PT Reaper Converter (which
        // always emits it) keeps line-by-line diffs stable.
        let (m, s, sd) = match &self.mutesolo {
            Some(ms) => (b(ms.mute), track_solo_to_i32(&ms.solo), b(ms.solo_defeat)),
            None => (0, 0, 0),
        };
        out.push_str(&format!("{}MUTESOLO {} {} {}\n", inner, m, s, sd));
        if self.invert_phase {
            out.push_str(&format!("{}IPHASE {}\n", inner, b(self.invert_phase)));
        }
        if let Some(f) = &self.folder {
            out.push_str(&format!(
                "{}ISBUS {} {}\n",
                inner,
                folder_state_to_i32(&f.folder_state),
                f.indentation
            ));
        }
        if let Some(bc) = &self.bus_compact {
            out.push_str(&format!(
                "{}BUSCOMP {} {} {} {} {}\n",
                inner,
                bc.arrange_collapse,
                bc.mixer_collapse,
                bc.wiring_collapse,
                bc.wiring_x,
                bc.wiring_y
            ));
        }
        if let Some(sim) = &self.show_in_mixer {
            out.push_str(&format!(
                "{}SHOWINMIX {} {} {} {} {} {} {} {}\n",
                inner,
                b(sim.show_in_mixer),
                sim.unknown_field_2,
                sim.unknown_field_3,
                b(sim.show_in_track_list),
                sim.unknown_field_5,
                sim.unknown_field_6,
                sim.unknown_field_7,
                sim.unknown_field_8
            ));
        }
        if let Some(fm) = self.free_mode {
            out.push_str(&format!("{}FREEMODE {}\n", inner, free_mode_to_i32(&fm)));
        }
        // Track-group membership. REAPER omits the line entirely when a
        // track is in no group, and trims trailing zero fields; matching
        // that keeps an ungrouped project byte-identical through a
        // round-trip. Slots 65-128 have no known line — see
        // `TrackGrouping::to_rpp_fields`.
        for (key, fields) in [
            ("GROUP_FLAGS", self.group_flags.as_deref()),
            ("GROUP_FLAGS_HIGH", self.group_flags_high.as_deref()),
        ] {
            // `rposition` is both the "is this track in any group at
            // all" test and the trailing-zero trim: no non-zero field
            // means REAPER would not write the line.
            let Some(last) = fields.and_then(|f| f.iter().rposition(|v| *v != 0)) else {
                continue;
            };
            let joined = fields.unwrap_or_default()[..=last]
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            out.push_str(&format!("{inner}{key} {joined}\n"));
        }
        if let Some(fl) = &self.fixed_lanes {
            out.push_str(&format!(
                "{}FIXEDLANES {} {} {} {} {}\n",
                inner,
                fl.bitfield,
                b(fl.allow_editing),
                b(fl.show_play_only_lane),
                b(fl.mask_playback),
                fl.recording_behavior
            ));
        }
        if let Some(ls) = &self.lane_solo {
            out.push_str(&format!(
                "{}LANESOLO {} {} {} {} {} {} {} {}\n",
                inner,
                ls.playing_lanes,
                ls.unknown_field_2,
                ls.unknown_field_3,
                ls.unknown_field_4,
                ls.unknown_field_5,
                ls.unknown_field_6,
                ls.unknown_field_7,
                ls.unknown_field_8
            ));
        }
        if let Some(lr) = &self.lane_record {
            out.push_str(&format!(
                "{}LANEREC {} {} {}\n",
                inner, lr.record_enabled_lane, lr.comping_enabled_lane, lr.last_comping_lane
            ));
        }
        if let Some(ln) = &self.lane_names {
            // REAPER's format is names only — `LANENAME "Lane" C1 1 2 3` —
            // no leading count (lane_count is derived metadata, not a field).
            out.push_str(&format!("{}LANENAME", inner));
            for name in &ln.lane_names {
                out.push_str(&format!(" \"{}\"", rpp_escape(name)));
            }
            out.push('\n');
        }
        if let Some(n) = self.item_lanes {
            out.push_str(&format!("{}ITEMLANES {}\n", inner, n));
        }
        for a in &self.comp_areas {
            out.push_str(&format!(
                "{}LINKEDLANE {} {} {} {} {} {} {}\n",
                inner,
                a.start,
                a.end,
                a.source_lane,
                a.comp_lane,
                a.unknown_field_5,
                a.fade_in,
                a.fade_out
            ));
        }
        if let Some(rec) = &self.record {
            out.push_str(&format!(
                "{}REC {} {} {} {} {} {} {}\n",
                inner,
                b(rec.armed),
                rec.input,
                monitor_mode_to_i32(&rec.monitor),
                record_mode_to_i32(&rec.record_mode),
                b(rec.monitor_track_media),
                b(rec.preserve_pdc_delayed),
                rec.record_path
            ));
        }
        if let Some(th) = &self.track_height {
            out.push_str(&format!(
                "{}TRACKHEIGHT {} {}\n",
                inner,
                th.height,
                b(th.folder_override)
            ));
        }
        if let Some(iq) = &self.input_quantize {
            out.push_str(&format!(
                "{}INQ {} {} {} {} {} {} {} {}\n",
                inner,
                b(iq.quantize_midi),
                iq.quantize_to_pos,
                b(iq.quantize_note_offs),
                iq.quantize_to,
                iq.quantize_strength,
                iq.swing_strength,
                iq.quantize_range_min,
                iq.quantize_range_max
            ));
        }
        out.push_str(&format!("{}NCHAN {}\n", inner, self.channel_count));
        // The master-send-channel-count line. REAPER inherits the value from
        // NCHAN by default; the official PT Reaper Converter always emits
        // it explicitly. Including it keeps line-by-line diffs stable.
        out.push_str(&format!("{}MAINSEND_NCH {}\n", inner, self.channel_count));
        if let Some(cfg) = &self.rec_cfg {
            out.push_str(&format!("{}RECCFG {}\n", inner, cfg));
        }
        if let Some(mcm) = &self.midi_color_map_fn {
            out.push_str(&format!(
                "{}MIDICOLORMAPFN \"{}\"\n",
                inner,
                rpp_escape(mcm)
            ));
        }
        // `FX 1` (FX-enabled) is the REAPER default — only emit when
        // disabled. The official PT Reaper Converter also omits it for
        // FX-enabled tracks.
        if !self.fx_enabled {
            out.push_str(&format!("{}FX {}\n", inner, b(self.fx_enabled)));
        }
        if let Some(perf) = self.perf {
            out.push_str(&format!("{}PERF {}\n", inner, perf));
        }
        if let Some((tcp, mcp)) = &self.layouts {
            out.push_str(&format!(
                "{}LAYOUTS \"{}\" \"{}\"\n",
                inner,
                rpp_escape(tcp),
                rpp_escape(mcp)
            ));
        }

        // Master send
        if let Some(ms) = &self.master_send {
            out.push_str(&format!(
                "{}MAINSEND {} {}\n",
                inner,
                b(ms.enabled),
                ms.unknown_field_2
            ));
        }

        // Receives
        for recv in &self.receives {
            out.push_str(&format!(
                "{}AUXRECV {} {} {} {} {} {} {} {} {} {} {} {}\n",
                inner,
                recv.source_track_index,
                recv.mode,
                recv.volume,
                recv.pan,
                b(recv.mute),
                b(recv.mono_sum),
                b(recv.invert_polarity),
                recv.source_audio_channels,
                recv.dest_audio_channels,
                recv.pan_law,
                recv.midi_channels,
                recv.automation_mode
            ));
        }

        // Hardware outputs
        for hw in &self.hardware_outputs {
            out.push_str(&format!(
                "{}HWOUT {} {} {} {} {} {} {} {} {}\n",
                inner,
                hw.output_index,
                hw.send_mode,
                hw.volume,
                hw.pan,
                b(hw.mute),
                b(hw.invert_polarity),
                hw.send_source_channel,
                hw.unknown_field_8,
                hw.automation_mode
            ));
        }

        // MIDI output
        if let Some(mo) = &self.midi_output {
            out.push_str(&format!("{}MIDIOUT {} {}\n", inner, mo.device, mo.channel));
        }

        // MIDI note names
        for nn in &self.midi_note_names {
            out.push_str(&format!(
                "{}MIDINOTENAMES {} {} \"{}\" {} {}\n",
                inner,
                nn.channel,
                nn.note_number,
                rpp_escape(&nn.note_name),
                nn.unknown_field_4,
                nn.note_number_2
            ));
        }

        // Extension data
        for (key, value) in &self.extension_data {
            out.push_str(&format!("{}EXT {} {}\n", inner, key, value));
        }

        // Input FX chain
        if let Some(ifx) = &self.input_fx {
            // Write as FXCHAIN_REC instead of FXCHAIN
            out.push_str(&format!("{}<FXCHAIN_REC\n", inner));
            let ifx_inner = format!("{}  ", inner);
            if let Some(rect) = &ifx.window_rect {
                out.push_str(&format!(
                    "{}WNDRECT {} {} {} {}\n",
                    ifx_inner, rect[0], rect[1], rect[2], rect[3]
                ));
            }
            out.push_str(&format!("{}SHOW {}\n", ifx_inner, ifx.show));
            out.push_str(&format!("{}LASTSEL {}\n", ifx_inner, ifx.last_sel));
            out.push_str(&format!("{}DOCKED {}\n", ifx_inner, b(ifx.docked)));
            for node in &ifx.nodes {
                node.write_rpp(out, &ifx_inner);
            }
            out.push_str(&format!("{}>\n", inner));
        }

        // FX chain
        if let Some(fx) = &self.fx_chain {
            fx.write_rpp(out, &inner);
        }

        // Envelopes
        for env in &self.envelopes {
            env.write_rpp(out, &inner);
        }

        // Items
        for item in &self.items {
            item.write_rpp(out, &inner);
        }

        out.push_str(&format!("{}>\n", indent));
    }
}

// ---------------------------------------------------------------------------
// MarkerRegion
// ---------------------------------------------------------------------------

impl MarkerRegion {
    /// Write this marker/region as RPP MARKER line(s).
    ///
    /// Regions emit two MARKER lines: the start (with name) and the end (with "").
    pub fn write_marker_line(&self, out: &mut String, indent: &str) {
        // MARKER id position name flags color locked B guid additional [lane]
        //
        // An empty GUID is written `""`, not left out: the fields after it
        // are positional, and `B  0 2` reads back with the `0` as the GUID
        // and the lane as `additional` — the lane silently lost.
        let guid = if self.guid.is_empty() {
            "\"\"".to_string()
        } else {
            self.guid.clone()
        };
        out.push_str(&format!(
            "{}MARKER {} {} \"{}\" {} {} {} B {} {}",
            indent,
            self.id,
            self.position,
            rpp_escape(&self.name),
            self.flags,
            self.color,
            self.locked,
            guid,
            self.additional,
        ));
        if let Some(lane) = self.lane {
            out.push_str(&format!(" {}", lane));
        }
        out.push('\n');

        // Regions emit a second MARKER line for the end position
        if let Some(end) = self.end_position {
            out.push_str(&format!(
                "{}MARKER {} {} \"\" {}\n",
                indent, self.id, end, self.flags
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// TempoTimeEnvelope
// ---------------------------------------------------------------------------

impl RppSerialize for TempoTimeEnvelope {
    fn write_rpp(&self, out: &mut String, indent: &str) {
        if self.points.is_empty() {
            return;
        }

        let inner = format!("{}  ", indent);

        out.push_str(&format!("{}<TEMPOENVEX\n", indent));
        out.push_str(&format!("{}ACT 1 -1\n", inner));
        out.push_str(&format!("{}VIS 1 0 1\n", inner));
        out.push_str(&format!("{}LANEHEIGHT 0 0\n", inner));
        out.push_str(&format!("{}ARM 0\n", inner));
        out.push_str(&format!("{}DEFSHAPE 0 -1 -1\n", inner));

        for pt in &self.points {
            out.push_str(&format!(
                "{}PT {:.12} {:.10} {}",
                inner, pt.position, pt.tempo, pt.shape
            ));
            if let Some(ts) = pt.time_signature_encoded {
                out.push_str(&format!(" {}", ts));
                // When time signature is present, emit the full PT line
                out.push_str(&format!(
                    " {} {} {} \"{}\" {} {} {}",
                    b(pt.selected),
                    pt.unknown1,
                    pt.bezier_tension,
                    rpp_escape(&pt.metronome_pattern),
                    pt.unknown2,
                    pt.unknown3,
                    pt.unknown4,
                ));
            }
            out.push('\n');
        }

        out.push_str(&format!("{}>\n", indent));
    }
}

// ---------------------------------------------------------------------------
// ReaperProject
// ---------------------------------------------------------------------------

impl RppSerialize for ReaperProject {
    fn write_rpp(&self, out: &mut String, indent: &str) {
        let inner = format!("{}  ", indent);

        out.push_str(&format!(
            "{}<REAPER_PROJECT {} \"{}\" {}\n",
            indent,
            self.version,
            rpp_escape(&self.version_string),
            self.timestamp
        ));

        // Minimal project properties
        if let Some((r1, r2)) = self.properties.ripple {
            out.push_str(&format!("{}RIPPLE {} {}\n", inner, r1, r2));
        } else {
            out.push_str(&format!("{}RIPPLE 0\n", inner));
        }

        if let Some(sr) = self.properties.sample_rate {
            out.push_str(&format!("{}SAMPLERATE {} {} {}\n", inner, sr.0, sr.1, sr.2));
        }
        if let Some(tempo) = self.properties.tempo {
            out.push_str(&format!(
                "{}TEMPO {} {} {} {}\n",
                inner, tempo.0, tempo.1, tempo.2, tempo.3
            ));
        }

        // Tempo envelope
        if let Some(ref tempo_env) = self.tempo_envelope {
            tempo_env.write_rpp(out, &inner);
        }

        // Ruler lanes (v7.62+) — parsed on read (see the RULERLANE/RULERHEIGHT
        // arms in `decode_property`) but previously dropped on write, so a
        // lane's display name/color never survived a round trip even though
        // the markers/regions placed on it (via `MarkerRegion::lane`) did.
        for lane in &self.ruler_lanes {
            out.push_str(&format!(
                "{}RULERLANE {} {} \"{}\" {} {} 0\n",
                inner,
                lane.index,
                lane.flags,
                rpp_escape(&lane.name),
                lane.color,
                lane.extra
            ));
        }
        if let Some((total, secondary)) = self.ruler_height {
            out.push_str(&format!("{}RULERHEIGHT {} {}\n", inner, total, secondary));
        }

        // Markers and regions
        for marker in &self.markers_regions.all {
            marker.write_marker_line(out, &inner);
        }

        // Tracks
        for track in &self.tracks {
            track.write_rpp(out, &inner);
        }

        out.push_str(&format!("{}>\n", indent));
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{ReaperProjectBuilder, TrackBuilder};
    use crate::types::track::{MuteSoloSettings, TrackSoloState};

    #[test]
    fn test_track_basic_roundtrip() {
        let mut track = TrackBuilder::new("Guitar").volume(0.8).pan(-0.25).build();
        // Builder doesn't populate mutesolo unless muted/soloed; set it
        // explicitly so the MUTESOLO line is emitted during serialization.
        track.mutesolo = Some(MuteSoloSettings {
            mute: false,
            solo: TrackSoloState::NoSolo,
            solo_defeat: false,
        });

        let rpp = track.to_rpp_string();
        assert!(rpp.starts_with("<TRACK\n"));
        // Simple identifiers like "Guitar" emit bare (no surrounding quotes)
        // to match the official PT Reaper Converter's token style. Names
        // with spaces or special chars are still quoted — see
        // `test_track_name_with_spaces` for that path.
        assert!(rpp.contains("NAME Guitar\n"));
        assert!(rpp.contains("VOLPAN 0.8 -0.25 -1"));
        assert!(rpp.contains("MUTESOLO 0 0 0"));
        assert!(rpp.contains("NCHAN 2"));
        // `FX 1` (FX-enabled) is the REAPER default; the serializer
        // omits it. We verify that FX-DISABLED tracks DO emit `FX 0`
        // in `test_track_fx_disabled` below.
        assert!(!rpp.contains("FX 1"));
        assert!(!rpp.contains("FX 0"));
        assert!(rpp.ends_with(">\n"));
    }

    #[test]
    fn test_track_with_folder() {
        let track = TrackBuilder::new("Drums").folder_start().build();

        let rpp = track.to_rpp_string();
        assert!(rpp.contains("ISBUS 1 1"));
    }

    #[test]
    fn test_track_with_fx_chain() {
        use crate::types::fx_chain::{FxChain, FxChainNode, FxPlugin, PluginType};

        // Use builder for the track shell, manual FxChain for plugin-specific fields
        // (fxid, state_data) that the .vst() shorthand doesn't expose.
        let mut track = TrackBuilder::new("Lead").build();
        track.fx_chain = Some(FxChain {
            window_rect: None,
            show: 0,
            last_sel: 0,
            docked: false,
            nodes: vec![FxChainNode::Plugin(FxPlugin {
                name: "VST: ReaEQ (Cockos)".to_string(),
                custom_name: None,
                plugin_type: PluginType::Vst,
                file: "reaeq.dll".to_string(),
                bypassed: false,
                offline: false,
                fxid: Some("{EQ-GUID}".to_string()),
                preset_name: None,
                float_pos: None,
                wak: None,
                parallel: false,
                state_data: vec!["ZXE=".to_string()],
                raw_block: String::new(),
                param_envelopes: vec![],
                params_on_tcp: vec![],
                header_extra: None,
            })],
            raw_content: String::new(),
        });

        let rpp = track.to_rpp_string();
        assert!(rpp.contains("<FXCHAIN"));
        assert!(rpp.contains("ReaEQ"));
        assert!(rpp.contains("FXID {EQ-GUID}"));
    }

    #[test]
    fn test_item_with_wave_source() {
        let track = TrackBuilder::new("_")
            .item(1.0, 4.0, |i| {
                i.name("Kick Pattern").source_wave("audio/kick.wav")
            })
            .build();
        let item = &track.items[0];

        let rpp = item.to_rpp_string();
        assert!(rpp.starts_with("<ITEM\n"));
        assert!(rpp.contains("POSITION 1"));
        assert!(rpp.contains("LENGTH 4"));
        assert!(rpp.contains("NAME \"Kick Pattern\""));
        // No `TAKE SEL` marker is emitted for a single-take item — REAPER
        // reads take #0's properties directly off the ITEM block.
        assert!(rpp.contains("<SOURCE WAVE"));
        assert!(rpp.contains("FILE \"audio/kick.wav\""));
    }

    #[test]
    fn test_track_parse_serialize_consistency() {
        // Build a track, serialize, parse back, verify key fields
        let track = TrackBuilder::new("Bass")
            .guid("{BASS-GUID}")
            .selected()
            .volume(0.7)
            .muted()
            .build();

        let rpp = track.to_rpp_string();
        let parsed = Track::from_rpp_block(&rpp).expect("should parse serialized track");
        assert_eq!(parsed.name, "Bass");
        assert_eq!(parsed.track_id.as_deref(), Some("{BASS-GUID}"));
        assert!(parsed.selected);
        assert!(parsed.mutesolo.as_ref().unwrap().mute);
    }

    #[test]
    fn test_project_serialize() {
        let project = ReaperProjectBuilder::new()
            .timestamp(12345)
            .empty_track("Track 1")
            .empty_track("Track 2")
            .build();

        let rpp = project.to_rpp_string();
        assert!(rpp.starts_with("<REAPER_PROJECT 0.1"));
        assert!(rpp.contains("RIPPLE 0"));
        assert!(rpp.contains("<TRACK"));
        assert!(rpp.contains("NAME \"Track 1\""));
        assert!(rpp.contains("NAME \"Track 2\""));
    }

    #[test]
    fn test_indentation_nesting() {
        // Verify that nested structures get progressively deeper indentation
        let track = TrackBuilder::new("Test")
            .item(0.0, 1.0, |i| i.source_wave("test.wav"))
            .build();

        let rpp = track.to_rpp_string();
        // Track at indent 0, item at indent 1, source at indent 2.
        // No `TAKE SEL\n` line is emitted for take #0 — REAPER reads
        // the first take's properties directly off the ITEM block.
        assert!(rpp.contains("<TRACK\n"));
        assert!(rpp.contains("  <ITEM\n"));
        assert!(rpp.contains("    <SOURCE WAVE\n"));
        assert!(rpp.contains("      FILE \"test.wav\"\n"));
    }
}

#[cfg(test)]
mod group_flags_tests {
    use super::RppSerialize;
    use crate::builder::TrackBuilder;
    use crate::io::parse_project_text;

    /// `GROUP_FLAGS` / `GROUP_FLAGS_HIGH` are written back exactly as
    /// parsed, so a project that goes through the writer keeps its
    /// groups. Values are the REAPER-saved fixture's (a lead of
    /// everything in group 1; VCA follow of group 3 in the high line).
    #[test]
    fn group_flags_round_trip_through_the_writer() {
        let mut track = TrackBuilder::new("Lead").build();
        track.group_flags = Some(vec![
            1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1,
        ]);
        track.group_flags_high = Some(vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4,
        ]);
        let rpp = track.to_rpp_string();
        assert!(
            rpp.contains("\n  GROUP_FLAGS 1 0 1 0 1 0 1 0 1 0 1 0 1 0 0 0 0 0 1\n"),
            "{rpp}"
        );
        assert!(
            rpp.contains("\n  GROUP_FLAGS_HIGH 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 4\n"),
            "{rpp}"
        );

        let project = format!("<REAPER_PROJECT 0.1 \"7.75/linux-x86_64\" 1700000000\n{rpp}>\n");
        let parsed = parse_project_text(&project).unwrap();
        let back = &parsed.tracks[0];
        assert_eq!(back.group_flags, track.group_flags);
        assert_eq!(back.group_flags_high, track.group_flags_high);
    }

    /// A track in no group writes no line — `None` and an empty Vec
    /// alike — so an ungrouped project stays byte-identical.
    #[test]
    fn ungrouped_track_writes_no_group_line() {
        let mut track = TrackBuilder::new("Plain").build();
        assert!(!track.to_rpp_string().contains("GROUP_FLAGS"));
        track.group_flags = Some(Vec::new());
        assert!(!track.to_rpp_string().contains("GROUP_FLAGS"));
    }
}
