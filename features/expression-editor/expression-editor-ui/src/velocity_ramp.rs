//! A velocity shape you can keep adjusting after you have applied it.
//!
//! The gesture this exists for: press `v u`, see a crescendo across the
//! selection, then roll the wheel until it is as strong as the passage
//! wants. That second half is the point. A velocity preset applied at
//! full strength flattens whatever performance was underneath it, and
//! the useful setting is almost never 100% — it is however much leans on
//! the phrase without erasing it, and you cannot know that number in
//! advance. You find it by moving it.
//!
//! ## Why it holds a baseline
//!
//! Everything here is computed from the velocities as they were when the
//! ramp opened, never from the current ones. That is what makes the
//! wheel reversible: dial it to 80% and back to 20% and you get exactly
//! what 20% would have given you first time, rather than a value that
//! has been through two roundings and a clamp.
//!
//! [`expression_editor_tools::velocity::Session`] is built for exactly
//! this and already owns the baseline, so this holds one rather than
//! reimplementing the idea. What it adds is the mapping in both
//! directions — document [`NoteId`]s to the engine's opaque indices and
//! back — and the live re-apply.
//!
//! ## One undo step
//!
//! The ramp opens a gesture when it is created and re-applies with
//! `apply_live`, so a ramp adjusted twenty times is one undo. That is
//! also why it has to be *closed*: while it is live the history has an
//! open snapshot, and anything else that opens one would nest.

use expression_editor_core::Editor;
use expression_editor_core::doc::NoteId;
use expression_editor_core::edit::Edit;
use expression_editor_tools::velocity::{CurvePreset, Note as VNote, Session};

/// How far one wheel notch moves the strength.
///
/// A twentieth, so a full sweep of the dial is twenty notches — enough
/// that you can land on a value deliberately, few enough that going from
/// nothing to everything is a flick rather than a chore.
const STRENGTH_PER_NOTCH: f64 = 0.05;

/// The strength a ramp opens at.
///
/// Not 1.0. A ramp that arrives at full strength has already destroyed
/// the performance by the time you see it, and every adjustment from
/// there is damage control. Opening at two thirds means the first thing
/// you see is a real shape that still has the take underneath it, and
/// the wheel goes both ways from there.
const DEFAULT_STRENGTH: f64 = 0.66;

/// A live velocity shape over a set of notes.
#[derive(Clone)]
pub struct VelocityRamp {
    /// The engine's session, holding the baseline velocities.
    session: Session,
    /// Engine index → document id. The engine numbers notes from zero
    /// within the selection and knows nothing about `NoteId`.
    ids: Vec<NoteId>,
    preset: CurvePreset,
    strength: f64,
}

