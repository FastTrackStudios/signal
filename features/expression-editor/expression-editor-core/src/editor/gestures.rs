//! Editor gestures behavior.
use super::*;

impl Editor {
    /// Show a pitch drawing without recording it.
    pub fn preview_draft(&mut self, draft: &mut draft::PitchDraft) -> bool {
        let mut any = false;
        for e in draft.preview_edits() {
            any |= self.apply_live(&e);
        }
        any
    }

    /// Commit a pitch drawing as **one** step of history.
    ///
    /// Rewinds to the captured curve first, without recording, so the
    /// snapshot the history takes is the state *before* drawing began.
    /// Skipping that would snapshot the live preview instead, and undo
    /// would return to the drawing rather than to what was sung — which
    /// is the whole promise of an explicit apply.
    pub fn apply_draft(&mut self, draft: &draft::PitchDraft) -> bool {
        let Some(commit) = draft.apply_edit() else {
            return false;
        };
        if let Some(rewind) = draft.cancel_edit() {
            self.apply_live(&rewind);
        }
        self.apply(&commit)
    }

    /// Throw a pitch drawing away, restoring exactly what was captured.
    pub fn dismiss_draft(&mut self, draft: &draft::PitchDraft) -> bool {
        match draft.cancel_edit() {
            Some(e) => self.apply_live(&e),
            None => false,
        }
    }

    /// The scope handles on `note` currently address.
    ///
    /// The temporary note if one is open on *this* note, the whole note
    /// otherwise. Resolving it here rather than at each call site is
    /// what makes temporary notes free: every handle already takes a
    /// scope, so nothing else has to know the feature exists.
    /// The tool the toolbar should light up right now.
    ///
    /// The armed tool, unless the modifiers currently held would make
    /// the next drag do something else — hold Ctrl and the razor lights
    /// up, hold Alt and note-draw does, exactly as holding `z` lights up
    /// zoom. The difference is that `z` really does arm the tool and
    /// these do not: the map resolves modifiers by itself, so this only
    /// reports what the map would say.
    ///
    /// Asked of the map rather than of a modifier table, so a rebound
    /// gesture relights the right button with nothing else to change.
    pub fn shown_tool(&self) -> Tool {
        self.mouse
            .resolve(
                mouse::Context::PianoRoll,
                mouse::Gesture::Drag,
                self.held_mods,
            )
            .tool_preview()
            .unwrap_or(self.tool)
    }

    pub fn scope_for(&self, note: NoteId) -> handles::Scope {
        match self.temp_note {
            Some((id, t0, t1)) if id == note => handles::Scope::Range { t0, t1 },
            _ => handles::Scope::Note,
        }
    }

    /// Open a temporary note over `[t0, t1]` of `note`.
    ///
    /// Dragging a new range always replaces the previous one — there is
    /// only ever one, and it is discarded rather than accumulated.
    /// Ranges narrower than a pixel or two are rejected, so a stray
    /// click does not leave an invisible scope armed on the note.
    pub fn set_temp_note(&mut self, note: NoteId, t0: f64, t1: f64) -> bool {
        let Some(n) = self.doc.note(note) else {
            return false;
        };
        let scope = handles::Scope::Range { t0, t1 };
        if !scope.is_valid(n) {
            return false;
        }
        let (lo, hi) = scope.span(n);
        if (hi - lo) < self.camera.units_per_px * 3.0 {
            return false;
        }
        self.temp_note = Some((note, lo, hi));
        true
    }

    pub fn clear_temp_note(&mut self) {
        self.temp_note = None;
    }

