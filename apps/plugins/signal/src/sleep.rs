//! **When Signal may sleep**: what [`crate::plugin::FtsSignal`] tells its
//! host after each block, so a host that honours CLAP's process status
//! (Studio's engine, `daw-standalone`'s `plugin_sleep`) stops calling a
//! chain that has nothing to play.
//!
//! Signal answers [`EXT_SLEEP`] to tell such a host its quiet is honest.
//! nice-plug speaks CLAP for us: [`ProcessStatus::Normal`] is
//! `CLAP_PROCESS_CONTINUE_IF_NOT_QUIET` (its `clap_plugin_tail` 0), and
//! [`ProcessStatus::Tail`] is `CLAP_PROCESS_CONTINUE` with `clap_plugin_tail`
//! the given length. So Signal says `Tail(its chain's tail)` while it may
//! still sound, and `Normal` only once it has decayed: its input quiet,
//! its output quiet, both for longer than the longest silence the chain can
//! put between two sounds ([`Plan::hold`]: a delay's time, a reverb's
//! pre-delay, every block's on a lane summed), and nothing pending that
//! plays without input.
//!
//! **Quiet** is at or under [`SILENCE`], -150 dBFS, on every channel: a NAM
//! capture's bias and a filter's asymptotic decay never reach exact zero,
//! so exactness is not asked of the chain. What it puts out under that
//! level when the host stops calling it is the most a render with sleep
//! differs from one without.
//!
//! **Never asleep**: a graph with remote keys (a piece's samples played
//! from another track's hits, or hits the host placed: they play with no
//! input at all), a third-party plugin inside it whose silences Signal
//! cannot know (held [`UNKNOWN_GAP_S`] instead), while calibrating.
//!
//! Realtime-safe: reads of the graph and its parameter banks, counters, no
//! allocation.

use nice_plug::prelude::ProcessStatus;
use signal_core::block::BlockKind;
use signal_core::graph::Graph;

/// What Signal answers to say its `Normal` (`CONTINUE_IF_NOT_QUIET`) is
/// honest — said only once nothing more can come — so a host may stop
/// calling it the block it says so. A host that does not know it takes
/// `Normal` as "keep calling unless the outputs are quiet", which is what
/// nice-plug says of every plugin.
pub const EXT_SLEEP: &std::ffi::CStr = c"com.fasttrackstudio.sleep/1";

/// [`EXT_SLEEP`]'s answer: its version.
pub static EXT_SLEEP_VERSION: u32 = 1;

/// -150 dBFS: the loudest a sample may be and still be quiet.
pub const SILENCE: f32 = 3.162_277_7e-8;

/// What a block with no gap of its own adds to the hold: a filter's or a
/// compressor's release ringing under -150 dBFS, seconds.
const SHORT_GAP_S: f32 = 0.05;
/// A third-party plugin's, or a block this build does not know: as long as
/// the host's own hold.
const UNKNOWN_GAP_S: f32 = 3.0;
/// The longest tail reported, seconds.
const MAX_TAIL_S: f32 = 60.0;

/// Whether every sample of `channels` is quiet.
#[must_use]
pub fn quiet<'a>(channels: impl IntoIterator<Item = &'a [f32]>) -> bool {
    channels
        .into_iter()
        .all(|c| c.iter().all(|s| s.abs() <= SILENCE))
}

/// A graph's silences, from its blocks and their parameters now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Quiet in and out this long, frames, and nothing more can come.
    pub hold: u64,
    /// How long it rings after its input stops, frames (what `Tail` says).
    pub tail: u32,
    /// Something in it plays with no input (remote keys): never asleep.
    pub never: bool,
}

/// One block's longest silence between two sounds and its tail, seconds.
fn block_gap_tail(kind: &BlockKind, value: impl Fn(&str) -> Option<f32>) -> (f32, f32) {
    match kind {
        BlockKind::Signal(name) => match name.as_str() {
            "delay" => {
                let time = value("time").unwrap_or(2000.0).max(1.0) / 1000.0;
                let fb = value("feedback").unwrap_or(0.98).clamp(0.0, 0.98);
                // Repeats until one is under -150 dBFS: fb^n <= 10^-7.5.
                let repeats = if fb > 0.0 {
                    (-7.5 / fb.log10()).ceil()
                } else {
                    1.0
                };
                // Wow and flutter stretch the time a little.
                (time * 1.25 + SHORT_GAP_S, time * (repeats + 1.0))
            }
            "reverb" => {
                let predelay = value("predelay").unwrap_or(250.0) / 1000.0;
                let decay = value("decay").unwrap_or(1.0).clamp(0.0, 1.0);
                (predelay + 0.1, predelay + 2.0 + 28.0 * decay)
            }
            "gain" | "gate" | "drive" | "eq" | "dyn_eq" | "comp" | "transient" | "nam" | "cab"
            | "sample" => (SHORT_GAP_S, SHORT_GAP_S),
            n if signal_core::offline::mode_of(n) == signal_core::offline::Mode::Offline => {
                (SHORT_GAP_S, SHORT_GAP_S)
            }
            _ => (UNKNOWN_GAP_S, UNKNOWN_GAP_S),
        },
        BlockKind::Plugin(_) => (UNKNOWN_GAP_S, UNKNOWN_GAP_S),
    }
}

