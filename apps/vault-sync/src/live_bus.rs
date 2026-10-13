//! The "Master Setlist Template" — a fixed, purpose-built live-FOH bus
//! scheme every song's project conforms to, distinct from
//! `rpp.rs`'s dynamic-template-organized studio scheme (which classifies
//! by general instrument taxonomy — DRUM/BASS/GUITAR/KEYS/ORCH/FX/VOX —
//! for a full multitrack session). This one is deliberately smaller and
//! live-sound-shaped: Click + Cues is its own bus (never summed with
//! anything else), and Leads/Pads are their own buses rather than folded
//! into Keys/Synths, matching how a live engineer actually wants faders
//! grouped for a backing-track rig.
//!
//! Two separate sections, not one: the raw stems are sorted into their
//! own content folders (by instrument), and a single `BUSES` folder
//! holds every live bus flat, each fed by sends (`AUXRECV`) from its
//! content tracks — not physically nested inside the bus. That's the
//! same shape a hand-built live session takes: scroll the content
//! folders to find/tweak a mic, work the bus folder to mix.
//!
//! Built as its own thing here rather than as a variant inside the
//! shared `dynamic-template` crate — other consumers depend on that
//! crate's existing general-purpose bus tree, and this is a
//! session-specific, worship-live-rig taxonomy, not a general one.

use std::collections::HashMap;

use dawfile_reaper::RppSerialize;
use dawfile_reaper::builder::{ReaperProjectBuilder, TrackBuilder};
use dawfile_reaper::types::track::MasterSendSettings;
use dynamic_template::colors::{groups, guitars, orchestra, synths, vocals};

use crate::library::{LibrarySong, Stem};
use crate::rpp::{
    FALLBACK_LENGTH_SECONDS, source_type_for, tempo_from_chart, wav_duration_seconds, wav_sibling,
};

/// The fixed bus list — also the content-folder categories (Click +
/// Cues through Bgv), just without the "BUS" suffix on that side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LiveBus {
    ClickCues,
    Drums,
    Bass,
    Acoustic,
    Electric,
    Keys,
    Leads,
    Pads,
    OrchStrings,
    OrchWoodwinds,
    OrchBrass,
    OrchPercussion,
    Fx,
    Bgv,
}

/// All buses, in the fixed order they appear in `BUSES` and in a song's
/// content folders.
const ALL_BUSES: [LiveBus; 14] = [
    LiveBus::ClickCues,
    LiveBus::Drums,
    LiveBus::Bass,
    LiveBus::Acoustic,
    LiveBus::Electric,
    LiveBus::Keys,
    LiveBus::Leads,
    LiveBus::Pads,
    LiveBus::OrchStrings,
    LiveBus::OrchWoodwinds,
    LiveBus::OrchBrass,
    LiveBus::OrchPercussion,
    LiveBus::Fx,
    LiveBus::Bgv,
];

