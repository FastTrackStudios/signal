//! Mixer View — a full mixing-console UI with instrument submix folders.
//!
//! A dense, professional multi-channel console in the spirit of a digital
//! live-sound desk (X32 / Allen & Heath) or a DAW mixer: a left rail of
//! instrument-group jump buttons, then channel strips grouped into collapsible
//! **submix folders** (Drums / Bass / Guitars / Keys / Vocals / …), the whole
//! thing horizontally scrollable. Each leaf strip carries an instrument icon, a
//! pan control, routing icon buttons, a tall vertical fader whose **cap is the
//! track's own color** (with grip ridges, sitting in a dark groove), a dB
//! scale, a level meter, the fader's dB value, a mute button, and the track
//! name on a colored footer bar. Each folder has its own group mute / solo /
//! fader and a collapse toggle.
//!
//! ## Organization
//! The flat `tracks: Vec<Track>` is auto-organized into instrument folders. On
//! native targets this uses the **dynamic-template** crate (`monarchy` sort →
//! multi-level folder hierarchy). That crate transitively pulls REAPER FFI +
//! tokio-net and does not build for `wasm32`, so on the wasm app we use a
//! built-in taxonomy classifier that yields the same single-level folder tree.
//! Both paths avoid unnecessary folders (a lone item stays a bare strip).
//!
//! ## Submix semantics (no real bus yet)
//! There is no submix-bus audio node behind a folder, so folder controls drive
//! the children directly: group mute/solo blanket-toggle every descendant leaf
//! to a common state, and the group fader **sets** every child fader to the
//! group value (no relative scaling). True submix-bus audio is a follow-up.
//!
//! Purely presentational: reports edits via callbacks; the state of record is
//! always the `Track` props. Meters are prop-driven (the app feeds real peaks
//! from per-stem `AnalyserNodes`) — this component never invents motion.

use crate::prelude::*;
use daw_proto::Track;
use lucide_dioxus::{AlarmClock, Drum, Guitar, Mic, Music2, Piano};
use std::collections::HashMap;

/// dB scale ticks drawn beside each fader (top → bottom). The fader taper
/// tops out at 0 dB (fader position `1.0`), so there is no positive region.
const DB_TICKS: &[&str] = &["0", "-5", "-10", "-20", "-40", "-∞"];

