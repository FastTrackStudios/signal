//! Where a velocity edit goes.
//!
//! The panel knows what shape you dialled in; it deliberately does not
//! know how to reach a DAW. That's this seam.
//!
//! The trait lives here rather than in the UI crate — as chord-tool has
//! it — so that `midi-tools-ui` and `midi-tools-daw` are siblings that
//! both depend on this crate and not on each other. A DAW backend has no
//! business linking Dioxus to find out what a sink is.

use crate::velocity::Session;

/// A take that velocity edits can be read from and written to.
///
/// `'static` only — deliberately not `Send + Sync`. A sink is driven by
/// the panel, the panel runs on the UI thread, and nothing here ever
/// crosses one. The bounds were a guess at a requirement that never
/// arrived, and they cost something real: the editor's own sink reads
/// its selection out of a dioxus `Signal`, which is not `Sync`, so the
/// one surface most obviously entitled to a velocity panel was the one
/// that could not have one.
///
/// A backend that genuinely does cross threads is free to be `Send` on
/// its own account; requiring it of every implementor to serve a case
/// that does not exist is the wrong way round.
pub trait VelocitySink: 'static {
    /// Bind to the user's current target and read its notes into a fresh
    /// session.
    ///
    /// Returns a message on failure rather than an error type: the only
    /// consumer is a status line, and a panel should say "select a MIDI
    /// item first" rather than swallow it.
    fn open(&self) -> Result<Session, String>;

    /// Push the session's result to the take. Returns how many notes moved.
    fn commit(&self, session: &Session) -> Result<usize, String>;

    /// Put the take back exactly as [`VelocitySink::open`] found it.
    fn revert(&self, session: &Session) -> Result<usize, String>;

    /// Re-read the bound take into `session`, keeping its parameters.
    fn resync(&self, session: &mut Session) -> Result<(), String>;
}

/// The no-DAW default: reports what *would* happen.
///
/// Used by the standalone example, and by the panel when no sink is
/// provided. It hands back a synthetic take so the panel has something
/// to shape — a velocity tool with no notes in it is impossible to
/// iterate on, and every control would be greyed out.
pub struct DemoSink {
    notes: Vec<crate::velocity::Note>,
}

impl Default for DemoSink {
    fn default() -> Self {
        Self::sixteenths(32)
    }
}

impl DemoSink {
    /// `count` notes with a plausible played-hi-hat velocity shape:
    /// accented downbeats, a little drift, nothing perfectly even.
    pub fn sixteenths(count: usize) -> Self {
        let notes = (0..count)
            .map(|i| {
                let accent = match i % 4 {
                    0 => 104,
                    2 => 88,
                    _ => 68,
                };
                // A slow sway, so the demo take isn't a repeating
                // sawtooth that makes every engine look like it works.
                let sway = ((i as f64) * 0.4).sin() * 9.0;
                crate::velocity::Note::new(i as u32, (f64::from(accent) + sway).round() as u8)
            })
            .collect();
        Self { notes }
    }
}

impl VelocitySink for DemoSink {
    fn open(&self) -> Result<Session, String> {
        Ok(Session::new(self.notes.clone()))
    }

    fn commit(&self, session: &Session) -> Result<usize, String> {
        let n = session.edits().len();
        tracing::debug!(notes = n, "commit (no DAW attached)");
        Err(format!("no DAW attached — would write {n} notes"))
    }

    fn revert(&self, session: &Session) -> Result<usize, String> {
        Err(format!(
            "no DAW attached — would restore {} notes",
            session.baseline().len()
        ))
    }

    fn resync(&self, _session: &mut Session) -> Result<(), String> {
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────
// Arpeggiator
// ─────────────────────────────────────────────────────────────────────

/// A take an arpeggiator can read chords from and write an arp back to.
///
/// Separate from [`VelocitySink`] rather than one big `MidiToolsSink`:
/// the two tools need different things from a backend (velocity needs
/// only note *reads* and velocity *writes*; the arp needs deletes and
/// raw-PPQ inserts), and a panel that only shapes velocity shouldn't
/// have to be handed something that can delete notes.
pub trait ArpSink: Send + Sync + 'static {
    /// Read the target's notes and group them into chords.
    fn open(&self) -> Result<crate::arp::ArpSession, String>;

    /// Replace the source chords with the arpeggio. Returns how many
    /// notes were written.
    fn commit(&self, session: &crate::arp::ArpSession) -> Result<usize, String>;
}

/// The no-DAW default: a two-chord progression to arpeggiate.
pub struct DemoArpSink {
    chords: Vec<crate::arp::Chord>,
}

impl Default for DemoArpSink {
    /// Am → F, a bar each. Two chords rather than one so the panel shows
    /// that each is arpeggiated over its own span rather than the whole
    /// selection being treated as one cluster.
    fn default() -> Self {
        use crate::arp::{Chord, ChordNote, PPQ};
        let chord = |start: f64, pitches: [u8; 3]| Chord {
            start_ppq: start,
            end_ppq: start + PPQ * 4.0,
            notes: pitches
                .into_iter()
                .map(|pitch| ChordNote {
                    pitch,
                    velocity: 96,
                })
                .collect(),
        };
        Self {
            chords: vec![chord(0.0, [57, 60, 64]), chord(PPQ * 4.0, [53, 57, 60])],
        }
    }
}

impl ArpSink for DemoArpSink {
    fn open(&self) -> Result<crate::arp::ArpSession, String> {
        Ok(crate::arp::ArpSession::new(self.chords.clone(), Vec::new()))
    }

    fn commit(&self, session: &crate::arp::ArpSession) -> Result<usize, String> {
        Err(format!(
            "no DAW attached — would write {} notes",
            session.resolve().len()
        ))
    }
}