impl LiveBus {
    fn bus_track_name(self) -> &'static str {
        match self {
            LiveBus::ClickCues => "CLICK + CUES BUS",
            LiveBus::Drums => "DRUMS BUS",
            LiveBus::Bass => "BASS BUS",
            LiveBus::Acoustic => "ACOUSTIC BUS",
            LiveBus::Electric => "ELECTRIC BUS",
            LiveBus::Keys => "KEYS BUS",
            LiveBus::Leads => "LEADS BUS",
            LiveBus::Pads => "PADS BUS",
            LiveBus::OrchStrings => "STRINGS BUS",
            LiveBus::OrchWoodwinds => "WOODWINDS BUS",
            LiveBus::OrchBrass => "BRASS BUS",
            LiveBus::OrchPercussion => "ORCH PERCUSSION BUS",
            LiveBus::Fx => "FX BUS",
            LiveBus::Bgv => "BGV BUS",
        }
    }

    /// Where this bus's content lives in the content-folder tree —
    /// e.g. Acoustic and Electric nest under one "GUITARS" folder,
    /// everything else is its own top-level folder.
    fn content_path(self) -> &'static [&'static str] {
        match self {
            LiveBus::ClickCues => &["CLICK + CUES"],
            LiveBus::Drums => &["DRUMS"],
            LiveBus::Bass => &["BASS"],
            LiveBus::Acoustic => &["GUITARS", "ACOUSTIC"],
            LiveBus::Electric => &["GUITARS", "ELECTRIC"],
            LiveBus::Keys => &["KEYS"],
            // Matches dynamic-template's real classification path
            // (`["Synths", "Lead"]` / `["Synths", "Pad"]` — confirmed via
            // `cargo run -p dynamic-template -- -v "... Synth Lead"`):
            // Leads and Pads nest under one SYNTHS folder, not two
            // separate top-level ones.
            LiveBus::Leads => &["SYNTHS", "LEADS"],
            LiveBus::Pads => &["SYNTHS", "PADS"],
            // Matches dynamic-template's real classification path
            // (`["Orchestra", "Strings"]` etc. — confirmed via
            // `cargo run -p dynamic-template -- -v "... Strings"`):
            // one ORCH branch, one sub-folder/bus per orchestral family.
            LiveBus::OrchStrings => &["ORCH", "STRINGS"],
            LiveBus::OrchWoodwinds => &["ORCH", "WOODWINDS"],
            LiveBus::OrchBrass => &["ORCH", "BRASS"],
            LiveBus::OrchPercussion => &["ORCH", "PERCUSSION"],
            LiveBus::Fx => &["FX"],
            LiveBus::Bgv => &["VOX", "BGV"],
        }
    }

    /// A consistent color per bus, applied to the bus fader, its
    /// content folder, and every stem track under it — "auto color" in
    /// the sense that nothing needs hand-coloring afterward, everything
    /// in a category matches on sight. Pulled from the same established
    /// palette `dynamic-template`'s own `apply_colors` uses
    /// (`music_catalog::instruments`), not an invented one — so a Drums
    /// track here is the same red as a Drums track in the studio scheme.
    ///
    /// Encoded by hand via [`reaper_color`] rather than
    /// `dynamic_template::colors::to_reaper_color` — that one delegates
    /// to `color_palette::Color::to_reaper_native`, which is
    /// `#[cfg(target_os = "windows")]`-conditional (BGR on Windows, raw
    /// RGB elsewhere). REAPER's `.rpp` is a portable text format read
    /// identically on every OS — there is no such thing as a
    /// build-platform-dependent on-disk color encoding — so that split
    /// is a bug, not a real cross-platform difference; building on Linux
    /// hits the "raw RGB" branch, which is wrong. `dawfile-reaper`'s own
    /// `marker_with_color` documents the actual, single, correct format:
    /// `0x01000000 | (b<<16)|(g<<8)|r` (BGR) — used here instead,
    /// unconditionally.
    fn color(self) -> u32 {
        let c = match self {
            LiveBus::ClickCues => groups::GUIDE,
            LiveBus::Drums => groups::DRUMS,
            LiveBus::Bass => groups::BASS,
            LiveBus::Acoustic => guitars::ACOUSTIC,
            LiveBus::Electric => guitars::ELECTRIC,
            LiveBus::Keys => groups::KEYS,
            LiveBus::Leads => synths::LEAD,
            LiveBus::Pads => synths::PAD,
            LiveBus::OrchStrings => orchestra::STRINGS,
            LiveBus::OrchWoodwinds => orchestra::WOODWINDS,
            LiveBus::OrchBrass => orchestra::BRASS,
            LiveBus::OrchPercussion => orchestra::PERCUSSION,
            LiveBus::Fx => groups::SFX,
            LiveBus::Bgv => vocals::BACKGROUND,
        };
        reaper_color(c.r(), c.g(), c.b())
    }
}

/// REAPER's actual native color int, per `dawfile-reaper`'s own
/// `marker_with_color` documentation: `0x01000000 | (b<<16)|(g<<8)|r` —
/// BGR, not RGB, and not platform-dependent.
fn reaper_color(r: u8, g: u8, b: u8) -> u32 {
    0x0100_0000 | ((b as u32) << 16) | ((g as u32) << 8) | (r as u32)
}