/// Custom fader styling: a chunky, rounded console fader CAP tinted with the
/// track's own color (`--fader`), with horizontal grip ridges and a top
/// highlight, sliding in a visibly inset dark groove. Injected once. `--fader`
/// is a per-strip CSS custom property (the track color) — the only raw color;
/// everything else is theme tokens / neutral shading.
const FADER_CSS: &str = r"
.fts-fader{-webkit-appearance:none;appearance:none;background:transparent;cursor:pointer;}
.fts-fader::-webkit-slider-runnable-track{width:8px;border-radius:5px;
  background:linear-gradient(to right,#000,color-mix(in oklab,var(--muted) 55%,#000),#000);
  box-shadow:inset 0 0 4px rgba(0,0,0,.9);}
.fts-fader::-moz-range-track{width:8px;border-radius:5px;
  background:color-mix(in oklab,var(--muted) 55%,#000);box-shadow:inset 0 0 4px rgba(0,0,0,.9);}
.fts-fader::-webkit-slider-thumb{-webkit-appearance:none;appearance:none;
  width:22px;height:34px;border-radius:5px;border:1px solid rgba(0,0,0,.65);
  /* center the wide cap over the 8px rail: -(22-8)/2 on each inline edge */
  margin-inline:-7px;
  box-shadow:0 2px 4px rgba(0,0,0,.7),inset 0 1px 0 rgba(255,255,255,.45);
  background:
    repeating-linear-gradient(to bottom,rgba(0,0,0,.30) 0 1.5px,transparent 1.5px 6px),
    linear-gradient(to bottom,rgba(255,255,255,.42),rgba(255,255,255,0) 45%),
    var(--fader,var(--primary));}
.fts-fader::-moz-range-thumb{width:22px;height:34px;border-radius:5px;border:1px solid rgba(0,0,0,.65);
  margin-inline:-7px;
  box-shadow:0 2px 4px rgba(0,0,0,.7),inset 0 1px 0 rgba(255,255,255,.45);
  background:
    repeating-linear-gradient(to bottom,rgba(0,0,0,.30) 0 1.5px,transparent 1.5px 6px),
    linear-gradient(to bottom,rgba(255,255,255,.42),rgba(255,255,255,0) 45%),
    var(--fader,var(--primary));}
";

/// Instrument category derived from a track / group name.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Inst {
    /// The reference / original / backing track — its own dedicated home,
    /// always the leftmost folder (see [`organize_builtin`]).
    Reference,
    Drums,
    Bass,
    Guitars,
    Synths,
    Keys,
    Strings,
    Vocals,
    /// Loops, samples, sequences, playback stems — anything triggered live
    /// rather than an instrument performance.
    Tracks,
    Click,
    Other,
}

impl Inst {
    /// Case-insensitive keyword match on a track or group name.
    fn from_name(name: &str) -> Self {
        let n = name.to_lowercase();
        let has = |kws: &[&str]| kws.iter().any(|k| n.contains(k));
        // Reference FIRST so "Original Track" lands in Reference, not Tracks
        // (it contains "track") — and never in an instrument folder.
        if has(&["reference", "original", "backing"]) {
            Self::Reference
        } else if has(&["click", "cue", "guide", "metronome", "count"]) {
            Self::Click
        } else if has(&[
            "drum", "kick", "snare", "tom", "hat", "perc", "cymbal", "ride", "crash",
        ]) {
            Self::Drums
        } else if has(&["bass"]) {
            Self::Bass
        } else if has(&["guitar", "gtr", "electric", "acoustic"]) {
            Self::Guitars
        } else if has(&["synth", "arp", "saw", "pluck"]) {
            Self::Synths
        } else if has(&["key", "piano", "organ", "pad", "rhodes", "wurli", "ep "]) {
            Self::Keys
        } else if has(&["string", "violin", "viola", "cello", "orchestr"]) {
            Self::Strings
        } else if has(&["vox", "vocal", "choir", "bgv", "lead", "singer"]) {
            Self::Vocals
        } else if has(&[
            "loop", "sample", "sequence", "playback", "sfx", "stem", "track",
        ]) {
            Self::Tracks
        } else {
            Self::Other
        }
    }

    /// Group label / folder name.
    const fn label(self) -> &'static str {
        match self {
            Self::Reference => "Reference",
            Self::Drums => "Drums",
            Self::Bass => "Bass",
            Self::Guitars => "Guitars",
            Self::Synths => "Synths",
            Self::Keys => "Keys",
            Self::Strings => "Strings",
            Self::Vocals => "Vocals",
            Self::Tracks => "Tracks",
            Self::Click => "Click",
            Self::Other => "Other",
        }
    }
}

/// A small lucide instrument icon chosen from a track/group name.
fn instrument_icon(name: &str) -> Element {
    let sz = 13usize;
    match Inst::from_name(name) {
        Inst::Drums => rsx! { Drum { size: sz, color: "currentColor" } },
        // Bass shares the guitar-family icon.
        Inst::Bass | Inst::Guitars => rsx! { Guitar { size: sz, color: "currentColor" } },
        Inst::Synths | Inst::Keys => rsx! { Piano { size: sz, color: "currentColor" } },
        Inst::Vocals => rsx! { Mic { size: sz, color: "currentColor" } },
        Inst::Click => rsx! { AlarmClock { size: sz, color: "currentColor" } },
        Inst::Reference | Inst::Tracks | Inst::Strings | Inst::Other => {
            rsx! { Music2 { size: sz, color: "currentColor" } }
        }
    }
}

/// Map a normalized fader position (`0.0` = -∞, `1.0` = 0 dB) to a display dB
/// label. Uses a simple, monotonic log taper: `dB = 40·log10(v)` — so `0.5`
/// reads ≈ -12 dB and `0.1` reads -40 dB — clamped to `-∞` at the bottom.
fn fader_db_label(v: f64) -> String {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.0009 {
        return "-∞".to_string();
    }
    let db = 40.0 * v.log10();
    if db >= -0.05 {
        "0.0".to_string()
    } else {
        format!("{db:.1}")
    }
}

/// A `0.0..=1.0` ratio as a whole percentage (`0..=100`).
fn pct_from_ratio(ratio: f64) -> i32 {
    let clamped = (ratio * 100.0).round().clamp(0.0, 100.0);
    // Range-checked above (0.0..=100.0); std has no non-`as` float-to-int
    // conversion.
    #[allow(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    {
        clamped as i32
    }
}

