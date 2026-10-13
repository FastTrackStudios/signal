//! Editor instruments behavior.
use super::*;

impl Editor {
    /// Step the selected drum hits through the flam cycle.
    ///
    /// **none → before → after → none.** One key, and the state lives on
    /// the note rather than in a mode: you can look at a hit and know
    /// what the next press will do, which a global before/after flag
    /// cannot give you — there, the same key means different things
    /// depending on what you last did to some other note.
    ///
    /// The third press removing the flam is what makes it a cycle rather
    /// than a trap. Without it, changing your mind means reaching for
    /// undo or hunting the grace note down by hand.
    ///
    /// Silent about pieces that cannot flam — a hi-hat in the selection
    /// is skipped rather than refusing the whole gesture.
    pub fn flam_selection(&mut self) -> usize {
        let RowSpace::Drums(map) = self.row_space.clone() else {
            return 0;
        };
        let ids: Vec<doc::NoteId> = self.selection.notes.clone();

        let mut made = 0;
        for id in ids {
            match flam::flam(&self.doc, &map, id, flam::DEFAULT_FLAM_MS, self.bpm) {
                Ok(edit) => {
                    self.apply(&edit);
                    made += 1;
                }
                Err(_) => continue,
            }
        }

        if made > 0 {
            // Splitting the affected pieces is what makes the flam
            // *visible*: a grace note on a hidden row is a note you
            // cannot see or select.
            self.show_hands_for_selection(&map);
        }
        made
    }

    /// What the next press of the flam key will do to a hit, for a UI
    /// that wants to say so before you press it.
    pub fn flam_step(&self, id: doc::NoteId) -> Option<flam::FlamStep> {
        let RowSpace::Drums(map) = &self.row_space else {
            return None;
        };
        flam::next_step(&self.doc, map, id, flam::DEFAULT_FLAM_MS, self.bpm).ok()
    }

    /// Open both hands for every two-handed piece in the selection.
    pub(super) fn show_hands_for_selection(&mut self, map: &rows::DrumMap) {
        let rows: Vec<usize> = self
            .selection
            .notes
            .iter()
            .filter_map(|id| self.doc.note(*id))
            .map(|n| n.row.max(0) as usize)
            .collect();
        for row in rows {
            if let Some(other) = map.other_hand_row(row) {
                for r in [row, other] {
                    if !self.split_pieces.contains(&r) {
                        self.split_pieces.push(r);
                    }
                }
            }
        }
        self.split_pieces.sort_unstable();
        self.split_pieces.dedup();
    }

    /// Move the selected hits to a hand.
    ///
    /// The sticking control: a part that says which hand plays what —
    /// notated drum music, or a groove you want to be playable — needs
    /// this to be one action rather than dragging notes between rows and
    /// hoping you picked the right one.
    ///
    /// Opens the piece, because a note that moved to a row you cannot
    /// see has vanished as far as the user is concerned.
    ///
    /// Returns how many moved. Hits on one-handed pieces are skipped
    /// rather than refusing the whole gesture.
    pub fn set_hand_of_selection(&mut self, hand: rows::Hand) -> usize {
        let RowSpace::Drums(map) = self.row_space.clone() else {
            return 0;
        };
        let ids: Vec<doc::NoteId> = self.selection.notes.clone();
        let mut moved = 0;

        for id in ids {
            let Some(note) = self.doc.note(id) else {
                continue;
            };
            let row = note.row.max(0) as usize;
            let Some(target) = map.row_for_hand(row, hand) else {
                continue;
            };
            if target == row {
                continue;
            }
            // Transpose rather than a bare row assignment: on a drum
            // roll a row *is* the pitch, and this is the edit that
            // carries a note's owned expression with it.
            self.apply(&Edit::Transpose {
                notes: vec![id],
                semitones: target as i32 - row as i32,
            });
            moved += 1;
        }

        if moved > 0 {
            self.show_hands_for_selection(&map);
        }
        moved
    }

    /// The header for a drum row, knowing which pieces are open.
    ///
    /// A collapsed piece shows its own name — `T1` — and an open one
    /// shows the hand, `L` or `R`, under it.
    pub fn row_header(&self, row: i32) -> String {
        match &self.row_space {
            RowSpace::Drums(m) => {
                let r = row.max(0) as usize;
                m.display_name(r, self.split_pieces.contains(&r))
            }
            other => other.row_label(row),
        }
    }

