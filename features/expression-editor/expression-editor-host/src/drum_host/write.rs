//! Resolve the live kit and preflight writes before entering an undo block.
use super::{DrumDaw, DrumHost, QuantizePanel, WriteMode, apply_warp, panel_bridge, stretch_hit};
use daw::service::{Item, ItemRef, SourceType, TakeRef, TrackRef};
use expression_editor_audio::apply_quantize::{Applied, GroupError, apply_split};
use expression_editor_audio::quantize::{Piece, SplitConfig};

struct ItemSplit {
    item: ItemRef,
    track_guid: String,
    pieces: Vec<Piece>,
}

impl<D: DrumDaw> DrumHost<D> {
    /// Anchors identify tracks, not the complete edit scope. Resolve all
    /// current playing pieces on those tracks on every operation.
    fn edit_members(&self) -> Result<Vec<Item>, GroupError> {
        let tracks = daw::service::Tracks::all(&self.daw, self.ctx.clone());
        let mut seen = std::collections::HashSet::new();
        let mut members = Vec::new();
        for anchor in self.group() {
            let item = self
                .daw
                .get_item(self.ctx.clone(), anchor)
                .ok_or(GroupError::Missing)?;
            if !seen.insert(item.track_guid.clone()) {
                continue;
            }
            let track = tracks
                .iter()
                .find(|t| t.guid == item.track_guid)
                .ok_or(GroupError::Missing)?;
            let previous_members = members.len();
            for item in self
                .daw
                .get_items(self.ctx.clone(), TrackRef::Guid(track.guid.clone()))
            {
                if !crate::audio_read::is_playing(track, &item) {
                    continue;
                }
                let Some(take) = self
                    .daw
                    .get_active_take(self.ctx.clone(), ItemRef::Guid(item.guid.clone()))
                else {
                    continue;
                };
                if take.source_type != SourceType::Audio {
                    continue;
                }
                if item.locked {
                    return Err(GroupError::Unsupported {
                        reason: "A kit item is locked",
                    });
                }
                if (take.play_rate - 1.0).abs() > 1e-9 {
                    return Err(GroupError::Unsupported {
                        reason: "Render or normalize take playback rates before editing this kit",
                    });
                }
                if !self
                    .daw
                    .get_stretch_markers(
                        self.ctx.clone(),
                        ItemRef::Guid(item.guid.clone()),
                        TakeRef::Active,
                    )
                    .is_empty()
                {
                    return Err(GroupError::Unsupported {
                        reason: "Existing stretch maps must be rendered before further kit edits",
                    });
                }
                members.push(item);
            }
            if members.len() == previous_members {
                return Err(GroupError::Missing);
            }
        }
        if members.is_empty() {
            return Err(GroupError::Empty);
        }
        Ok(members)
    }

    /// Intersect a project-time split plan with each current item. The
    /// resulting cuts are item-local, which is what the audio writer accepts.
    pub(super) fn write_splits(
        &self,
        pieces: &[Piece],
        cfg: SplitConfig,
        label: &str,
    ) -> Result<Applied, GroupError> {
        if pieces.is_empty() {
            return Ok(Applied::default());
        }
        let members = self.edit_members()?;
        let mut writes = Vec::new();
        for item in members {
            let start = item.position.as_seconds();
            let length = item.length.as_seconds();
            let local = intersect(pieces, start, length);
            if local.is_empty() || unchanged(&local, length) {
                continue;
            }
            writes.push(ItemSplit {
                item: ItemRef::Guid(item.guid),
                track_guid: item.track_guid,
                pieces: local,
            });
        }
        if writes.is_empty() {
            return Ok(Applied::default());
        }
        let tracks = writes
            .iter()
            .map(|write| &write.track_guid)
            .collect::<std::collections::HashSet<_>>()
            .len();
        self.daw.begin_undo_block(self.ctx.clone(), label);
        let result = writes
            .into_iter()
            .try_fold(Applied::default(), |mut total, write| {
                let done = apply_split(
                    &self.daw,
                    self.ctx.clone(),
                    &[write.item],
                    &write.pieces,
                    cfg,
                )?;
                total.items += done.items;
                total.pieces += done.pieces;
                Ok(total)
            });
        self.daw.end_undo_block(self.ctx.clone(), label, None);
        result.map(|mut done| {
            done.items = tracks;
            done
        })
    }