/// `#RRGGBB` for a track color, or a neutral grey fallback.
fn color_hex(color: Option<u32>) -> String {
    color.map_or_else(
        || "#52525b".to_string(),
        |c| format!("#{:06x}", c & 0xFF_FF_FF),
    )
}

// ── Submix tree ─────────────────────────────────────────────────────────────

/// A node in the organized mixer tree: either a channel or a submix folder.
#[derive(Clone, PartialEq)]
enum MixNode {
    Leaf(Track),
    Folder(FolderNode),
}

/// A submix folder grouping child channels (and possibly nested folders).
#[derive(Clone, PartialEq)]
struct FolderNode {
    name: String,
    children: Vec<MixNode>,
}

impl FolderNode {
    /// Every descendant channel (depth-first), for aggregate group controls.
    fn leaves(&self) -> Vec<Track> {
        fn walk(nodes: &[MixNode], out: &mut Vec<Track>) {
            for n in nodes {
                match n {
                    MixNode::Leaf(t) => out.push(t.clone()),
                    MixNode::Folder(f) => walk(&f.children, out),
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.children, &mut out);
        out
    }
}

/// Organize the flat track list into a submix tree (target-dispatched).
fn organize(tracks: &[Track]) -> Vec<MixNode> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        organize_dynamic(tracks)
    }
    #[cfg(target_arch = "wasm32")]
    {
        organize_builtin(tracks)
    }
}

/// Built-in taxonomy grouping — wasm path and native fallback. Groups leaves by
/// instrument category (first-seen order); a category with a single track stays
/// a bare strip, and a single overall category produces a flat list (no folder).
fn organize_builtin(tracks: &[Track]) -> Vec<MixNode> {
    let leaves: Vec<Track> = tracks.iter().filter(|t| !t.is_folder).cloned().collect();
    if leaves.is_empty() {
        return tracks.iter().cloned().map(MixNode::Leaf).collect();
    }
    let mut order: Vec<Inst> = Vec::new();
    let mut groups: HashMap<Inst, Vec<Track>> = HashMap::new();
    for t in leaves {
        let c = Inst::from_name(&t.name);
        if !order.contains(&c) {
            order.push(c);
        }
        groups.entry(c).or_default().push(t);
    }
    if order.len() <= 1 {
        if let Some(&key) = order.first() {
            return groups
                .remove(&key)
                .unwrap_or_default()
                .into_iter()
                .map(MixNode::Leaf)
                .collect();
        }
        return Vec::new();
    }
    // Reference is the dedicated, always-leftmost home for the reference /
    // original track — pull it to the front regardless of stem order. Stable,
    // so every other category keeps its first-seen order behind it.
    order.sort_by_key(|c| i32::from(*c != Inst::Reference));
    let mut out = Vec::new();
    for c in order {
        let items = groups.remove(&c).unwrap_or_default();
        // Reference and Tracks are always their OWN labeled folder — even a
        // lone track — so they read as deliberate homes. Ordinary instrument
        // categories collapse a single track to a bare strip.
        let always_folder = matches!(c, Inst::Reference | Inst::Tracks);
        if items.len() == 1 && !always_folder {
            out.extend(items.into_iter().map(MixNode::Leaf));
        } else {
            out.push(MixNode::Folder(FolderNode {
                name: c.label().to_string(),
                children: items.into_iter().map(MixNode::Leaf).collect(),
            }));
        }
    }
    out
}