/// Classify a stem by its display label (e.g. "EG 1", "Synth Pad",
/// "BG Harm A 1"). Tries dynamic-template's real classifier first
/// ([`real_classify`] — the same parser/taxonomy `organize_into_tracks`
/// and the studio scheme use, verified against the real stem vocabulary
/// in `features/dynamic-template/tests/multitrack_examples/{holy_forever,
/// thank_god_im_free, washed, who_else, god_im_just_grateful,
/// elevation_worship_praise}.rs` — every song in this library already has
/// a passing test there), falling back to a small keyword table only for
/// what it leaves unclassified (documented "KNOWN GAP"s in those tests:
/// bare "Arps", bare "Synths").
///
/// Deliberately NOT using `organize_into_tracks`'s *rendered tree* for
/// grouping, only its per-item classification path
/// (`ItemMetadata::group`): that tree collapses a folder down to nothing
/// when it would hold only one child (e.g. a lone "Synth Pad" renders as
/// a bare "Pad" track with no enclosing folder at all — confirmed via
/// `cargo run -p dynamic-template -- -v "... Synth Pad"`). This scheme
/// wants Pads to always be its own folder/bus, one stem or ten, so
/// [`build_content_tree`] builds the folder tree itself from the
/// classification path instead of trusting the collapsed tree shape.
pub fn classify(label: &str) -> LiveBus {
    real_classify(label).unwrap_or_else(|| fallback_classify(label))
}

/// Run dynamic-template's actual parser/taxonomy on one stem label and
/// map its classification path to a [`LiveBus`]. Building a fresh
/// `default_config()` per call is wasteful but harmless at this scale
/// (a few dozen stems per song, once per `build-project` invocation).
fn real_classify(label: &str) -> Option<LiveBus> {
    let config = dynamic_template::default_config();
    let item = monarchy::Parser::new(&config)
        .parse(label.to_string())
        .ok()?;
    let path = item.metadata.group?;
    let top = path.first()?.as_str();
    let second = path.get(1).map(String::as_str);
    match top {
        "Guide" => Some(LiveBus::ClickCues),
        "Tracks" | "SFX" => Some(LiveBus::Fx),
        "Bass" => Some(LiveBus::Bass),
        "Guitars" => Some(match second {
            Some("Acoustic") => LiveBus::Acoustic,
            _ => LiveBus::Electric,
        }),
        "Keys" => Some(LiveBus::Keys),
        "Percussion" | "Drums" => Some(LiveBus::Drums),
        "Vocals" | "Choir" => Some(LiveBus::Bgv),
        // Real Orchestra sub-paths map straight across. "Horns" (a
        // separate top-level group in dynamic-template's own taxonomy —
        // Saxophone/Trumpet/Trombone as a contemporary horn section, not
        // full orchestral sub-groups) folds into Brass; "Fiddle" (a
        // string instrument) into Strings; "Harmonica" has no clean
        // orchestral family, parked under Woodwinds as the closest fit.
        "Orchestra" => Some(match second {
            Some("Strings") | Some("Harp") => LiveBus::OrchStrings,
            Some("Woodwinds") => LiveBus::OrchWoodwinds,
            Some("Brass") => LiveBus::OrchBrass,
            Some("Percussion") => LiveBus::OrchPercussion,
            _ => LiveBus::OrchStrings,
        }),
        "Horns" => Some(LiveBus::OrchBrass),
        "Fiddle" => Some(LiveBus::OrchStrings),
        "Harmonica" => Some(LiveBus::OrchWoodwinds),
        "Synths" => Some(match second {
            Some("Lead") => LiveBus::Leads,
            Some("Pad") => LiveBus::Pads,
            _ => LiveBus::Fx,
        }),
        _ => None,
    }
}

/// Covers what [`real_classify`] leaves unclassified — bare "Arps", bare
/// "Synths" (both documented "KNOWN GAP"s in the dynamic-template tests
/// this scheme reuses) — plus anything else that shows up later. All of
/// today's known gaps are supporting texture/production elements, so
/// this is deliberately a single fallback rather than a growing table;
/// add a case here only once something needs to land somewhere else.
fn fallback_classify(_label: &str) -> LiveBus {
    LiveBus::Fx
}

