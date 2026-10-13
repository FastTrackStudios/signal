//! Audio playback: click / count / guide sample players, mixing helpers,
//! and the subdivision trigger scheduler.
//!
//! Ported from fts-guide `src/audio/`. The players no longer take
//! `Arc<Mutex<Option<DecodedAudioF32>>>` — the engine owns its `SampleBank`
//! and passes plain `Option<&AudioSample>` references (no locking on the
//! render path).

mod click_player;
mod count_player;
mod guide_player;
mod routing;
mod trigger_scheduler;

pub use click_player::{ClickPlayer, ClickPlayerState};
pub use count_player::{CountPlayer, CountPlayerState};
pub use guide_player::{GuidePlayer, GuidePlayerState};
pub use routing::AudioRouter;
pub use trigger_scheduler::{
    SubdivisionIntervals, TriggerResult, TriggerScheduler, TriggerSchedulingParams,
};
