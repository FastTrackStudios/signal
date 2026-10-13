//! [`TemplateTarget`] over the raw `.RPP` chunk tree — the lossless offline
//! backend.
//!
//! # Why this exists
//!
//! [`super::dawfile::RppTarget`] wraps `ReaperProject`, a *typed* model of a
//! project. Applying a template through it is fine; writing the result back is
//! not, because `ReaperProject::to_rpp_string` can only emit the fields the
//! type models. Everything else in the file — and a REAPER project is mostly
//! everything else — is dropped on the floor. Organizing one real album session
//! through it lost the entire master track, `<NOTES>`, `<RECORD_CFG>`, all
//! twelve `RENDER_*` settings, `<METRONOME>`, `<PROJBAY>`, `<EXTENSIONS>`, every
//! per-item `CHANMODE`/`YPOS`, and all 22 `<EXT>` blocks carrying the original
//! capture filenames. REAPER opened the result with "11160 elements in the
//! project were not understood".
//!
//! That is not a bug to fix field by field. A typed model of a format this
//! large will always trail the format, and each gap is invisible until a real
//! session hits it.
//!
//! So this backend never builds a typed model. It edits
//! [`dawfile_reaper::RChunk`] — the generic line/chunk tree — in place, and
//! writes it back. Lines it does not touch are returned to disk byte for byte
//! (`rpp_tree::render_line` keeps each node's original text while its tokens
//! are unmodified), so organizing a project is a *diff*, not a rewrite: the
//! only lines that change are the ones the template actually changed.
//!
//! # Track ids are indices
//!
//! [`RChunkTarget::TrackId`] is a position in the project's sequence of
//! `<TRACK>` chunks — the same contract [`super::dawfile::RppTarget`] has, and
//! for the same reason. REAPER's `AUXRECV` names its source track by index, so
//! a mid-list insert would silently repoint existing sends. This backend only
//! ever appends, except in
//! [`gather_into_folder`](TemplateTarget::gather_into_folder), which reorders
//! and rewrites every `AUXRECV` index to match.

use color_palette::Color;
use daw_proto::FolderDepthChange;
use dawfile_reaper::rpp_tree::{tokenize, RChunk, RNode, RNodeTree, RToken};

use super::{find_group, TemplateTarget};

/// A parsed `.RPP` chunk tree the template can be applied to.
pub struct RChunkTarget<'a> {
    root: &'a mut RChunk,
}

/// This backend edits an in-memory tree; nothing here can fail, so the error
/// type is uninhabited and `?` on it is free.
pub use super::dawfile::Never;

impl<'a> RChunkTarget<'a> {
    /// Borrow the root `<REAPER_PROJECT>` chunk as a template target.
    pub const fn new(root: &'a mut RChunk) -> Self {
        Self { root }
    }

    /// The wrapped tree.
    #[must_use]
    pub const fn root(&self) -> &RChunk {
        self.root
    }

    /// Positions of every `<TRACK>` chunk within the root's children.
    ///
    /// Recomputed on each call rather than cached: a stale index here would
    /// repoint a send at the wrong track, and the list is a few hundred entries
    /// on the largest real session.
    fn track_slots(&self) -> Vec<usize> {
        self.root
            .children
            .iter()
            .enumerate()
            .filter(|(_, child)| matches!(child, RNodeTree::Chunk(c) if is_track(c)))
            .map(|(i, _)| i)
            .collect()
    }

    fn track(&self, id: usize) -> Option<&RChunk> {
        let slot = *self.track_slots().get(id)?;
        match self.root.children.get(slot)? {
            RNodeTree::Chunk(c) => Some(c),
            RNodeTree::Node(_) => None,
        }
    }

    fn track_mut(&mut self, id: usize) -> Option<&mut RChunk> {
        let slot = *self.track_slots().get(id)?;
        match self.root.children.get_mut(slot)? {
            RNodeTree::Chunk(c) => Some(c),
            RNodeTree::Node(_) => None,
        }
    }