impl VelocityRamp {
    /// Open a ramp over `notes`, in the order they sound.
    ///
    /// Time order, not selection order: a crescendo is a statement about
    /// what happens *first* and what happens *last*, and the selection
    /// is a set with whatever order the clicks happened to give it. A
    /// ramp built from click order would rise and fall at random.
    ///
    /// `None` when there is nothing to shape — one note has no span for
    /// a curve to cross, so a "ramp" across it is just setting a value,
    /// which is what the drag is for.
    pub fn open(ed: &mut Editor, preset: CurvePreset, notes: &[NoteId]) -> Option<Self> {
        let mut picked: Vec<(f64, NoteId, u8)> = notes
            .iter()
            .filter_map(|id| {
                let n = ed.doc.note(*id)?;
                Some((
                    n.start,
                    *id,
                    (n.velocity * 127.0).round().clamp(1.0, 127.0) as u8,
                ))
            })
            .collect();
        if picked.len() < 2 {
            return None;
        }
        picked.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));

        let ids: Vec<NoteId> = picked.iter().map(|(_, id, _)| *id).collect();
        let engine_notes: Vec<VNote> = picked
            .iter()
            .enumerate()
            .map(|(i, (_, _, vel))| VNote::selected(i as u32, *vel))
            .collect();

        ed.begin_gesture();
        let mut ramp = Self {
            session: Session::new(engine_notes),
            ids,
            preset,
            strength: DEFAULT_STRENGTH,
        };
        ramp.session.curve = Some(preset.curve());
        ramp.apply(ed);
        Some(ramp)
    }

    pub fn preset(&self) -> CurvePreset {
        self.preset
    }

    /// Strength as a percentage, for a readout.
    pub fn percent(&self) -> u8 {
        (self.strength * 100.0).round() as u8
    }

    /// Turn the ramp over — a crescendo becomes a diminuendo.
    ///
    /// The same key that made it, pressed again. Which direction you
    /// wanted is a thing you find out by looking at it, and re-pressing
    /// is a faster answer than undoing and picking the other command.
    pub fn invert(&mut self, ed: &mut Editor) {
        self.preset = invert_preset(self.preset);
        self.session.curve = Some(self.preset.curve());
        self.apply(ed);
    }

    /// Move the strength by `notches` of the wheel.
    ///
    /// Returns whether anything changed, so a wheel event at either end
    /// of the range can fall through to scrolling the view rather than
    /// being silently swallowed by a control that is already at its
    /// limit.
    pub fn nudge(&mut self, ed: &mut Editor, notches: f64) -> bool {
        let next = (self.strength + notches * STRENGTH_PER_NOTCH).clamp(0.0, 1.0);
        if (next - self.strength).abs() < f64::EPSILON {
            return false;
        }
        self.strength = next;
        self.apply(ed);
        true
    }

    /// Write the current shape to the document, from the baseline.
    fn apply(&mut self, ed: &mut Editor) {
        let Some(curve) = self.session.curve.clone() else {
            return;
        };
        let baseline = self.session.baseline().to_vec();
        for edit in curve.apply_blended(&baseline, self.strength, self.session.range) {
            let Some(id) = self.ids.get(edit.index as usize) else {
                continue;
            };
            ed.apply_live(&Edit::SetVelocity {
                notes: vec![*id],
                velocity: edit.velocity as f64 / 127.0,
            });
        }
    }

    /// Put the velocities back as the ramp found them.
    ///
    /// Escape while a ramp is live. Restoring from the baseline rather
    /// than undoing, because the gesture is still open — an undo here
    /// would take the *previous* edit with it.
    pub fn revert(&self, ed: &mut Editor) {
        for (i, id) in self.ids.iter().enumerate() {
            let Some(was) = self.session.baseline().get(i) else {
                continue;
            };
            ed.apply_live(&Edit::SetVelocity {
                notes: vec![*id],
                velocity: was.velocity as f64 / 127.0,
            });
        }
    }
}

/// The same shape, the other way up.
///
/// Paired so that inverting twice returns you to where you started,
/// which a table mapping every rise to a plain fall would not do.
fn invert_preset(preset: CurvePreset) -> CurvePreset {
    match preset {
        CurvePreset::Rise => CurvePreset::Fall,
        CurvePreset::Fall => CurvePreset::Rise,
        CurvePreset::RiseSmooth => CurvePreset::FallSmooth,
        CurvePreset::FallSmooth => CurvePreset::RiseSmooth,
        CurvePreset::RiseS => CurvePreset::FallS,
        CurvePreset::FallS => CurvePreset::RiseS,
        CurvePreset::RiseFast => CurvePreset::FallFast,
        CurvePreset::FallFast => CurvePreset::RiseFast,
        CurvePreset::RiseSlow => CurvePreset::FallSlow,
        CurvePreset::FallSlow => CurvePreset::RiseSlow,
    }
}

// ── the one-shot operations ─────────────────────────────────────────
//
// The other three MVelocity engines, as commands. Unlike a ramp these
// leave nothing to adjust afterwards — they are a single statement about
// the selection — so they apply and close rather than staying live.
//
// Free functions taking `&mut Editor` for the same reason the ramp is a
// UI type: they need `expression-editor-tools`, and `tools` depends on
// `core`, so `core` cannot reach them without a cycle.

use expression_editor_tools::velocity::{Dynamics, Pattern, Pivot, Randomize};