    /// The piece name for a split row, so a renderer can bracket the
    /// two hands together.
    pub fn row_group(&self, row: i32) -> Option<String> {
        let RowSpace::Drums(m) = &self.row_space else {
            return None;
        };
        let r = row.max(0) as usize;
        if !self.split_pieces.contains(&r) {
            return None;
        }
        m.group_name(r).map(str::to_string)
    }

    /// Commit a typed syllable and close the field.
    ///
    /// An empty string clears the lyric rather than storing one, so
    /// deleting a syllable is the same gesture as typing one.
    pub fn set_lyric(&mut self, id: doc::NoteId, text: &str) -> bool {
        self.editing_lyric = None;
        self.apply(&Edit::SetText {
            note: id,
            text: (!text.trim().is_empty()).then(|| text.trim().to_string()),
        })
    }

    /// Abandon the lyric field without writing anything.
    pub fn cancel_lyric(&mut self) {
        self.editing_lyric = None;
    }

    /// Put the selection on `fret`, keeping each note's string.
    ///
    /// The fret is derived, so this transposes — it is the "slide the
    /// shape up the neck" gesture, not a relabelling.
    pub fn set_fret_of_selection(&mut self, fret: u8) -> bool {
        let notes: Vec<doc::NoteId> = self.selection.notes.to_vec();
        if notes.is_empty() {
            return false;
        }
        self.apply(&Edit::SetFret { notes, fret })
    }

    /// Move one band split and re-sort every slice into its new band.
    pub fn move_band_split(&mut self, index: usize, hz: f64) -> bool {
        // The document's row space is the authority — every edit reads
        // that one, and `Editor::row_space` is a view of it that is not
        // populated until a mode is applied.
        let RowSpace::Bands(bands) = &self.doc.row_space else {
            return false;
        };
        let mut bands = bands.clone();
        if index >= bands.splits.len() {
            return false;
        }
        // Splits are ascending by contract, and the caller addresses one
        // by index — the inspector's -/+ buttons key on the render-time
        // position. Re-sorting after a write would hand the next click a
        // different split, so a split is instead penned between its
        // neighbours: crossing one has no meaning, since the bands *are*
        // the ascending boundaries.
        let lower = if index == 0 {
            f64::NEG_INFINITY
        } else {
            bands.splits[index - 1]
        };
        let upper = bands
            .splits
            .get(index + 1)
            .copied()
            .unwrap_or(f64::INFINITY);
        bands.splits[index] = hz.clamp(lower, upper);
        let ok = self.apply(&Edit::SetBands {
            bands: bands.clone(),
        });
        if ok {
            self.row_space = RowSpace::Bands(bands);
        }
        ok
    }

    /// Which hand a hit is played with, when its piece has two.
    pub fn hand_of_note(&self, id: doc::NoteId) -> Option<rows::Hand> {
        let RowSpace::Drums(map) = &self.row_space else {
            return None;
        };
        let row = self.doc.note(id)?.row.max(0) as usize;
        map.hand_of(row)
    }

    /// Show or collapse a two-handed piece.
    pub fn toggle_piece_split(&mut self, row: usize) {
        let RowSpace::Drums(map) = self.row_space.clone() else {
            return;
        };
        let Some(other) = map.other_hand_row(row) else {
            return;
        };
        if self.split_pieces.contains(&row) {
            self.split_pieces.retain(|r| *r != row && *r != other);
        } else {
            self.split_pieces.push(row);
            self.split_pieces.push(other);
            self.split_pieces.sort_unstable();
            self.split_pieces.dedup();
        }
        self.refresh_fold();
    }

    // ── the chord gun ────────────────────────────────────────────────

