use daw::service::{
    DawError, DawResult, Items, Projects, Track, TrackShape, TrackTree, Tracks, TracksExt,
};
use dynamic_template::track_schema::{self, TrackDimension};
// The action contract lives in session-proto (traits are protocol);
// this file is the implementation. `register_track_manager_actions`
// is macro-emitted alongside the trait there.
pub use session_proto::track_manager::{TrackManagerActions, register_track_manager_actions};

/// Session's track-tree editor for one DAW backend.
///
/// This is what callers actually use: `TrackManager::new(daw).add_channel()`,
/// never `daw.add_channel()` — adding a dynamic-template channel/multi-mic/
/// arrangement isn't a generic DAW capability, it's session business logic
/// layered on top of one (`daw::service::Tracks`/`Items`/`Projects`), so it
/// isn't blanket-impl'd onto every backend the way `daw::service::TracksExt`
/// (generic selection/lookup/tree-building plumbing — `selected_scope`,
/// `track_tree`, `append_shape`, `set_depth`, ...) is. Everything left in
/// this file is dynamic-template-specific: which dimension a track name
/// reads as, and what shape each action builds out of that.
///
/// `TrackManager<D>` wraps whichever `D` for the duration it's used; it
/// always acts on `ProjectContext::Current` (matches how a REAPER named
/// command works — there's no "target a background project" for a
/// user-triggered action).
/// Production wraps `daw::reaper::Reaper`; tests wrap
/// `daw_standalone::sync::Standalone` to drive the tree logic headless,
/// with no live REAPER process — same trait impl either way.
pub struct TrackManager<D> {
    daw: D,
}

impl<D> TrackManager<D> {
    pub const fn new(daw: D) -> Self {
        Self { daw }
    }
}

/// Derefs to the wrapped backend so `Tracks`/`Items`/`Projects`/
/// `TracksExt` methods are callable directly as `self.method(...)` instead
/// of `self.daw.method(...)` everywhere below — `TrackManager`'s own
/// methods still resolve to themselves first, Rust only reaches through
/// `Deref` for names `Self` doesn't otherwise have.
impl<D> std::ops::Deref for TrackManager<D> {
    type Target = D;
    fn deref(&self) -> &D {
        &self.daw
    }
}

impl<D: Tracks + Items + Projects> TrackManagerActions for TrackManager<D> {
    /// Three cases, in order: an existing channel layer to extend, bare
    /// multi-mics to fold underneath new channels, or nothing yet
    /// (scaffold the first L/R pair).
    fn add_channel(&self) -> DawResult<()> {
        let scope = self.selected_scope()?;
        let tree = self.track_tree();
        let children: Vec<&Track> = tree.children_of(&scope.guid).collect();

        if let Some(existing) = children
            .iter()
            .find(|t| Self::dimension_of(t, &scope) == TrackDimension::Channel)
        {
            let taken: Vec<&str> = children.iter().map(|t| t.name.as_str()).collect();
            let next = Self::next_value(TrackDimension::Channel, &scope, &taken)?;
            // Mirror the existing channel's own subtree onto the new one,
            // so a second channel arrives with the same mics as the first.
            let inherited = tree.shape_of_children(&existing.guid);
            return self.append_shape(&scope.guid, &[TrackShape::with_children(next, inherited)]);
        }

        let bare_multi_mics: Vec<String> = children
            .iter()
            .filter(|t| Self::dimension_of(t, &scope) == TrackDimension::MultiMic)
            .map(|t| t.name.clone())
            .collect();
        if !bare_multi_mics.is_empty() {
            return self.split_multi_mics_across_channels(&scope, &tree, &bare_multi_mics);
        }

        self.scaffold_first_channels(&scope)
    }

    fn add_layer(&self) -> DawResult<()> {
        self.add_named_scope("DBL")
    }

    fn add_multi_mic(&self) -> DawResult<()> {
        let scope = self.selected_scope()?;
        let taken: Vec<String> = self
            .track_tree()
            .children_of(&scope.guid)
            .map(|t| t.name.clone())
            .collect();
        let taken: Vec<&str> = taken.iter().map(String::as_str).collect();

        let next = Self::next_value(TrackDimension::MultiMic, &scope, &taken)?;
        self.append_child(&scope.guid, &next)
    }

    fn add_performer(&self) -> DawResult<()> {
        self.add_named_scope("New Performer")
    }

    fn add_arrangement(&self) -> DawResult<()> {
        let selected = self.selected_scope()?;
        let scope = Self::instrument_scope(&self.track_tree(), selected);
        self.append_child(&scope.guid, "<ArrangementDescriptor>")
    }

    fn reorganize_selected_by_performer(&self) -> DawResult<()> {
        self.reorganize_selected_by("Performer")
    }

    fn reorganize_selected_by_arrangement(&self) -> DawResult<()> {
        self.reorganize_selected_by("Arrangement")
    }
}

// ── add_channel's three cases ───────────────────────────────────────