/// One node in the content-folder tree: a folder name, its color, the
/// stems (tagged with their bus) that land directly in it, and any
/// sub-folders (e.g. GUITARS holds no stems of its own — only ACOUSTIC
/// and ELECTRIC do).
struct ContentNode<'a> {
    path_segment: &'static str,
    color: u32,
    stems: Vec<(LiveBus, &'a Stem)>,
    children: Vec<ContentNode<'a>>,
}

impl<'a> ContentNode<'a> {
    fn is_empty(&self) -> bool {
        self.stems.is_empty() && self.children.iter().all(ContentNode::is_empty)
    }
}

/// Color for a branch folder that spans more than one bus (GUITARS
/// holds Acoustic + Electric, VOX holds Bgv) — the broader family color
/// from the same established palette, distinct from its children's own
/// bus colors.
fn branch_color(segment: &str) -> u32 {
    let c = match segment {
        "GUITARS" => groups::GUITARS,
        "VOX" => groups::VOCALS,
        "SYNTHS" => groups::SYNTHS,
        "ORCH" => groups::ORCHESTRA,
        _ => groups::REFERENCE,
    };
    reaper_color(c.r(), c.g(), c.b())
}

/// Build the content-folder tree from a song's stems, classified via
/// [`classify`] and bucketed by [`LiveBus::content_path`].
fn build_content_tree(song: &LibrarySong) -> ContentNode<'_> {
    let mut by_bus: HashMap<LiveBus, Vec<&Stem>> = HashMap::new();
    for stem in &song.stems {
        by_bus.entry(classify(&stem.label)).or_default().push(stem);
    }

    // Group buses by their content path's first segment, preserving
    // `ALL_BUSES` order within each group.
    let mut top_level: Vec<(&'static str, Vec<LiveBus>)> = Vec::new();
    for bus in ALL_BUSES {
        let first = bus.content_path()[0];
        match top_level.iter_mut().find(|(seg, _)| *seg == first) {
            Some((_, buses)) => buses.push(bus),
            None => top_level.push((first, vec![bus])),
        }
    }

    let children = top_level
        .into_iter()
        .map(|(segment, buses)| {
            if buses.len() == 1 && buses[0].content_path().len() == 1 {
                // A plain top-level category with no sub-folder (e.g. DRUMS).
                let bus = buses[0];
                ContentNode {
                    path_segment: segment,
                    color: bus.color(),
                    stems: by_bus
                        .get(&bus)
                        .into_iter()
                        .flatten()
                        .map(|s| (bus, *s))
                        .collect(),
                    children: Vec::new(),
                }
            } else {
                // A branch (GUITARS, VOX) whose buses nest one level deeper.
                let sub_children = buses
                    .into_iter()
                    .map(|bus| ContentNode {
                        path_segment: bus.content_path()[1],
                        color: bus.color(),
                        stems: by_bus
                            .get(&bus)
                            .into_iter()
                            .flatten()
                            .map(|s| (bus, *s))
                            .collect(),
                        children: Vec::new(),
                    })
                    .collect();
                ContentNode {
                    path_segment: segment,
                    color: branch_color(segment),
                    stems: Vec::new(),
                    children: sub_children,
                }
            }
        })
        .collect();

    ContentNode {
        path_segment: "",
        color: 0,
        stems: Vec::new(),
        children,
    }
}

/// Emit the content tree onto `builder`, recording each stem track's
/// final index (needed for the `BUSES` folder's sends afterward) into
/// `by_bus`. `closing_after` works exactly as in the bus-nesting code
/// this replaced: how many enclosing folders also close once this
/// subtree's last track finishes, one more added per level here.
fn emit_content(
    node: &ContentNode<'_>,
    song_title: &str,
    closing_after: i32,
    next_index: &mut usize,
    by_bus: &mut HashMap<LiveBus, Vec<i32>>,
    mut builder: ReaperProjectBuilder,
) -> ReaperProjectBuilder {
    if node.is_empty() {
        return builder;
    }

    // The root node is a bare container (no folder of its own); only
    // its children become real folder tracks.
    let is_root = node.path_segment.is_empty();
    if !is_root {
        builder = builder.track(node.path_segment, |t| t.folder_start().color(node.color));
        *next_index += 1;
    }

    let non_empty_children: Vec<&ContentNode<'_>> =
        node.children.iter().filter(|c| !c.is_empty()).collect();
    let total_units = node.stems.len() + non_empty_children.len();
    let mut unit_index = 0;

    for (bus, stem) in &node.stems {
        unit_index += 1;
        let is_last = unit_index == total_units;
        let close_levels = if is_root {
            0
        } else if is_last {
            closing_after + 1
        } else {
            0
        };
        let index = *next_index;
        *next_index += 1;
        by_bus.entry(*bus).or_default().push(index as i32);
        builder = emit_stem_track(builder, song_title, stem, *bus, close_levels);
    }
    for child in non_empty_children {
        unit_index += 1;
        let is_last = unit_index == total_units;
        let passed_close = if is_root {
            0
        } else if is_last {
            closing_after + 1
        } else {
            0
        };
        builder = emit_content(child, song_title, passed_close, next_index, by_bus, builder);
    }

    builder
}

