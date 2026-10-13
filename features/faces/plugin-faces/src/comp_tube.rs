//! Faces: the tube compressors — the two optical units (the grey opto
//! leveler, the blue opto compressor) and the two variable-mu ones (the big
//! twin-channel valve limiter, the modern stereo vari-mu).
//!
//! Each is laid out from photos of its unit: the controls it really has,
//! their positions as fractions of the panel, the knobs, meters and paint —
//! with no maker's or model's name printed. Params are namespaced per face
//! (`opto/…`, `optocomp/…`, `twinvalve/…`, `varimu/…`) with each control's
//! real range, unit and detents; every face meters a `<face>/gr`
//! gain-reduction stream (at rest on its card's 0) and declares a
//! `<face>/out` output level.

pub mod opto_comp;
pub mod opto_leveler;
pub mod twin_valve;
pub mod ui;
pub mod varimu;

use crate::face::Face;

pub fn faces() -> Vec<Face> {
    vec![opto_leveler::face(), opto_comp::face(), twin_valve::face(), varimu::face()]
}

/// The tube compressors redrawn for a host's FX lane (see [`crate::lane`]).
pub fn lane_faces() -> Vec<Face> {
    let mut v = opto_leveler::lane::faces();
    v.extend(opto_comp::lane::faces());
    v.extend(twin_valve::lane::faces());
    v.extend(varimu::lane::faces());
    v
}