/// Native path: organize via dynamic-template's monarchy sort, then map each
/// organized leaf back to its original `Track` (preserving guid/color/state) by
/// the item name the folder carries. Falls back to the built-in grouping on any
/// error, and appends any tracks the sorter didn't place.
#[cfg(not(target_arch = "wasm32"))]
fn organize_dynamic(tracks: &[Track]) -> Vec<MixNode> {
    use daw_proto::FolderDepthChange;
    use dynamic_template::{default_config, OrganizeIntoTracks};
    use std::collections::{HashSet, VecDeque};

    let leaves: Vec<Track> = tracks.iter().filter(|t| !t.is_folder).cloned().collect();
    if leaves.is_empty() {
        return organize_builtin(tracks);
    }

    // Queue of original tracks keyed by their display name (the key the
    // organizer carries in each leaf's `items`).
    let mut by_name: HashMap<String, VecDeque<Track>> = HashMap::new();
    for t in &leaves {
        by_name
            .entry(t.name.clone())
            .or_default()
            .push_back(t.clone());
    }

    let names: Vec<String> = leaves.iter().map(|t| t.name.clone()).collect();
    let cfg = default_config();
    let Ok(hierarchy) = names.organize_into_tracks(&cfg, None) else {
        return organize_builtin(tracks);
    };

    // Absolute folder depth per node (mirrors TrackHierarchy::print_tree).
    let mut depth = 0i32;
    let mut entries: Vec<(i32, &daw_proto::TrackNode)> = Vec::new();
    for node in &hierarchy.tracks {
        if let FolderDepthChange::ClosesLevels(n) = node.folder_depth_change {
            depth = depth.saturating_add(i32::from(n)).max(0);
        }
        entries.push((depth, node));
        if node.folder_depth_change == FolderDepthChange::FolderStart {
            depth = depth.saturating_add(1);
        }
    }

    let mut consumed: HashSet<String> = HashSet::new();
    let mut cursor = 0usize;
    let mut out = parse_entries(&entries, &mut cursor, 0, &mut by_name, &mut consumed);

    // Anything the sorter didn't place → append as top-level strips, in order.
    for t in &leaves {
        if !consumed.contains(&t.guid) {
            out.push(MixNode::Leaf(t.clone()));
        }
    }
    if out.is_empty() {
        return organize_builtin(tracks);
    }
    out
}

/// Recursively build `MixNode`s for all entries at `level`, descending into
/// folders. Leaves map back to their original `Track` via `by_name`.
#[cfg(not(target_arch = "wasm32"))]
fn parse_entries(
    entries: &[(i32, &daw_proto::TrackNode)],
    i: &mut usize,
    level: i32,
    by_name: &mut HashMap<String, std::collections::VecDeque<Track>>,
    consumed: &mut std::collections::HashSet<String>,
) -> Vec<MixNode> {
    let mut out = Vec::new();
    while let Some(&(entry_level, node)) = entries.get(*i) {
        if entry_level != level {
            break;
        }
        *i = i.saturating_add(1);
        if node.is_folder {
            let children = parse_entries(entries, i, level.saturating_add(1), by_name, consumed);
            if !children.is_empty() {
                out.push(MixNode::Folder(FolderNode {
                    name: node.name.clone(),
                    children,
                }));
            }
        } else {
            let keys = if node.items.is_empty() {
                vec![node.name.clone()]
            } else {
                node.items.clone()
            };
            for k in keys {
                if let Some(q) = by_name.get_mut(&k) {
                    if let Some(tr) = q.pop_front() {
                        consumed.insert(tr.guid.clone());
                        out.push(MixNode::Leaf(tr));
                    }
                }
            }
        }
    }
    out
}

/// A rail jump-target: DOM `id`, display label, and a name to pick an icon.
#[derive(Clone, PartialEq)]
struct RailGroup {
    anchor: String,
    label: String,
    icon_name: String,
}

