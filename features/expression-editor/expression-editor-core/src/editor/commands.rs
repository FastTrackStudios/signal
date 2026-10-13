//! Editor commands behavior.
use super::*;

impl Editor {
    /// Notes a discrete command acts on.
    ///
    /// A right-click on an unselected note targets *that* note. Menus
    /// that quietly act on a selection somewhere else on screen are how
    /// the wrong bar gets deleted.
    pub fn command_targets(&self, under: Option<NoteId>) -> Vec<NoteId> {
        match under {
            Some(id) if !self.selection.notes.contains(&id) => vec![id],
            Some(id) => {
                if self.selection.notes.is_empty() {
                    vec![id]
                } else {
                    self.selection.notes.clone()
                }
            }
            None => self.selection.notes.clone(),
        }
    }

    /// Note ids inside the bar containing `t`.
    pub fn notes_in_measure(&self, t: f64) -> Vec<NoteId> {
        let bar = self.units_per_bar();
        let i = ((t - self.doc.start) / bar).floor();
        let (lo, hi) = (self.doc.start + i * bar, self.doc.start + (i + 1.0) * bar);
        self.doc
            .notes
            .iter()
            .filter(|n| n.start < hi && n.end > lo)
            .map(|n| n.id)
            .collect()
    }

    /// Run a context-menu command.
    ///
    /// Commands the core cannot complete on its own — the ones that
    /// need a text field or a submenu — return `false` so the UI knows
    /// to open something rather than assuming the edit happened.
    pub fn run_command(&mut self, cmd: &menu::Command, under: Option<NoteId>) -> bool {
        use menu::Command as C;

        // The razor verbs, which act on the areas rather than on a
        // selection — so they are answered before `command_targets`
        // works out what a note command would apply to.
        //
        // These call the same methods the keyboard does. A menu that
        // reimplemented them would be a second definition of "reverse",
        // and the two would differ the first time either changed.
        match cmd {
            C::RazorReverse => return self.razor_reverse(),
            C::RazorReversePitches => return self.razor_reverse_pitches(),
            C::RazorInvert => return self.razor_invert(),
            C::RazorDeleteContents => return self.razor_delete_contents(),
            C::RazorDuplicate => return self.razor_duplicate(),
            C::RazorSelectContents => return self.razor_select_contents(),
            C::RazorSplit => return self.razor_split(),
            C::RazorClearLane => return self.razor_clear_lane(),
            C::RazorFullLane => return self.razor_full_lane(),
            // The factor is carried as a small integer because `Command`
            // derives `PartialEq` and a menu built twice has to compare
            // equal — two `f64`s that went through different arithmetic
            // do not reliably do that.
            C::RazorScale(n) => {
                let factor = if *n <= 1 { 0.5 } else { *n as f64 };
                return self.razor_scale(factor);
            }
            C::RazorClear => {
                let had = !self.razor.is_empty();
                self.razor.clear();
                return had;
            }
            _ => {}
        }

        let targets = self.command_targets(under);
        match cmd {
            C::Copy => self.clipboard.copy_from(&self.doc, &targets),
            C::Cut => {
                if !self.clipboard.copy_from(&self.doc, &targets) {
                    return false;
                }
                self.apply(&Edit::DeleteNotes(targets))
            }
            C::Paste => {
                // Paste lands at the playhead when there is one, and
                // back where it came from otherwise — never silently at
                // zero, which puts the phrase off-screen.
                let t = self.playhead.unwrap_or(self.doc.start);
                let notes = self.clipboard.placed(t, self.clipboard.origin_row());
                self.apply(&Edit::PasteNotes(notes))
            }
            C::Delete => self.apply(&Edit::DeleteNotes(targets)),
            C::SelectAll => {
                self.selection.notes = self.doc.notes.iter().map(|n| n.id).collect();
                true
            }
            C::SelectMeasure => {
                let t = self
                    .playhead
                    .or_else(|| {
                        targets
                            .first()
                            .and_then(|id| self.doc.note(*id))
                            .map(|n| n.start)
                    })
                    .unwrap_or(self.doc.start);
                self.selection.notes = self.notes_in_measure(t);
                !self.selection.notes.is_empty()
            }
            C::CopyMeasure => {
                let t = self
                    .playhead
                    .or_else(|| {
                        targets
                            .first()
                            .and_then(|id| self.doc.note(*id))
                            .map(|n| n.start)
                    })
                    .unwrap_or(self.doc.start);
                let ids = self.notes_in_measure(t);
                self.clipboard.copy_from(&self.doc, &ids)
            }
            C::ClearExpression => {
                let dimension = self.dimension;
                let spans: Vec<(NoteId, f64, f64)> = targets
                    .iter()
                    .filter_map(|id| self.doc.note(*id).map(|n| (*id, n.start, n.end)))
                    .collect();
                let mut any = false;
                for (id, t0, t1) in spans {
                    any |= self.apply(&Edit::EraseDimension {
                        note: id,
                        dimension,
                        t0,
                        t1,
                    });
                }
                any
            }
            C::ToggleMute => self.apply(&Edit::ToggleMuted { notes: targets }),
            C::AssignChannels => self.apply(&Edit::AssignChannels {
                notes: targets,
                seed: 0,
            }),
            C::CycleString(id) => {
                let Some(n) = self.doc.note(*id) else {
                    return false;
                };
                let RowSpace::Strings(tuning) = self.doc.row_space.clone() else {
                    return false;
                };
                // The string is on the note; the row is the pitch. Using
                // the row here cycled to a "string" that was a MIDI
                // pitch number.
                //
                // Cycling walks the strings that can actually *play* this
                // pitch, and wraps. A plain `+ 1` clamped at the top into
                // a no-op that still reported success, and dead-ended
                // mid-neck the moment the next string could not reach the
                // note (the A string's 2nd fret is fret -3 on the D).
                let count = tuning.strings();
                if count == 0 {
                    return false;
                }
                let current = n.string;
                let pitch = n.row;
                let reachable = |s: usize| {
                    let fret = pitch - tuning.open(s);
                    (0..=tuning.frets as i32).contains(&fret)
                };
                // Start past the current string, or at the lowest for a
                // note that carries none yet, then wrap once.
                let start = current.map(|s| s as usize + 1).unwrap_or(0);
                let Some(next) = (0..count)
                    .map(|i| (start + i) % count)
                    .find(|&s| Some(s as u8) != current && reachable(s))
                else {
                    // No other string reaches this pitch — the note is
                    // playable in one place, so there is nothing to
                    // cycle to and nothing to undo.
                    return false;
                };
                self.apply(&Edit::SetString {
                    note: *id,
                    string: next as i32,
                })
            }
            C::ToggleLegato(id) => {
                let Some(n) = self.doc.note(*id) else {
                    return false;
                };
                let gap = if n.legato { 0.0 } else { 1.0 };
                self.apply(&Edit::Legato {
                    notes: vec![*id],
                    gap,
                })
            }
            C::SplitNote(id, t) => self.apply(&Edit::SplitNote { note: *id, t: *t }),
            C::MergeNotes(id) => self.merge_with_next(*id),
            // `EditLyric` opens the field rather than applying: the
            // text arrives later, through `set_lyric`.
            C::EditLyric(id) => {
                self.editing_lyric = Some(*id);
                true
            }
            // These still need UI: a submenu, a panel.
            C::SetArticulation(_) | C::Properties => false,
            // Answered above, before the targets are worked out. Listed
            // rather than swept up by a `_`, so a *new* note command
            // still fails to compile until it has an arm.
            C::RazorReverse
            | C::RazorReversePitches
            | C::RazorInvert
            | C::RazorDeleteContents
            | C::RazorDuplicate
            | C::RazorSelectContents
            | C::RazorSplit
            | C::RazorClearLane
            | C::RazorFullLane
            | C::RazorScale(_)
            | C::RazorClear => false,
        }
    }

    /// Absorb the next note on the same row into `id`.
    ///
    /// The audio editor's note-assignment merge, and the same operation
    /// a MIDI editor wants for a note split by mistake. The survivor
    /// keeps its own expression and simply extends — re-deriving a
    /// merged curve from two would discard whichever was edited.
    pub(super) fn merge_with_next(&mut self, id: NoteId) -> bool {
        let Some(n) = self.doc.note(id) else {
            return false;
        };
        let (row, end) = (n.row, n.end);
        let next = self
            .doc
            .notes
            .iter()
            .filter(|o| o.row == row && o.start >= end && o.id != id)
            .min_by(|a, b| a.start.total_cmp(&b.start))
            .map(|o| (o.id, o.end));
        let Some((next_id, next_end)) = next else {
            return false;
        };
        let Some(n) = self.doc.note(id) else {
            return false;
        };
        let start = n.start;
        self.apply(&Edit::Resize {
            note: id,
            start,
            end: next_end,
        }) && self.apply(&Edit::DeleteNotes(vec![next_id]))
    }
}