    /// Fire the chord on `degree` (1..=7) into the document.
    ///
    /// At the play cursor when there is one, else at the left edge of
    /// the view — the same rule paste follows, and for the same reason:
    /// dropping a chord at zero puts it off screen and looks like
    /// nothing happened.
    ///
    /// One grid division long, because that is the length every other
    /// insert on this surface uses and a chord you have to resize is a
    /// chord you did not want fired.
    ///
    /// The new notes become the selection, so the gesture composes: fire
    /// a chord, then `v u` to ramp it, or drag it somewhere else. And it
    /// is one undo step, however many notes the depth produced.
    pub fn insert_chord(&mut self, degree: usize) -> bool {
        let pitches = self.chord_gun.pitches(degree);
        if pitches.is_empty() {
            return false;
        }
        let start = self
            .playhead
            .unwrap_or_else(|| self.camera.t_at(0.0))
            .max(self.doc.start);
        let start = self.snap_time(start);
        let len = self.grid.step(self.units_per_beat());
        // A free grid has no step to take a length from; a beat is the
        // honest default rather than a zero-length note.
        let len = if len > 0.0 {
            len
        } else {
            self.units_per_beat()
        };

        self.begin_gesture();
        let mut ids = Vec::with_capacity(pitches.len());
        for pitch in pitches {
            let id = self.doc.mint_id();
            let mut note = doc::Note::new(id, start, start + len, pitch);
            note.velocity = 100.0 / 127.0;
            self.apply_live(&Edit::AddNote(Box::new(note)));
            ids.push(id);
        }
        self.selection.notes = ids;
        true
    }

    /// Switch mode, re-applying its preset.
    ///
    /// A preset, not a lock: everything it sets can be changed
    /// afterwards. Switching back re-applies.
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        // The track owns the mode; `Editor::mode` is the active one's.
        // Writing both keeps a stack drawn from the workspace agreeing
        // with the surface the user is looking at.
        if let Some(track) = self.tracks.track_mut(self.tracks.active()) {
            track.set_mode(mode);
        }
        // A mode preset supplies a row space, but never overwrites one
        // of the same kind: the document's own may have been tuned —
        // band splits moved, a guitar retuned — and re-applying a preset
        // (which switching tracks does) must not silently undo that.
        if self.doc.row_space.same_kind(&mode.default_row_space()) {
            self.row_space = self.doc.row_space.clone();
        } else {
            self.row_space = mode.default_row_space();
            self.doc.row_space = self.row_space.clone();
        }
        // A new row space means a new fold; drums fold, nothing else
        // does, and switching between them must not leave the old one
        // applied to the new rows.
        self.split_pieces.clear();
        self.refresh_fold();
        self.mouse = mode.default_mouse();
        self.overlays = mode.default_overlays();
        self.strip_lane = mode.default_strip();
        if !mode.has_expression_lanes() {
            // Leaving the active dimension on Pressure in plain MIDI would
            // point every gesture at something the format cannot carry.
            self.dimension = Dimension::Pitch;
        }
        self.reset_view();
    }

    /// Whether CC edit mode is on.
    pub fn cc_editing(&self) -> bool {
        self.cc_edit.is_some()
    }

    /// Enter CC edit mode on a controller, pinning it if it was not
    /// already visible — editing something invisible is a trap.
    pub fn edit_cc(&mut self, number: u8) {
        self.doc.cc.ensure(number);
        if let Some(l) = self.doc.cc.get_mut(number) {
            l.pinned = true;
        }
        self.cc_edit = Some(number);
    }

    pub fn exit_cc_edit(&mut self) {
        self.cc_edit = None;
    }

    /// Sounding pitches of the selected notes — what the chord box
    /// reads.
    ///
    /// Falls back to the notes under the playhead when nothing is
    /// selected, so the box says something useful while you navigate
    /// rather than going blank.
    pub fn chord_pitches(&self) -> Vec<i32> {
        let space = &self.row_space;
        if !self.selection.notes.is_empty() {
            let mut v: Vec<i32> = self
                .selection
                .notes
                .iter()
                .filter_map(|id| self.doc.note(*id))
                .map(|n| space.pitch_of(n))
                .collect();
            v.sort_unstable();
            v.dedup();
            return v;
        }
        let Some(t) = self.playhead else {
            return Vec::new();
        };
        let mut v: Vec<i32> = self
            .doc
            .notes
            .iter()
            .filter(|n| n.start <= t && n.end > t && !n.muted)
            .map(|n| space.pitch_of(n))
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// The chord the box shows, if the current pitches form one.
    pub fn current_chord(&self) -> Option<Chord> {
        chord::identify(&self.chord_pitches())
    }
}