/// A mixer bound to a flat, folder-depth-ordered track list.
#[component]
pub fn MixerView(
    tracks: Vec<Track>,
    /// `(track_guid, volume 0.0..=1.0)`
    on_volume: Callback<(String, f64)>,
    /// toggle mute for `track_guid`
    on_mute: Callback<String>,
    /// toggle solo for `track_guid`
    on_solo: Callback<String>,
    /// Optional pan edit: `(track_guid, pan -1.0..=1.0)`. When absent the pan
    /// control renders read-only (the existing call sites don't wire it).
    #[props(default)]
    on_pan: Option<Callback<(String, f64)>>,
    /// Optional live meter levels: `track_guid -> peak 0.0..=1.0`. When absent
    /// (or missing a guid) the strip shows a static gutter. The app feeds this
    /// from per-stem `AnalyserNodes`; this component only reads it.
    #[props(default)]
    levels: Option<HashMap<String, f32>>,
) -> Element {
    // Purely-visual hover focus (adds a ring). Authoritative selection is
    // `Track::selected`.
    let hovered = use_signal(|| Option::<String>::None);

    // Organize into a submix tree, then build the left rail from top-level
    // folders (each gets a stable scroll-anchor id).
    let nodes = organize(&tracks);
    let rail: Vec<RailGroup> = nodes
        .iter()
        .enumerate()
        .filter_map(|(k, n)| match n {
            MixNode::Folder(f) => Some(RailGroup {
                anchor: format!("mixgrp-{k}"),
                label: f.name.clone(),
                icon_name: f.name.clone(),
            }),
            MixNode::Leaf(_) => None,
        })
        .collect();

    rsx! {
        div { class: "flex h-full w-full bg-background text-foreground select-none",
            // Inject the fader-cap styling once for the whole console.
            style { dangerous_inner_html: FADER_CSS }

            // ── Left rail: instrument-group jump buttons ───────────────────
            if !rail.is_empty() {
                div {
                    class: "flex flex-col gap-1 shrink-0 border-r border-border bg-card/60 \
                            px-1.5 py-2 overflow-y-auto",
                    span {
                        class: "px-1 pb-1 text-[9px] font-bold uppercase tracking-widest \
                                text-muted-foreground",
                        "Groups"
                    }
                    for g in rail.iter().cloned() {
                        button {
                            class: "flex items-center gap-1.5 rounded px-1.5 py-1 text-left \
                                    text-[10px] font-semibold text-muted-foreground \
                                    hover:bg-accent hover:text-foreground",
                            onclick: {
                                let anchor = g.anchor;
                                move |_| {
                                    let _ = dioxus::document::eval(&format!(
                                        "document.getElementById('{anchor}')?.scrollIntoView({{inline:'start',block:'nearest',behavior:'smooth'}});"
                                    ));
                                }
                            },
                            span { class: "shrink-0", {instrument_icon(&g.icon_name)} }
                            span { class: "truncate", "{g.label}" }
                        }
                    }
                }
            }

            // ── Console body (horizontally scrollable) ─────────────────────
            div { class: "flex-1 overflow-x-auto",
                div { class: "flex h-full items-stretch gap-1.5 p-2",
                    if nodes.is_empty() {
                        div { class: "flex h-full w-full items-center justify-center \
                                      text-xs text-muted-foreground",
                            "No channels"
                        }
                    }
                    for (k, node) in nodes.iter().cloned().enumerate() {
                        {
                            match node {
                                MixNode::Leaf(track) => {
                                    let level = levels.as_ref().and_then(|m| m.get(&track.guid).copied());
                                    rsx! {
                                        ChannelStrip {
                                            key: "{track.guid}",
                                            track,
                                            on_volume,
                                            on_mute,
                                            on_solo,
                                            on_pan,
                                            hovered,
                                            level,
                                        }
                                    }
                                }
                                MixNode::Folder(folder) => rsx! {
                                    MixGroup {
                                        key: "grp-{k}",
                                        folder,
                                        on_volume,
                                        on_mute,
                                        on_solo,
                                        on_pan,
                                        hovered,
                                        levels: levels.clone(),
                                        anchor: format!("mixgrp-{k}"),
                                        depth: 0usize,
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A collapsible submix folder: a group-master mini-strip (its own mute / solo
/// / fader) plus, when expanded, its child strips (and nested folders).
#[component]
fn MixGroup(
    folder: FolderNode,
    on_volume: Callback<(String, f64)>,
    on_mute: Callback<String>,
    on_solo: Callback<String>,
    on_pan: Option<Callback<(String, f64)>>,
    hovered: Signal<Option<String>>,
    levels: Option<HashMap<String, f32>>,
    /// Scroll-anchor id (top-level folders only; nested pass "").
    #[props(default)]
    anchor: String,
    /// Nesting depth (for a subtle indent tint).
    #[props(default)]
    depth: usize,
) -> Element {
    let mut collapsed = use_signal(|| false);

    let leaves = folder.leaves();
    let count = leaves.len();
    let all_muted = !leaves.is_empty() && leaves.iter().all(|t| t.muted);
    let any_solo = leaves.iter().any(|t| t.soloed);
    let avg_vol = if leaves.is_empty() {
        1.0
    } else {
        let count = u32::try_from(leaves.len()).unwrap_or(1);
        leaves.iter().map(|t| t.volume).sum::<f64>() / f64::from(count)
    };
    let accent = color_hex(leaves.iter().find_map(|t| t.color));
    let db_label = fader_db_label(avg_vol);

    // Snapshots captured by the group-control closures.
    let leaves_m = leaves.clone();
    let leaves_s = leaves.clone();
    let leaves_v = leaves;

    let border = if depth == 0 {
        "border-primary/40"
    } else {
        "border-primary/25"
    };
    let id_attr = if anchor.is_empty() {
        None
    } else {
        Some(anchor)
    };

    rsx! {
        div {
            class: "flex h-full shrink-0 flex-col rounded-md border {border} bg-primary/5",
            id: id_attr,

            // ── Folder header: collapse · icon · name · count ──────────────
            div { class: "flex items-center gap-1 border-b border-border px-1.5 py-1",
                button {
                    class: "flex h-4 w-4 items-center justify-center rounded text-[10px] \
                            text-muted-foreground hover:bg-accent hover:text-foreground",
                    onclick: move |_| {
                        let v = collapsed();
                        collapsed.set(!v);
                    },
                    title: if collapsed() { "Expand group" } else { "Collapse group" },
                    if collapsed() { "▸" } else { "▾" }
                }
                span { class: "text-foreground", {instrument_icon(&folder.name)} }
                span {
                    class: "flex-1 truncate text-[10px] font-bold uppercase tracking-wide text-foreground",
                    title: "{folder.name}",
                    "{folder.name}"
                }
                span {
                    class: "rounded bg-muted px-1 text-[8px] font-mono text-muted-foreground",
                    "{count}"
                }
            }

            // ── Body: group master strip + (optionally) child strips ───────
            div { class: "flex flex-1 items-stretch gap-1 p-1 min-h-0",

                // Group master mini-strip.
                div {
                    class: "flex h-full w-[58px] shrink-0 flex-col items-stretch overflow-hidden \
                            rounded-md border border-primary/30 bg-card",
                    div {
                        class: "px-1 pt-1 text-center text-[8px] font-bold uppercase \
                                tracking-wide text-primary",
                        "Group"
                    }
                    div { class: "flex flex-1 items-stretch justify-center px-1 py-1 min-h-0",
                        input {
                            r#type: "range",
                            min: "0",
                            max: "1",
                            step: "0.005",
                            value: "{avg_vol}",
                            class: "fts-fader h-full",
                            style: "writing-mode: vertical-lr; direction: rtl; width: 30px; --fader: {accent};",
                            oninput: move |e| {
                                if let Ok(v) = e.value().parse::<f64>() {
                                    for t in &leaves_v {
                                        on_volume.call((t.guid.clone(), v));
                                    }
                                }
                            },
                        }
                    }
                    div { class: "px-1 pb-0.5 text-center",
                        span { class: "font-mono text-[10px] font-semibold text-foreground", "{db_label}" }
                    }
                    div { class: "flex gap-1 px-1 pb-1",
                        button {
                            class: if any_solo {
                                "flex-1 rounded py-0.5 text-[10px] font-bold bg-amber-400 text-black shadow-inner"
                            } else {
                                "flex-1 rounded py-0.5 text-[10px] font-bold bg-muted text-muted-foreground hover:bg-accent"
                            },
                            title: "Solo group",
                            onclick: move |_| {
                                let desired = !leaves_s.iter().any(|t| t.soloed);
                                for t in &leaves_s {
                                    if t.soloed != desired {
                                        on_solo.call(t.guid.clone());
                                    }
                                }
                            },
                            "S"
                        }
                        button {
                            class: if all_muted {
                                "flex-1 rounded py-0.5 text-[10px] font-bold bg-red-600 text-white shadow-inner"
                            } else {
                                "flex-1 rounded py-0.5 text-[10px] font-bold bg-muted text-muted-foreground hover:bg-accent"
                            },
                            title: "Mute group",
                            onclick: move |_| {
                                let desired = leaves_m.is_empty() || leaves_m.iter().any(|t| !t.muted);
                                for t in &leaves_m {
                                    if t.muted != desired {
                                        on_mute.call(t.guid.clone());
                                    }
                                }
                            },
                            "M"
                        }
                    }
                    div {
                        class: "mt-auto flex h-6 items-center justify-center px-1 text-center",
                        style: "background: {accent};",
                        span {
                            class: "w-full truncate text-[8px] font-semibold text-white \
                                    drop-shadow-[0_1px_1px_rgba(0,0,0,0.8)]",
                            "{folder.name}"
                        }
                    }
                }

                // Child strips (hidden when collapsed).
                if !collapsed() {
                    for child in folder.children.iter().cloned() {
                        {
                            match child {
                                MixNode::Leaf(track) => {
                                    let level = levels.as_ref().and_then(|m| m.get(&track.guid).copied());
                                    rsx! {
                                        ChannelStrip {
                                            key: "{track.guid}",
                                            track,
                                            on_volume,
                                            on_mute,
                                            on_solo,
                                            on_pan,
                                            hovered,
                                            level,
                                        }
                                    }
                                }
                                MixNode::Folder(sub) => rsx! {
                                    MixGroup {
                                        key: "{sub.name}",
                                        folder: sub,
                                        on_volume,
                                        on_mute,
                                        on_solo,
                                        on_pan,
                                        hovered,
                                        levels: levels.clone(),
                                        depth: depth + 1,
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A single channel strip (leaf): icon, pan, routing, colored fader, dB scale,
/// meter, dB readout, solo / mute, and a colored name footer.
#[component]
fn ChannelStrip(
    track: Track,
    on_volume: Callback<(String, f64)>,
    on_mute: Callback<String>,
    on_solo: Callback<String>,
    on_pan: Option<Callback<(String, f64)>>,
    hovered: Signal<Option<String>>,
    /// Live peak level `0.0..=1.0`, or `None` for a static gutter.
    level: Option<f32>,
) -> Element {
    let guid = track.guid.clone();

    // The track's own REAPER color (0xRRGGBB) — the ONLY raw color allowed for
    // chrome; everything else is theme tokens.
    let accent = color_hex(track.color);

    let db_label = fader_db_label(track.volume);
    let is_hovered = hovered.read().as_deref() == Some(guid.as_str());
    let focused = track.selected || is_hovered;

    let pan_label = if track.pan.abs() < 0.02 {
        "C".to_string()
    } else if track.pan < 0.0 {
        let pan_pct = pct_from_ratio(track.pan.abs());
        format!("L{pan_pct}")
    } else {
        let pan_pct = pct_from_ratio(track.pan);
        format!("R{pan_pct}")
    };

    let ring = if focused {
        "ring-2 ring-primary"
    } else {
        "ring-1 ring-transparent"
    };
    let strip_class = format!(
        "group relative flex h-full w-[64px] shrink-0 flex-col items-stretch overflow-hidden \
         rounded-md border border-border bg-card/70 {ring}"
    );

    // Meter fill: live level → height + green/yellow/red gradient (color rises
    // toward the top as the fill grows). Static fallback is a faint
    // volume-proportional bar (no motion, no fake data).
    let (meter_h, meter_style) = if let Some(l) = level {
        let pct = pct_from_ratio(f64::from(l.clamp(0.0, 1.0)));
        (
            pct,
            "background:linear-gradient(to top,#22c55e,#eab308 70%,#ef4444);".to_string(),
        )
    } else {
        let pct = pct_from_ratio(track.volume.clamp(0.0, 1.0));
        (
            pct,
            "background:var(--muted-foreground);opacity:0.3;".to_string(),
        )
    };

    let (g_vol, g_mute, g_solo, g_pan) = (guid.clone(), guid.clone(), guid.clone(), guid.clone());
    let h_enter = guid;

    rsx! {
        div {
            class: "{strip_class}",
            onmouseenter: move |_| hovered.set(Some(h_enter.clone())),
            onmouseleave: move |_| hovered.set(None),

            // ── Instrument icon ────────────────────────────────────────────
            div { class: "flex items-center justify-center pt-1 text-foreground",
                {instrument_icon(&track.name)}
            }

            // ── Header: index + arm ────────────────────────────────────────
            div { class: "flex items-center justify-between px-1.5",
                span { class: "text-[9px] font-mono text-muted-foreground", "{track.index}" }
                if track.armed {
                    span { class: "h-2 w-2 rounded-full bg-red-600" }
                }
            }

            // ── Pan control ────────────────────────────────────────────────
            div { class: "px-1.5 pt-1",
                input {
                    r#type: "range",
                    min: "-1",
                    max: "1",
                    step: "0.02",
                    value: "{track.pan}",
                    disabled: on_pan.is_none(),
                    class: "h-1.5 w-full cursor-pointer accent-primary disabled:cursor-default \
                            disabled:opacity-60",
                    oninput: move |e| {
                        if let (Some(cb), Ok(p)) = (on_pan, e.value().parse::<f64>()) {
                            cb.call((g_pan.clone(), p));
                        }
                    },
                }
                div { class: "mt-0.5 flex items-center justify-between text-[7px] text-muted-foreground",
                    span { "L" }
                    span { class: "font-mono text-foreground", "{pan_label}" }
                    span { "R" }
                }
            }

            // ── Routing icon buttons (decorative desk chrome) ──────────────
            div { class: "flex items-center justify-center gap-1 px-1.5 pt-1",
                span { class: "flex h-4 w-4 items-center justify-center rounded bg-muted \
                               text-[8px] text-muted-foreground",
                    "◇"
                }
                span { class: "flex h-4 w-4 items-center justify-center rounded bg-muted \
                               text-[8px] text-muted-foreground",
                    "⌂"
                }
                if track.phase_inverted {
                    span { class: "flex h-4 w-4 items-center justify-center rounded bg-muted \
                                   text-[8px] font-bold text-primary",
                        "ø"
                    }
                }
            }

            // ── Fader region: dB scale | meter | colored fader ─────────────
            div { class: "flex flex-1 items-stretch justify-center gap-1 px-1 py-1 min-h-0",

                // dB scale ticks, aligned top(0 dB) → bottom(-∞).
                div { class: "flex flex-col justify-between py-0.5 text-right",
                    for t in DB_TICKS.iter() {
                        span { class: "text-[7px] leading-none text-muted-foreground", "{t}" }
                    }
                }

                // Level meter gutter — prop-driven; no live data ⇒ static.
                div { class: "flex w-1.5 flex-col justify-end overflow-hidden rounded-sm bg-muted/60",
                    div {
                        class: "w-full",
                        style: "height: {meter_h}%; {meter_style}",
                    }
                }

                // Vertical fader — rotated range input; cap tinted by --fader.
                div { class: "flex items-stretch justify-center",
                    input {
                        r#type: "range",
                        min: "0",
                        max: "1",
                        step: "0.005",
                        value: "{track.volume}",
                        class: "fts-fader h-full",
                        style: "writing-mode: vertical-lr; direction: rtl; width: 30px; --fader: {accent};",
                        oninput: move |e| {
                            if let Ok(v) = e.value().parse::<f64>() {
                                on_volume.call((g_vol.clone(), v));
                            }
                        },
                    }
                }
            }

            // ── dB readout ─────────────────────────────────────────────────
            div { class: "px-1 pb-0.5 text-center",
                span {
                    class: "font-mono text-[10px] font-semibold text-foreground",
                    "{db_label}"
                }
            }

            // ── Solo / Mute ────────────────────────────────────────────────
            div { class: "flex gap-1 px-1.5 pb-1",
                button {
                    class: if track.soloed {
                        "flex-1 rounded py-0.5 text-[10px] font-bold bg-amber-400 text-black shadow-inner"
                    } else {
                        "flex-1 rounded py-0.5 text-[10px] font-bold bg-muted text-muted-foreground hover:bg-accent"
                    },
                    onclick: move |_| on_solo.call(g_solo.clone()),
                    "S"
                }
                button {
                    class: if track.muted {
                        "flex-1 rounded py-0.5 text-[10px] font-bold bg-red-600 text-white shadow-inner"
                    } else {
                        "flex-1 rounded py-0.5 text-[10px] font-bold bg-muted text-muted-foreground hover:bg-accent"
                    },
                    onclick: move |_| on_mute.call(g_mute.clone()),
                    "M"
                }
            }

            // ── Colored footer bar with the track name ─────────────────────
            div {
                class: "mt-auto flex h-7 items-center justify-center px-1 text-center",
                style: "background: {accent};",
                title: "{track.name}",
                span {
                    class: "w-full truncate text-[9px] font-semibold leading-tight text-white \
                            drop-shadow-[0_1px_1px_rgba(0,0,0,0.8)]",
                    "{track.name}"
                }
            }
        }
    }
}