    /// The existing warp writer replaces an item's marker map. Until maps
    /// compose, only an unsliced, unwarped common take can use that writer.
    pub(super) fn warp_group(&self) -> Result<Vec<ItemRef>, GroupError> {
        let members = self.edit_members()?;
        let mut tracks = std::collections::HashSet::new();
        for item in &members {
            if !tracks.insert(&item.track_guid)
                || item.position.as_seconds().abs() > 1e-6
                || (item.length.as_seconds() - self.take_secs).abs() > 1e-6
            {
                return Err(GroupError::Unsupported {
                    reason: "Warp requires one full-length item per mic; use Split for comped or sliced takes",
                });
            }
        }
        Ok(members
            .into_iter()
            .map(|item| ItemRef::Guid(item.guid))
            .collect())
    }
}

fn intersect(pieces: &[Piece], start: f64, length: f64) -> Vec<Piece> {
    pieces
        .iter()
        .filter_map(|piece| {
            let cut = piece.cut.max(start);
            let end = piece.end.min(start + length);
            (end > cut).then_some(Piece {
                cut: cut - start,
                end: end - start,
                shift: piece.shift,
                transient: piece.transient.map(|t| t - start),
            })
        })
        .collect()
}

fn unchanged(pieces: &[Piece], length: f64) -> bool {
    matches!(pieces, [p] if p.cut.abs() < 1e-9 && (p.end - length).abs() < 1e-9 && p.shift.abs() < 1e-9)
}

impl<D: DrumDaw> DrumHost<D> {
    /// Cut every mic in the kit at `at` seconds.
    ///
    /// One undo step and one cut time across the whole group: mics cut
    /// at different places stop being phase-coherent, and a kit that has
    /// lost phase coherence cannot be repaired by hand.
    // r[impl drums.manual.split]
    pub fn split(&self, at: f64, cfg: SplitConfig) -> Result<Applied, GroupError> {
        let pieces = expression_editor_audio::slip::split_pieces(at, self.take_secs, cfg);
        self.write_splits(&pieces, cfg, "Split kit")
    }

    /// Write the panel's plan to the whole kit, one undo step.
    // r[impl drums.quantize.apply]
    pub fn apply(&self, panel: &QuantizePanel) -> Result<Applied, GroupError> {
        let hits = self.quantizable(panel);
        let (plan, _) = panel_bridge::preview_hits(&hits, &self.target_of(panel));
        match panel.mode {
            WriteMode::Split => {
                let cfg = SplitConfig {
                    leading_pad_secs: panel.pad,
                    crossfade_secs: panel.crossfade,
                };
                self.write_splits(&plan.splits(self.take_secs, cfg), cfg, "Quantize kit")
            }
            WriteMode::Warp => {
                let items = self.warp_group()?;
                let frames = (self.take_secs * self.sample_rate).ceil() as usize;
                let Some(alignment) = plan.alignment(frames, self.sample_rate) else {
                    return Ok(Applied::default());
                };
                self.daw.begin_undo_block(self.ctx.clone(), "Quantize kit");
                let out = apply_warp(&self.daw, self.ctx.clone(), &items, &alignment);
                self.daw
                    .end_undo_block(self.ctx.clone(), "Quantize kit", None);
                out
            }
        }
    }

    /// Slip one hit across the whole kit, one undo step.
    // r[impl drums.manual.slip]
    pub fn slip(
        &self,
        hit: f64,
        next: f64,
        delta: f64,
        cfg: SplitConfig,
    ) -> Result<Applied, GroupError> {
        let pieces =
            expression_editor_audio::slip::slip_pieces(hit, next, self.take_secs, delta, cfg);
        self.write_splits(&pieces, cfg, "Slip hit")
    }

    /// Stretch one hit across the whole kit, one undo step — the WARP
    /// twin of [`DrumHost::slip`]. `both` is the BothStretch law: the
    /// take's ends pin instead of the neighbours.
    // r[impl drums.manual.stretch]
    pub fn stretch(
        &self,
        hit: f64,
        prev: f64,
        next: f64,
        delta: f64,
        both: bool,
    ) -> Result<Applied, GroupError> {
        let items = self.warp_group()?;
        self.daw.begin_undo_block(self.ctx.clone(), "Stretch hit");
        let out = stretch_hit(
            &self.daw,
            self.ctx.clone(),
            &items,
            hit,
            prev,
            next,
            self.take_secs,
            delta,
            both,
            self.sample_rate,
        );
        self.daw
            .end_undo_block(self.ctx.clone(), "Stretch hit", None);
        out
    }

    /// One undo step back — the whole last gesture.
    pub fn undo(&self) -> bool {
        self.daw.undo(self.ctx.clone())
    }

    /// Redo the host's last undone operation, in the same history domain.
    pub fn redo(&self) -> bool {
        self.daw.redo(self.ctx.clone())
    }
}
