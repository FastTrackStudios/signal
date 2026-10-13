//! Composable mouse-modifier sections per profile.
//!
//! This keeps profile definitions maintainable by letting each context live in its own function
//! (e.g. `ruler_left_drag()`, `track_left_drag()`), then composing sections into a full profile.

use super::manager::MouseModifierSetting;

pub mod fts;
pub mod logic;
pub mod reaper;

pub fn merge_sections(
    sections: impl IntoIterator<Item = Vec<MouseModifierSetting>>,
) -> Vec<MouseModifierSetting> {
    sections.into_iter().flatten().collect()
}
