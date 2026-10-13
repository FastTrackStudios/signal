//! The drum corpus scenario 2 quantizes against — and the flams that
//! had to be synthesized because nobody has published any.
//!
//! ## Why a corpus at all
//!
//! Two engines decide where a drum hit is: [`gate`] (two envelope
//! followers racing, the timing detector) and [`onsets`] (spectral
//! flux, the segmenter). Neither has ever run against real close mics,
//! and **bleed between mics is the entire difficulty** — a synthesized
//! fixture has none of it, so passing on one proves nothing.
//!
//! [`gate`]: expression_editor_audio::gate
//! [`onsets`]: expression_editor_audio::onsets
//!
//! The research behind this crate
//! (`../spec/research/drum-datasets.md`, issue #158) found that no
//! permissive dataset of real multitracked drums exists. Every corpus
//! with true close mics is non-commercial; every CC BY corpus is
//! bleed-free by construction. So the corpus is assembled rather than
//! downloaded:
//!
//! - **DrumGizmo CrocellKit and DRSKit**, CC BY 4.0, rendered from MIDI
//!   we author. Every sample was captured on the whole mic array at
//!   once, so each hit already carries its own leakage into every other
//!   channel — the bleed is measured, not modelled. `fetch-corpus.sh`
//!   fetches and renders; nothing lands in the tree.
//! - **Flam sweeps we synthesize**, because flam recall cannot be
//!   measured from public data at all. The entire public corpus holds
//!   eleven annotated flams, all in MDB-Drums, whose QC collapses
//!   labels inside a 50 ms window — so even those eleven cannot say
//!   where the second strike was. See [`flam`].
//! - **ENST-Drums** as a reality check, CC BY-NC-ND, internal
//!   evaluation only. See [`enst`] and the licence section below.
//!
//! ## The one thing the flam sweep is for
//!
//! [`onsets`] documents its own conservatism: the second strike of a
//! flam "rises out of the first one's decay rather than out of silence,
//! so its spectral change is small and it falls below the threshold".
//! That is a deliberate choice, not a bug, and the useful response is
//! not to move the threshold until the miss goes away — it is to know
//! *where the knee is*. Sweeping grace spacing from 5 to 60 ms across
//! the ghost-velocity range turns a judgement call into a curve, and
//! [`recall`] is what a test asserts on.
//!
//! The synthetic renderer in [`synth`] exists so that curve can be
//! measured with no download, deterministically, in a unit test. The
//! same sweep is emitted as MIDI ([`smf`]) so the identical grid can be
//! rendered through a real kit for the number that actually counts.
//!
//! ## Licence discipline, which is the hard constraint here
//!
//! - **DrumGizmo kits — CC BY 4.0.** Usable commercially. Anything
//!   derived from them that ships must carry the attribution string
//!   [`DRUMGIZMO_ATTRIBUTION`].
//! - **ENST-Drums — CC BY-NC-ND 4.0.** "No commercial use is
//!   possible." Internal evaluation only: never vendored, never
//!   shipped, and nothing derived from it baked into a release asset.
//!   The fetch script will not touch it without an explicit opt-in flag.
//! - **The `drumgizmo` renderer CLI is GPL.** A build-time tool that is
//!   invoked, never a dependency and never in this tree.
//!
//! Nothing this crate *produces* is committed either. The repository
//! holds the tooling, the sweep definition, and small deterministic
//! fixtures the tooling generates — no audio.

pub mod enst;
pub mod flam;
pub mod recall;
pub mod smf;
pub mod synth;
pub mod wav;

pub use enst::{FlamEvidence, Histogram, Onset as EnstOnset};
pub use flam::{FlamCase, FlamSweep, Rendered};
pub use recall::{CaseResult, Curve, CurvePoint, Lag, Tolerance, measure, recall_curve};
pub use synth::Snare;
pub use wav::{Format, WavHeader, probe, probe_tree};

/// The attribution CC BY 4.0 requires of anything derived from the
/// DrumGizmo kits. Verbatim from the kit pages.
pub const DRUMGIZMO_ATTRIBUTION: &str = "Drum samples provided by DrumGizmo.org";

/// What the corpus material is licensed under, and what that permits.
///
/// Carried as data rather than prose so the CLI can print it on every
/// fetch: a licence that only exists in a README is one that gets
/// forgotten by the next person to run the script.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Licence {
    /// DrumGizmo kits. Commercial use fine, attribution required.
    CcBy40,
    /// ENST-Drums. Internal evaluation only — see the module docs.
    CcByNcNd40,
    /// Material we authored: the sweeps, the MIDI, the renders of them.
    Ours,
}

impl Licence {
    /// Whether material under this licence may end up in a release
    /// asset. The single question the whole discipline reduces to.
    pub fn shippable(self) -> bool {
        match self {
            Self::CcBy40 | Self::Ours => true,
            Self::CcByNcNd40 => false,
        }
    }

    /// The attribution line that must accompany shipped material, if
    /// any.
    pub fn attribution(self) -> Option<&'static str> {
        match self {
            Self::CcBy40 => Some(DRUMGIZMO_ATTRIBUTION),
            Self::CcByNcNd40 | Self::Ours => None,
        }
    }
}
