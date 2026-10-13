//! Theme engine — trait and types for theme rendering.

/// A visual theme that can render a profile's controls.
pub trait Theme {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    /// Which profile this theme is designed for (e.g., "eq_pultec_eqp1a").
    fn profile_id(&self) -> &'static str;
}
