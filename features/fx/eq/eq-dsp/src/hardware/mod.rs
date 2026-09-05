//! Hardware-modelled EQ curves — a separate product feature.
//!
//! These are fixed response targets taken from named analogue units, not part
//! of the parametric design pipeline. They live apart because they answer a
//! different question: the design pipeline asks "what coefficients realize the
//! band the user asked for", these ask "what did this box do at these knob
//! positions".

pub mod calibration;
pub mod hardware_eq;
pub mod neve_1073;