    /// Tracks whose folder depth goes negative — each closes a folder that was
    /// never opened.
    ///
    /// REAPER tolerates this by clamping, so a project can carry the damage
    /// invisibly for years, but the nesting it describes is not a tree.
    /// Anything that reasons about folder membership is working from bad data
    /// until it is repaired.
    #[must_use]
    pub fn negative_depths(&self) -> Vec<(usize, String, i32)> {
        let mut depth: i32 = 0;
        let mut out = Vec::new();
        for (id, _) in self.track_slots().iter().enumerate() {
            depth = depth.saturating_add(self.indentation(id));
            if depth < 0 {
                out.push((id, self.track_name(id).unwrap_or_default(), depth));
            }
        }
        out
    }

    /// Folder nesting depth of each track, 0 at the top level.
    ///
    /// REAPER stores nesting as a per-track *change*, so a track's own depth is
    /// the running sum of every change before it — a track with no `ISBUS` line
    /// at all can still sit three folders deep.
    fn running_depths(&self) -> Vec<i32> {
        let mut depth: i32 = 0;
        (0..self.track_slots().len())
            .map(|id| {
                let here = depth;
                depth = depth.saturating_add(self.indentation(id));
                here
            })
            .collect()
    }

    fn track_name(&self, id: usize) -> Option<String> {
        param(self.track(id)?, "NAME", 0)
    }