    /// Write a handle drag at pointer height `y`.
    ///
    /// Always rebuilt from the drag's captured lanes, never from what
    /// is currently on screen — see [`handles::HandleDrag`]. Call inside
    /// a gesture opened with [`Editor::begin_gesture`]; this uses
    /// `apply_live` so the whole drag is one undo step.
    /// `snap` applies only to the coarse pitch handle. The UI resolves
    /// it as `ed.snap_pitch != mods.shift`, the same shift-reverses
    /// rule every other snap on this surface follows.
    pub fn drag_handle(&mut self, drag: &mut handles::HandleDrag, y: f64, snap: bool) -> bool {
        use handles::Handle as H;
        let amount = drag.amount(y, self.viewport.h);
        drag.applied = amount;

        let Some(note) = self.doc.note(drag.note) else {
            return false;
        };
        let (t0, t1) = drag.scope.span(note);
        if t1 <= t0 {
            return false;
        }
        let id = drag.note;

        // Restore the captured dimension first, so the edit below always
        // sees the same input it saw on the previous frame.
        let restore = |ed: &mut Self, dimension: Dimension| {
            let points = drag.base_of(dimension).points().to_vec();
            ed.apply_live(&Edit::RestoreDimension {
                note: id,
                dimension,
                t0,
                t1,
                points,
            });
        };

        match drag.handle {
            H::Pitch | H::FinePitch => {
                restore(self, Dimension::Pitch);
                // Coarse pitch snaps, fine pitch never does — that is
                // the whole distinction between the two handles. The
                // row is left alone during the drag and normalized on
                // release.
                //
                // Snapping goes through the temperament rather than
                // rounding to a semitone, so in a microtonal tuning the
                // handle lands on the tuning's degrees and not on 12-TET
                // ones that are not in the scale.
                let delta = if drag.handle == H::Pitch && snap {
                    // The note's pitch is its contour's *centre*, not
                    // its value at the midpoint — that reading carries
                    // whatever drift and vibrato are passing through,
                    // and snapping against it lands the note wherever
                    // the wobble happened to be.
                    let base = drag.base_row as f64
                        + blob::decompose(
                            drag.base_of(Dimension::Pitch),
                            t0,
                            t1,
                            edit::DEFAULT_SAMPLES,
                            self.doc.time_base.units_per_second(self.bpm),
                            Dimension::Pitch.default_value(),
                        )
                        .center;
                    self.tuning.snap(base + amount).pitch - base
                } else {
                    amount
                };
                self.apply_live(&Edit::ShiftDimension {
                    note: id,
                    dimension: Dimension::Pitch,
                    t0,
                    t1,
                    delta,
                })
            }
            H::LeftSlope | H::RightSlope => {
                restore(self, Dimension::Pitch);
                self.apply_live(&Edit::TiltDimension {
                    note: id,
                    dimension: Dimension::Pitch,
                    t0,
                    t1,
                    amount,
                    from_start: drag.handle == H::LeftSlope,
                })
            }
            H::Formant | H::Amplitude => {
                let dimension = if drag.handle == H::Formant {
                    Dimension::Timbre
                } else {
                    Dimension::Pressure
                };
                // A level, so it reads off the captured value at the
                // scope's midpoint rather than restoring and shifting.
                let mid = (t0 + t1) * 0.5;
                let base = drag
                    .base_of(dimension)
                    .sample(mid, dimension.default_value());

                // Sibilant scope: the amplitude handle addresses only
                // the unvoiced spans inside the scope. Each is written
                // separately rather than as one range, because the
                // voiced singing between them must not move.
                if drag.handle == H::Amplitude && drag.sibilants {
                    let spans: Vec<(f64, f64)> = self
                        .doc
                        .unvoiced
                        .iter()
                        .filter(|(a, b)| *b >= t0 && *a <= t1)
                        .map(|(a, b)| (a.max(t0), b.min(t1)))
                        .filter(|(a, b)| b > a)
                        .collect();
                    // A hairline either side of each span, holding the
                    // level the note already had.
                    //
                    // Without these the edit leaks: a curve holds its
                    // endpoint value outside the authored range, so on
                    // a note whose Pressure was never authored, writing
                    // only the consonant would raise the *whole* note
                    // to that level — the singing included, which is
                    // precisely what this scope exists to avoid.
                    let eps = (t1 - t0) * 1e-3;
                    let mut any = false;
                    for (a, b) in spans {
                        // Restore first: the level is absolute, so a
                        // span already written this frame has to go
                        // back before it is written again.
                        let points = drag.base_of(dimension).points().to_vec();
                        self.apply_live(&Edit::RestoreDimension {
                            note: id,
                            dimension,
                            t0: a,
                            t1: b,
                            points,
                        });
                        for (g0, g1) in [(a - eps * 2.0, a - eps), (b + eps, b + eps * 2.0)] {
                            if g0 > t0 && g1 < t1 {
                                let held = drag
                                    .base_of(dimension)
                                    .sample(g0, dimension.default_value());
                                self.apply_live(&Edit::SetDimensionLevel {
                                    note: id,
                                    dimension,
                                    t0: g0,
                                    t1: g1,
                                    value: held,
                                });
                            }
                        }
                        any |= self.apply_live(&Edit::SetDimensionLevel {
                            note: id,
                            dimension,
                            t0: a,
                            t1: b,
                            value: base + amount,
                        });
                    }
                    return any;
                }

                self.apply_live(&Edit::SetDimensionLevel {
                    note: id,
                    dimension,
                    t0,
                    t1,
                    value: base + amount,
                })
            }
            H::Vibrato => {
                restore(self, Dimension::Pitch);
                // 1.0 is as sung; 0 is robotic; above 1 exaggerates.
                // Drift is held at full so the vibrato handle changes
                // only the vibrato, which is what it says it does.
                self.apply_live(&Edit::ReblendPitch {
                    note: id,
                    t0,
                    t1,
                    drift_amount: 1.0,
                    modulation_amount: (1.0 + amount).max(0.0),
                })
            }
        }
    }

    /// Finish a handle drag.
    ///
    /// Folds whole semitones back into the row, restoring the invariant
    /// the surface depends on. Only the pitch handles can break it, so
    /// only they pay for it.
    pub fn end_handle_drag(&mut self, drag: &handles::HandleDrag) -> bool {
        use handles::Handle as H;
        if !matches!(drag.handle, H::Pitch | H::FinePitch) {
            return false;
        }
        self.apply_live(&Edit::NormalizeRow {
            notes: vec![drag.note],
        })
    }
}
