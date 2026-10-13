//! Curve shapes across the editor/DAW boundary.
//!
//! `CurveShape` in the editor's core mirrors `EnvelopeShape` in
//! `daw_proto` variant for variant. Core cannot depend on the proto
//! crate — it has one dependency by design — so the two are a deliberate
//! copy, and this module is where that copy is *proved* to be exact.
//!
//! Both conversions are total: no fallback arm, no default, nothing that
//! could silently degrade a shape on the way out to a DAW or back. If
//! either enum grows a variant, this stops compiling, which is the whole
//! reason it is written as an exhaustive match rather than a lookup.

use daw::service::EnvelopeShape;
use expression_editor_core::CurveShape;

/// Editor shape → DAW shape.
pub fn to_daw(shape: CurveShape) -> EnvelopeShape {
    match shape {
        CurveShape::Linear => EnvelopeShape::Linear,
        CurveShape::Square => EnvelopeShape::Square,
        CurveShape::SlowStartEnd => EnvelopeShape::SlowStartEnd,
        CurveShape::FastStart => EnvelopeShape::FastStart,
        CurveShape::FastEnd => EnvelopeShape::FastEnd,
        CurveShape::Bezier => EnvelopeShape::Bezier,
    }
}

/// DAW shape → editor shape.
pub fn from_daw(shape: EnvelopeShape) -> CurveShape {
    match shape {
        EnvelopeShape::Linear => CurveShape::Linear,
        EnvelopeShape::Square => CurveShape::Square,
        EnvelopeShape::SlowStartEnd => CurveShape::SlowStartEnd,
        EnvelopeShape::FastStart => CurveShape::FastStart,
        EnvelopeShape::FastEnd => CurveShape::FastEnd,
        EnvelopeShape::Bezier => CurveShape::Bezier,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [CurveShape; 6] = [
        CurveShape::Linear,
        CurveShape::Square,
        CurveShape::SlowStartEnd,
        CurveShape::FastStart,
        CurveShape::FastEnd,
        CurveShape::Bezier,
    ];

    #[test]
    fn every_editor_shape_round_trips_through_the_daw() {
        for shape in ALL {
            assert_eq!(from_daw(to_daw(shape)), shape, "{shape:?} did not survive");
        }
    }

    #[test]
    fn the_mapping_is_injective() {
        // Two editor shapes collapsing onto one DAW shape would lose a
        // distinction silently on export.
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                assert_ne!(to_daw(*a), to_daw(*b), "{a:?} and {b:?} collide");
            }
        }
    }

    #[test]
    fn linear_is_the_default_on_both_sides() {
        assert_eq!(CurveShape::default(), CurveShape::Linear);
        assert_eq!(to_daw(CurveShape::default()), EnvelopeShape::default());
    }
}