    /// The `ISBUS` indentation delta, 0 when the track has no `ISBUS` line.
    fn indentation(&self, id: usize) -> i32 {
        self.track(id)
            .and_then(|t| param(t, "ISBUS", 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }

    /// The `ISBUS` folder state, 0 (a regular track) when there is no line.
    fn folder_state(&self, id: usize) -> i32 {
        self.track(id)
            .and_then(|t| param(t, "ISBUS", 0))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }

    /// Whether the track carries no folder structure of its own — neither
    /// opening a folder nor closing one.
    fn is_plain(&self, id: usize) -> bool {
        self.folder_state(id) == 0 && self.indentation(id) == 0
    }

    /// How many tracks the project has.
    #[must_use]
    pub fn track_count(&self) -> usize {
        self.track_slots().len()
    }

    /// The track's name, for reporting.
    #[must_use]
    pub fn name_of(&self, id: usize) -> Option<String> {
        self.track_name(id)
    }

    /// Whether the track opens a folder or closes one — the reason
    /// [`gather_into_folder`](TemplateTarget::gather_into_folder) refuses to
    /// move it.
    #[must_use]
    pub fn carries_folder_structure(&self, id: usize) -> bool {
        !self.is_plain(id)
    }
}

/// Whether a chunk is a `<TRACK>`.
fn is_track(chunk: &RChunk) -> bool {
    chunk.name().as_deref() == Some("TRACK")
}

/// The `index`-th parameter of the first line in `chunk` named `key`.
///
/// Parameters are 0-based *after* the key, so `param(t, "ISBUS", 1)` is the
/// indentation in `ISBUS 2 -1`.
fn param(chunk: &RChunk, key: &str, index: usize) -> Option<String> {
    chunk.children.iter().find_map(|child| match child {
        RNodeTree::Node(node) => {
            let mut clone = node.clone();
            (clone.get_name().as_deref() == Some(key)).then(|| clone.get_param(index))?
        }
        RNodeTree::Chunk(_) => None,
    })
}

/// Replace the first line in `chunk` named `key` with `line`, or insert one.
///
/// Replacing in place matters: it keeps the property where REAPER wrote it,
/// so the diff of an organized project shows the value changing rather than a
/// line moving. A key the track does not have yet is inserted directly after
/// `NAME`, which is where REAPER groups the track's own properties.
fn set_line(chunk: &mut RChunk, key: &str, line: String) {
    let node = RNodeTree::Node(RNode {
        // Both fields, and consistent with each other, so this line renders
        // verbatim and later reads of it tokenize as expected.
        tokens: Some(tokenize(&line)),
        line: Some(line),
    });

    let existing = chunk.children.iter().position(|child| match child {
        RNodeTree::Node(n) => {
            let mut clone = n.clone();
            clone.get_name().as_deref() == Some(key)
        }
        RNodeTree::Chunk(_) => false,
    });

    if let Some(i) = existing {
        if let Some(slot) = chunk.children.get_mut(i) {
            *slot = node;
        }
    } else {
        let after_name = chunk
            .children
            .iter()
            .position(|child| match child {
                RNodeTree::Node(n) => {
                    let mut clone = n.clone();
                    clone.get_name().as_deref() == Some("NAME")
                }
                RNodeTree::Chunk(_) => false,
            })
            .map_or(0, |i| i.saturating_add(1));
        chunk
            .children
            .insert(after_name.min(chunk.children.len()), node);
    }
}

/// Remove every line in `chunk` named `key`, returning how many went.
fn remove_lines(chunk: &mut RChunk, key: &str) -> usize {
    let before = chunk.children.len();
    chunk.children.retain(|child| match child {
        RNodeTree::Node(n) => {
            let mut clone = n.clone();
            clone.get_name().as_deref() != Some(key)
        }
        RNodeTree::Chunk(_) => true,
    });
    before.saturating_sub(chunk.children.len())
}

/// Append a line to `chunk`.
fn push_line(chunk: &mut RChunk, line: String) {
    chunk.add_node(RNodeTree::Node(RNode {
        tokens: Some(tokenize(&line)),
        line: Some(line),
    }));
}

/// Rewrite every `AUXRECV` source index in `chunk` through `remap`.
///
/// REAPER records a send on the *destination* track, naming its source by
/// position in the track list. Reordering tracks without this leaves every
/// existing send pointing at whatever track now occupies the old index —
/// silently rerouting audio rather than failing.
fn repoint_receives(chunk: &mut RChunk, remap: &[usize]) {
    let repointed: Vec<String> = chunk
        .children
        .iter()
        .filter_map(|child| match child {
            RNodeTree::Node(node) => {
                let mut clone = node.clone();
                (clone.get_name().as_deref() == Some("AUXRECV")).then(|| {
                    let mut tokens: Vec<String> = clone
                        .get_tokens()
                        .iter()
                        .map(|t| t.get_string().to_string())
                        .collect();
                    if let Some(src) = tokens.get_mut(1) {
                        if let Some(mapped) = src
                            .parse::<i32>()
                            .ok()
                            .filter(|v| *v >= 0)
                            .and_then(|v| usize::try_from(v).ok())
                            .and_then(|v| remap.get(v).copied())
                            .filter(|v| *v != usize::MAX)
                            .and_then(|v| i32::try_from(v).ok())
                        {
                            *src = mapped.to_string();
                        }
                    }
                    tokens.join(" ")
                })
            }
            RNodeTree::Chunk(_) => None,
        })
        .collect();
    if repointed.is_empty() {
        return;
    }
    remove_lines(chunk, "AUXRECV");
    for line in repointed {
        push_line(chunk, line);
    }
}

/// Build a fresh `<TRACK>` chunk named `name`, with `isbus` as its folder line.
///
/// The lines here are the minimum REAPER needs to treat this as a real track;
/// it fills in everything else from its own defaults on load. `TRACKID` and
/// the chunk's own GUID are the same value, as REAPER writes them.
fn new_track_chunk(name: &str, isbus: &str) -> RChunk {
    let guid = derived_guid(name);
    let mut chunk = RChunk::new(vec![RToken::new("TRACK"), RToken::new(guid.clone())]);
    for line in [
        format!("NAME {}", quoted(name)),
        "PEAKCOL 16576".to_string(),
        "BEAT -1".to_string(),
        "AUTOMODE 0".to_string(),
        "VOLPAN 1 0 -1 -1 1".to_string(),
        "MUTESOLO 0 0 0".to_string(),
        "IPHASE 0".to_string(),
        "PLAYOFFS 0 1".to_string(),
        isbus.to_string(),
        "NCHAN 2".to_string(),
        "FX 1".to_string(),
        format!("TRACKID {guid}"),
        "PERF 0".to_string(),
        "MIDIOUT -1".to_string(),
        "MAINSEND 1 0".to_string(),
    ] {
        push_line(&mut chunk, line);
    }
    chunk
}

/// A REAPER-shaped GUID derived from `name`.
///
/// Deterministic on purpose: organizing the same project twice must produce
/// the same bytes, or no diff of the output means anything and the pipeline
/// cannot be checked for idempotence. Collision with a REAPER-generated GUID
/// is not a practical concern — and a track GUID only has to be unique within
/// its own project.
fn derived_guid(name: &str) -> String {
    // FNV-1a, run over the name with four different offsets to fill 128 bits.
    let mut parts = [0u32; 4];
    for (i, part) in parts.iter_mut().enumerate() {
        let salt = u32::try_from(i).unwrap_or(0).wrapping_mul(0x9e37_79b9);
        let mut hash: u32 = 0x811c_9dc5 ^ salt;
        for byte in name.as_bytes() {
            hash ^= u32::from(*byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
        *part = hash;
    }
    let [a, b, c, d] = parts;
    format!(
        "{{{a:08X}-{:04X}-{:04X}-{:04X}-{:04X}{d:08X}}}",
        b >> 16,
        b & 0xffff,
        c >> 16,
        c & 0xffff,
    )
}

impl TemplateTarget for RChunkTarget<'_> {
    /// Nest a "DI" capture under its sibling primary track, for any group that
    /// opts in via [`monarchy::Group::nest_secondary_mics`] — electric guitar
    /// turns this on for its DI feed.
    ///
    /// Only reshapes the exact adjacency the house convention already tracks
    /// in: a "DI" track immediately following, at the same folder depth, a
    /// non-DI sibling. No track changes position, so no `AUXRECV` index needs
    /// rewriting — only folder nesting, mute, and collapse flags change.
    ///
    /// Idempotent: once nested, "DI" sits inside its sibling rather than beside
    /// it, so a second pass no longer sees them as siblings.
    fn nest_secondary_mics(&mut self) {
        let config = crate::default_config();
        let entries = super::contextual_paths(self);

        for pair in entries.windows(2) {
            let [main, di] = pair else { continue };
            if main.context != di.context {
                continue; // not siblings
            }
            let is_di = |name: &str| name.trim().eq_ignore_ascii_case("di");
            if !is_di(&di.name) || is_di(&main.name) {
                continue;
            }
            let Some(leaf) = di.path.last() else { continue };
            if !find_group(&config, leaf).is_some_and(|g| g.nest_secondary_mics) {
                continue;
            }

            let di_indentation = self.indentation(di.track);
            if !self.is_plain(main.track) || di_indentation > 0 {
                // Not the plain "two siblings" shape this expects — leave
                // whatever unusual structure is already there alone.
                continue;
            }

            if let Some(track) = self.track_mut(main.track) {
                set_line(track, "ISBUS", "ISBUS 1 1".to_string());
                set_line(track, "BUSCOMP", "BUSCOMP 2 2 0 0 0".to_string());
            }
            if let Some(track) = self.track_mut(di.track) {
                // One more level to close than before: the DI folder we just
                // opened on `main`, on top of whatever it already closed.
                let closes = di_indentation.saturating_sub(1);
                set_line(track, "ISBUS", format!("ISBUS 2 {closes}"));
                set_line(track, "MUTESOLO", "MUTESOLO 1 0 0".to_string());
            }
        }
    }

    type TrackId = usize;
    type Error = Never;

    fn find_track(&self, name: &str) -> Option<usize> {
        (0..self.track_slots().len()).find(|id| {
            self.track_name(*id)
                .is_some_and(|n| n.trim().eq_ignore_ascii_case(name.trim()))
        })
    }

    fn append_track(&mut self, name: &str) -> Result<usize, Never> {
        let chunk = new_track_chunk(name, "ISBUS 0 0");

        // Append after the last existing track, not at the very end — the root
        // also carries trailing chunks (`<EXTENSIONS>`, `<PROJBAY>`) that must
        // stay behind the tracks.
        let at = self
            .track_slots()
            .last()
            .map_or(self.root.children.len(), |last| last.saturating_add(1));
        self.root.children.insert(at, RNodeTree::Chunk(chunk));
        Ok(self.track_slots().len().saturating_sub(1))
    }

    fn set_folder_depth(&mut self, id: &usize, depth: FolderDepthChange) -> Result<(), Never> {
        // REAPER's ISBUS is (folder_state, indentation): a folder parent is
        // (1, 1), the last track inside n folders is (2, -n), and an ordinary
        // track is (0, 0).
        let line = match depth {
            FolderDepthChange::Normal => "ISBUS 0 0".to_string(),
            FolderDepthChange::FolderStart => "ISBUS 1 1".to_string(),
            FolderDepthChange::ClosesLevels(n) => format!("ISBUS 2 {n}"),
        };
        if let Some(track) = self.track_mut(*id) {
            set_line(track, "ISBUS", line);
        }
        Ok(())
    }

    fn set_color(&mut self, id: &usize, hex: &str) -> Result<(), Never> {
        // An unparseable color leaves the track's own color alone rather than
        // failing the whole apply — the routing matters, the tint does not.
        let Ok(color) = Color::from_hex_str(hex) else {
            return Ok(());
        };
        let native = color.to_reaper_native();
        if let Some(track) = self.track_mut(*id) {
            set_line(track, "PEAKCOL", format!("PEAKCOL {native}"));
        }
        Ok(())
    }

    fn set_channel_count(&mut self, id: &usize, channels: u32) -> Result<(), Never> {
        if let Some(track) = self.track_mut(*id) {
            set_line(track, "NCHAN", format!("NCHAN {channels}"));
        }
        Ok(())
    }

    fn has_send(&self, source: &usize, dest: &usize) -> bool {
        let Ok(source) = i32::try_from(*source) else {
            return false;
        };
        self.track(*dest).is_some_and(|track| {
            track.children.iter().any(|child| match child {
                RNodeTree::Node(node) => {
                    let mut clone = node.clone();
                    clone.get_name().as_deref() == Some("AUXRECV")
                        && clone
                            .get_param(0)
                            .and_then(|v| v.parse::<i32>().ok())
                            .is_some_and(|src| src == source)
                }
                RNodeTree::Chunk(_) => false,
            })
        })
    }

    fn add_send(&mut self, source: &usize, dest: &usize) -> Result<(), Never> {
        // REAPER stores a send as an AUXRECV on the *destination* track, which
        // names its source by index — there is no send record on the source.
        // Fields: src mode volume pan mute mono phase srcch dstch panlaw
        // midichans automode.
        let src = i32::try_from(*source).unwrap_or(-1);
        let line = format!("AUXRECV {src} 0 1 0 0 0 0 0 0 -1 -1 -1");
        if let Some(track) = self.track_mut(*dest) {
            push_line(track, line);
        }
        Ok(())
    }

    fn set_parent_send(&mut self, id: &usize, enabled: bool) -> Result<(), Never> {
        let flag = i32::from(enabled);
        if let Some(track) = self.track_mut(*id) {
            set_line(track, "MAINSEND", format!("MAINSEND {flag} 0"));
        }
        Ok(())
    }

    fn folder_depths(&self) -> Vec<(usize, String, i32)> {
        (0..self.track_slots().len())
            .map(|id| {
                (
                    id,
                    self.track_name(id).unwrap_or_default(),
                    self.indentation(id),
                )
            })
            .collect()
    }

    fn gather_into_folder(
        &mut self,
        folder: &str,
        tracks: &[usize],
    ) -> Result<Option<crate::apply::Gathered<usize>>, Never> {
        // Only top-level tracks that carry no folder structure can be pulled
        // out. Three ways a track fails that, each of which breaks the project
        // differently:
        //
        // - a folder parent — moving it orphans its children;
        // - the track that closes a folder — moving it leaves that folder open
        //   over everything after it;
        // - anything nested inside a folder — it belongs to that folder, and
        //   yanking it out silently changes what the folder sums.
        let depths = self.running_depths();
        let mut movable: Vec<usize> = tracks
            .iter()
            .copied()
            .filter(|id| depths.get(*id) == Some(&0) && self.is_plain(*id))
            .collect();
        movable.sort_unstable();
        movable.dedup();
        if movable.is_empty() {
            return Ok(None);
        }

        let moving: std::collections::HashSet<usize> = movable.iter().copied().collect();
        let total = self.track_slots().len();

        // New order: everything staying, then the folder, then the moved
        // tracks in their original relative order.
        let staying: Vec<usize> = (0..total).filter(|id| !moving.contains(id)).collect();

        // old index → new index, for the AUXRECV rewrite below. The folder
        // track is inserted between the two runs, so the moved tracks shift by
        // one extra place.
        let mut remap = vec![usize::MAX; total];
        for (new, old) in staying.iter().enumerate() {
            if let Some(slot) = remap.get_mut(*old) {
                *slot = new;
            }
        }
        let folder_index = staying.len();
        for (offset, old) in movable.iter().enumerate() {
            if let Some(slot) = remap.get_mut(*old) {
                *slot = folder_index.saturating_add(1).saturating_add(offset);
            }
        }

        // Lift every track chunk out of the tree and put the new order back
        // where the first one was, leaving the non-track children — the
        // project header lines before them, `<EXTENSIONS>` and `<PROJBAY>`
        // after — untouched.
        //
        // This assumes the `<TRACK>` chunks are contiguous, which is how
        // REAPER writes a project. Anything interleaved between two tracks
        // would end up after all of them; that is the same assumption the
        // index-based `TrackId` already rests on.
        let slots = self.track_slots();
        let first_slot = slots.first().copied().unwrap_or(self.root.children.len());
        let mut taken: Vec<Option<RChunk>> = Vec::with_capacity(total);
        for slot in slots.iter().rev() {
            if let RNodeTree::Chunk(c) = self.root.children.remove(*slot) {
                taken.push(Some(c));
            }
        }
        taken.reverse();

        let mut reordered: Vec<RChunk> = Vec::with_capacity(total.saturating_add(1));
        for old in &staying {
            if let Some(chunk) = taken.get_mut(*old).and_then(Option::take) {
                reordered.push(chunk);
            }
        }

        // The folder track itself, which opens a folder rather than sitting in one.
        reordered.push(new_track_chunk(folder, "ISBUS 1 1"));

        for old in &movable {
            if let Some(mut chunk) = taken.get_mut(*old).and_then(Option::take) {
                // These carried no folder structure (checked above); inside the
                // folder they are plain members.
                set_line(&mut chunk, "ISBUS", "ISBUS 0 0".to_string());
                reordered.push(chunk);
            }
        }
        // The last member closes the folder we just opened.
        if let Some(last) = reordered.last_mut() {
            set_line(last, "ISBUS", "ISBUS 2 -1".to_string());
        }

        // A receive names its source by index, so every one of them has to
        // follow the tracks that moved.
        for chunk in &mut reordered {
            repoint_receives(chunk, &remap);
        }

        // Put the tracks back where the first one used to be.
        for (offset, chunk) in reordered.into_iter().enumerate() {
            self.root
                .children
                .insert(first_slot.saturating_add(offset), RNodeTree::Chunk(chunk));
        }

        Ok(Some(crate::apply::Gathered {
            folder: folder_index,
            moved: movable
                .iter()
                .map(|old| remap.get(*old).copied().unwrap_or(usize::MAX))
                .collect(),
            skipped: tracks
                .iter()
                .copied()
                .filter(|i| !movable.contains(i))
                .map(|old| remap.get(old).copied().unwrap_or(usize::MAX))
                .collect(),
        }))
    }
}

/// Quote a value for a REAPER line if it needs it.
fn quoted(value: &str) -> String {
    if value.is_empty() {
        "\"\"".to_string()
    } else if value.contains('"') {
        format!("'{value}'")
    } else if value.chars().any(char::is_whitespace) {
        format!("\"{value}\"")
    } else {
        value.to_string()
    }
}