impl Plan {
    /// `graph`'s silences at `sample_rate`: every block's gap and tail
    /// summed, however its lanes route (a send's wet lane after a dry one
    /// only adds), and its latency on top. Allocation-free.
    #[must_use]
    pub fn of(graph: Option<&Graph>, sample_rate: f32) -> Self {
        let Some(g) = graph else {
            return Self::default();
        };
        let (mut gap, mut tail) = (SHORT_GAP_S, 0.0_f32);
        for lane in 0..g.lane_count() {
            for b in 0..g.block_count(lane) {
                let Some(block) = g.block(lane, b) else {
                    continue;
                };
                if block.bypass {
                    continue;
                }
                let params = block.params();
                let value = |id: &str| params.index(id).and_then(|i| params.effective(i));
                let (bg, bt) = block_gap_tail(&block.kind, value);
                gap += bg;
                tail += bt;
            }
        }
        let latency = u64::from(g.latency());
        let frames = |s: f32| (s * sample_rate).ceil().max(0.0) as u64;
        Self {
            hold: frames(gap) + latency,
            tail: u32::try_from(frames(tail.min(MAX_TAIL_S)) + latency).unwrap_or(u32::MAX),
            never: g.remote_keys() > 0,
        }
    }
}

/// The running count behind each block's [`ProcessStatus`].
#[derive(Debug)]
pub struct Sleep {
    /// Frames quiet in and out with nothing sent, so far.
    quiet: u64,
    plan: Plan,
    sample_rate: f32,
}

impl Default for Sleep {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}

impl Sleep {
    /// At `sample_rate` (the instance's activation's).
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        Self {
            quiet: 0,
            plan: Plan::default(),
            sample_rate,
        }
    }

    /// The status for a block of `frames` whose input (before the chain)
    /// and output (after) were quiet or not, with `events` (notes, host
    /// parameter changes) sent to it, or kept `awake` (calibrating). `graph` is read for its plan when a
    /// quiet run starts, so it follows the parameters as they are then.
    pub fn status(
        &mut self,
        graph: Option<&Graph>,
        input_quiet: bool,
        output_quiet: bool,
        events: bool,
        frames: usize,
        awake: bool,
    ) -> ProcessStatus {
        if !input_quiet || !output_quiet || events || awake {
            self.quiet = 0;
        } else {
            if self.quiet == 0 {
                self.plan = Plan::of(graph, self.sample_rate);
            }
            self.quiet = self.quiet.saturating_add(frames as u64);
        }
        decide(self.plan, self.quiet)
    }
}

/// `Normal` once `quiet` frames (more than none) reach the plan's hold and
/// nothing in it plays without input; `Tail` otherwise.
#[must_use]
pub fn decide(plan: Plan, quiet: u64) -> ProcessStatus {
    if !plan.never && quiet > 0 && quiet >= plan.hold {
        ProcessStatus::Normal
    } else {
        ProcessStatus::Tail(plan.tail.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_delay_holds_past_its_time_and_rings_until_its_repeats_are_under_minus_150_dbfs() {
        let (gap, tail) = block_gap_tail(&BlockKind::Signal("delay".into()), |id| match id {
            "time" => Some(500.0),
            "feedback" => Some(0.5),
            _ => None,
        });
        assert!(gap > 0.5 && gap < 0.7, "{gap}");
        // 0.5^25 is under 10^-7.5.
        assert!((tail - 0.5 * 26.0).abs() < 1e-3, "{tail}");
    }

    #[test]
    fn a_reverb_holds_past_its_predelay_and_short_blocks_barely_hold() {
        let (gap, _) = block_gap_tail(&BlockKind::Signal("reverb".into()), |id| {
            (id == "predelay").then_some(40.0)
        });
        assert!((gap - 0.14).abs() < 1e-4, "{gap}");
        for n in ["nam", "eq", "comp", "drive", "cab"] {
            assert_eq!(
                block_gap_tail(&BlockKind::Signal(n.into()), |_| None),
                (SHORT_GAP_S, SHORT_GAP_S),
                "{n}"
            );
        }
        let unknown = block_gap_tail(&BlockKind::Signal("granular".into()), |_| None);
        assert_eq!(unknown, (UNKNOWN_GAP_S, UNKNOWN_GAP_S));
    }

    #[test]
    fn quiet_is_under_minus_150_dbfs_on_every_channel() {
        assert!(quiet([&[0.0, 3e-8][..], &[-3e-8][..]]));
        assert!(!quiet([&[0.0][..], &[4e-8][..]]));
        assert!(!quiet([&[f32::NAN][..]]));
    }

    #[test]
    fn normal_only_after_the_hold_of_quiet_and_never_with_events_or_while_awake() {
        let mut s = Sleep::default();
        // No graph: the hold is 0 frames, so the first quiet block is Normal.
        assert_eq!(
            s.status(None, true, true, false, 512, false),
            ProcessStatus::Normal
        );
        assert_ne!(
            s.status(None, true, true, true, 512, false),
            ProcessStatus::Normal
        );
        assert_ne!(
            s.status(None, false, true, false, 512, false),
            ProcessStatus::Normal
        );
        assert_ne!(
            s.status(None, true, true, false, 512, true),
            ProcessStatus::Normal
        );
        assert_eq!(
            s.status(None, true, true, false, 512, false),
            ProcessStatus::Normal
        );
    }

    #[test]
    fn the_hold_must_pass_and_a_graph_with_remote_keys_never_sleeps() {
        let plan = Plan {
            hold: 1_000,
            tail: 9,
            never: false,
        };
        assert_eq!(decide(plan, 0), ProcessStatus::Tail(9));
        assert_eq!(decide(plan, 999), ProcessStatus::Tail(9));
        assert_eq!(decide(plan, 1_000), ProcessStatus::Normal);
        let never = Plan {
            never: true,
            ..plan
        };
        assert_eq!(decide(never, u64::MAX), ProcessStatus::Tail(9));
    }
}