impl<D: Tracks + Items + Projects> TrackManager<D> {
    /// The channel pair a scope starts with when it has none yet.
    const INITIAL_CHANNELS: [&'static str; 2] = ["L", "R"];

    /// No channels and no mics yet: open `scope` as a folder holding a
    /// fresh L/R pair, and move any items already on it onto L.
    fn scaffold_first_channels(&self, scope: &Track) -> DawResult<()> {
        let [left, right] = Self::INITIAL_CHANNELS;
        self.set_depth(&scope.guid, 1)?;
        let l = self.insert_track_at(left, scope.index.saturating_add(1))?;
        let r = self.insert_track_at(right, scope.index.saturating_add(2))?;
        self.set_depth(&l, 0)?;
        self.set_depth(&r, -1)?;
        self.move_items(&scope.guid, &l)
    }

    /// `scope` already has bare multi-mics directly under it: wrap those
    /// in place under an L channel, then add an R channel carrying a copy
    /// of the same mic names.
    fn split_multi_mics_across_channels(
        &self,
        scope: &Track,
        tree: &TrackTree,
        multi_mic_names: &[String],
    ) -> DawResult<()> {
        let [left, right] = Self::INITIAL_CHANNELS;
        let existing: Vec<&Track> = tree
            .children_of(&scope.guid)
            .filter(|t| Self::dimension_of(t, scope) == TrackDimension::MultiMic)
            .collect();
        let (Some(first), Some(last)) = (existing.first(), existing.last()) else {
            return Ok(());
        };

        self.set_depth(&scope.guid, 1)?;
        let l = self.insert_track_at(left, first.index)?;
        self.set_depth(&l, 1)?;
        // The mics are now L's children; the last one closes L (only L —
        // the R subtree appended below is what closes `scope`).
        for (i, track) in existing.iter().enumerate() {
            let closes_l = i.saturating_add(1) == existing.len();
            self.set_depth(&track.guid, if closes_l { -1 } else { 0 })?;
        }

        // Place R explicitly rather than via `append_shape`: mid-wrap the
        // tree is deliberately not well-formed (nothing closes `scope`
        // yet), so a subtree-end walk would bail and land R *before* L.
        // Every mic shifted one position when L was inserted, so the slot
        // after the last of them is `last.index + 2`.
        let mirrored = multi_mic_names.iter().map(TrackShape::leaf).collect();
        self.insert_shape_at(
            &[TrackShape::with_children(right, mirrored)],
            last.index.saturating_add(2),
        )
    }
}

// ── shared helpers ──────────────────────────────────────────────────

impl<D: Tracks + Items + Projects> TrackManager<D> {
    /// Add a new named sibling scope under the selection, carrying a copy
    /// of the selection's channel subtree (or, failing that, its mic
    /// subtree) so the new layer/performer is shaped like the original.
    fn add_named_scope(&self, name: &str) -> DawResult<()> {
        let scope = self.selected_scope()?;
        let children = self.track_tree().shape_of_children(&scope.guid);

        let inherited: Vec<TrackShape> = [TrackDimension::Channel, TrackDimension::MultiMic]
            .into_iter()
            .find_map(|dimension| {
                let matching: Vec<TrackShape> = children
                    .iter()
                    .filter(|shape| Self::dimension_of_name(&shape.name, &scope) == dimension)
                    .cloned()
                    .collect();
                (!matching.is_empty()).then_some(matching)
            })
            .unwrap_or_default();

        if inherited.is_empty() {
            return self.append_child(&scope.guid, name);
        }
        self.append_shape(&scope.guid, &[TrackShape::with_children(name, inherited)])
    }

    /// Walk up from `track` while its ancestors are still classified as
    /// part of an instrument (guitar/keys/drums/...), stopping at the
    /// outermost one — that's the scope a new arrangement variant nests
    /// under, not whatever leaf track happened to be selected.
    fn instrument_scope(tree: &TrackTree, track: Track) -> Track {
        let mut current = track;
        while let Some(parent) = tree.parent_of(&current) {
            let child_is_dimensional =
                Self::dimension_of(&current, parent) != TrackDimension::Other;
            let parent_is_dimensional = tree
                .parent_of(parent)
                .is_some_and(|gp| Self::dimension_of(parent, gp) != TrackDimension::Other);

            if !child_is_dimensional && !parent_is_dimensional {
                break;
            }
            current = parent.clone();
        }
        current
    }

    fn reorganize_selected_by(&self, field_name: &str) -> DawResult<()> {
        let _scope = self.selected_scope()?;
        tracing::warn!(
            "[session] Reorganize selected by {field_name} is registered; hierarchy rewrite policy is pending"
        );
        Ok(())
    }

    // ── dynamic-template classification ─────────────────────────────

    /// Which metadata dimension `track`'s name reads as, interpreted in
    /// the context of the scope it sits under.
    fn dimension_of(track: &Track, scope: &Track) -> TrackDimension {
        Self::dimension_of_name(&track.name, scope)
    }

    fn dimension_of_name(name: &str, scope: &Track) -> TrackDimension {
        track_schema::classify_track_dimension(name, std::slice::from_ref(&scope.name))
    }

    /// The next unused configured value for `dimension` within `scope`
    /// (e.g. the next channel after L/R, the next mic after Amp/DI).
    fn next_value(dimension: TrackDimension, scope: &Track, taken: &[&str]) -> DawResult<String> {
        track_schema::next_configured_value(
            dimension,
            std::slice::from_ref(&scope.name),
            taken.iter().copied(),
        )
        .ok_or_else(|| DawError::not_found(&format!("configured {dimension} name"), &scope.name))
    }
}
