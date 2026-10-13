//! Tick marks and text marks (iced_audio parity).
//!
//! Used by knobs and sliders to render visual reference points along the
//! parameter range. Scaffolding only — rendering is currently a no-op.

/// Position along a parameter's normalized range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TickMark {
    pub position: f32,
    pub tier: TickTier,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TickTier {
    One,
    Two,
    Three,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TickMarkGroup(pub Vec<TickMark>);

#[derive(Clone, Debug, PartialEq)]
pub struct TextMark {
    pub position: f32,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TextMarkGroup(pub Vec<TextMark>);

impl TickMarkGroup {
    /// Even-spaced tick marks at `count` positions across [0,1].
    pub fn evenly_spaced(count: usize, tier: TickTier) -> Self {
        if count == 0 {
            return Self::default();
        }
        let denom = (count.saturating_sub(1)).max(1) as f32;
        Self(
            (0..count)
                .map(|i| TickMark {
                    position: i as f32 / denom,
                    tier,
                })
                .collect(),
        )
    }
}