/// Read the selection into the engine's shape, in time order.
///
/// Time order because every engine here is positional: an accent
/// pattern cycles through the notes, and a pattern applied in click
/// order accents whichever ones you happened to select first.
fn picked(ed: &Editor, notes: &[NoteId]) -> (Vec<NoteId>, Vec<VNote>) {
    let mut rows: Vec<(f64, NoteId, u8)> = notes
        .iter()
        .filter_map(|id| {
            let n = ed.doc.note(*id)?;
            Some((
                n.start,
                *id,
                (n.velocity * 127.0).round().clamp(1.0, 127.0) as u8,
            ))
        })
        .collect();
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));
    let ids = rows.iter().map(|(_, id, _)| *id).collect();
    let engine = rows
        .iter()
        .enumerate()
        .map(|(i, (_, _, v))| VNote::selected(i as u32, *v))
        .collect();
    (ids, engine)
}

/// Write a set of engine edits back to the document, as one undo step.
fn write_back(
    ed: &mut Editor,
    ids: &[NoteId],
    edits: Vec<expression_editor_tools::velocity::VelocityEdit>,
) -> bool {
    if edits.is_empty() {
        return false;
    }
    ed.begin_gesture();
    for edit in edits {
        let Some(id) = ids.get(edit.index as usize) else {
            continue;
        };
        ed.apply_live(&Edit::SetVelocity {
            notes: vec![*id],
            velocity: edit.velocity as f64 / 127.0,
        });
    }
    true
}

/// A repeating accent, strong on the first of every four.
///
/// The shape a straight programmed part is missing: real playing leans
/// on the downbeat, and a grid of identical velocities is the single
/// clearest tell that nobody played it.
pub fn accent(ed: &mut Editor, notes: &[NoteId]) -> bool {
    let (ids, engine) = picked(ed, notes);
    if ids.is_empty() {
        return false;
    }
    let pattern = Pattern::new([110, 78, 92, 78]);
    let range = Default::default();
    let edits = pattern.apply(&engine, 1.0, range);
    write_back(ed, &ids, edits)
}

/// Pull velocities toward their own mean, or push them away from it.
///
/// Negative compresses, positive expands. The pivot is the selection's
/// mean rather than a fixed number, so this narrows or widens *this*
/// passage's dynamic range instead of dragging it toward some absolute.
pub fn dynamics(ed: &mut Editor, notes: &[NoteId], amount: f64) -> bool {
    let (ids, engine) = picked(ed, notes);
    if ids.is_empty() {
        return false;
    }
    let dyn_ = Dynamics::new(amount, Pivot::Mean);
    let edits = dyn_.apply(&engine, Default::default());
    write_back(ed, &ids, edits)
}

/// Scatter velocities slightly, to take the machine off them.
pub fn humanise(ed: &mut Editor, notes: &[NoteId]) -> bool {
    let (ids, engine) = picked(ed, notes);
    if ids.is_empty() {
        return false;
    }
    // Seeded, not random: the same selection humanised twice gives the
    // same result, so an undo-and-retry compares two *choices* rather
    // than two rolls of the dice.
    let mut rng = Randomize::default();
    rng.roll_seeded(engine.len(), Default::default(), 0x5EED);
    let edits = rng.apply(&engine, 0.35, Default::default());
    write_back(ed, &ids, edits)
}

/// Set every selected note to the selection's own average.
///
/// The way back to a clean slate before shaping. The average rather than
/// a fixed 100, so flattening a quiet passage leaves it quiet.
pub fn flatten(ed: &mut Editor, notes: &[NoteId]) -> bool {
    let (ids, engine) = picked(ed, notes);
    if ids.is_empty() {
        return false;
    }
    let mean = engine.iter().map(|n| n.velocity as f64).sum::<f64>() / engine.len() as f64;
    let velocity = mean.round().clamp(1.0, 127.0) / 127.0;
    ed.begin_gesture();
    ed.apply_live(&Edit::SetVelocity {
        notes: ids,
        velocity,
    })
}