fn emit_stem_track(
    builder: ReaperProjectBuilder,
    song_title: &str,
    stem: &Stem,
    bus: LiveBus,
    close_levels: i32,
) -> ReaperProjectBuilder {
    let _ = song_title; // kept for signature symmetry with rpp.rs; label alone names the track now.
    let length = wav_duration_seconds(&wav_sibling(&stem.path)).unwrap_or(FALLBACK_LENGTH_SECONDS);
    let source_type = source_type_for(&stem.path);
    let file_path = stem.path.to_string_lossy().into_owned();
    let label = stem.label.clone();
    let color = bus.color();

    let mut track = TrackBuilder::new(stem.label.clone())
        .color(color)
        .item(0.0, length, move |i| {
            i.take(file_path, source_type).take_name(label).looped()
        })
        .build();
    // Content tracks route to their bus via a send, not straight to
    // master — the bus is the only thing that should reach the mix.
    track.master_send = Some(MasterSendSettings {
        enabled: false,
        unknown_field_2: 0,
    });
    if close_levels > 0 {
        track.folder = Some(dawfile_reaper::types::track::FolderSettings {
            folder_state: dawfile_reaper::types::track::FolderState::LastInFolder,
            indentation: -close_levels,
        });
    }
    builder.add_track(track)
}

/// Build the `BUSES` folder: every bus with at least one content track,
/// flat (no further nesting), each fed by sends from its content
/// tracks' indices.
fn emit_buses(
    mut builder: ReaperProjectBuilder,
    by_bus: &HashMap<LiveBus, Vec<i32>>,
) -> ReaperProjectBuilder {
    let present: Vec<LiveBus> = ALL_BUSES
        .into_iter()
        .filter(|b| by_bus.contains_key(b))
        .collect();
    if present.is_empty() {
        return builder;
    }

    builder = builder.track("BUSES", |t| t.folder_start().color(branch_color("BUSES")));
    let last = present.len() - 1;
    for (i, bus) in present.iter().enumerate() {
        let sources = &by_bus[bus];
        let mut track = TrackBuilder::new(bus.bus_track_name()).color(bus.color());
        for &source_index in sources {
            track = track.receive(source_index);
        }
        let mut track = track.build();
        if i == last {
            track.folder = Some(dawfile_reaper::types::track::FolderSettings {
                folder_state: dawfile_reaper::types::track::FolderState::LastInFolder,
                indentation: -1,
            });
        }
        builder = builder.add_track(track);
    }
    builder
}

/// Build the Master Setlist Template project for one song: content
/// sorted into its own instrument folders, then one `BUSES` folder
/// holding every live bus flat, fed by sends from the content tracks
/// that classify into it.
pub fn build_live_rpp(song: &LibrarySong) -> String {
    let (bpm, num, den) = song
        .chart_kf
        .as_deref()
        .map(tempo_from_chart)
        .unwrap_or((120.0, 4, 4));
    let mut builder = ReaperProjectBuilder::new().tempo_with_time_sig(bpm, num, den);

    let content = build_content_tree(song);
    let mut next_index = 0usize;
    let mut by_bus: HashMap<LiveBus, Vec<i32>> = HashMap::new();
    builder = emit_content(
        &content,
        &song.title,
        0,
        &mut next_index,
        &mut by_bus,
        builder,
    );
    builder = emit_buses(builder, &by_bus);

    builder.build().to_rpp_string()
}
