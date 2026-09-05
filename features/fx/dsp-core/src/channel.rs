//! Per-channel state without a bounds check that can fail.
//!
//! The pattern this replaces appeared in every DSP crate: a fixed
//! `[f32; MAX_CHANNELS]` of per-channel state, and at the top of `process` a
//! `let ch = ch.min(MAX_CHANNELS - 1);` followed by half a dozen `self.x[ch]`
//! accesses. It is correct, but the correctness lives in a line the compiler
//! cannot connect to the accesses — which is precisely what
//! `clippy::indexing_slicing` is complaining about, and it is not wrong to.
//! Worse, the clamp has to be repeated at every entry point, and the one that
//! gets forgotten panics on an audio callback, which is a hard crash in a
//! plugin host rather than a stack trace.
//!
//! So the clamp moves into the type. A [`Channel`] cannot be constructed out
//! of range, [`PerChannel`] is indexed only by one, and the accesses become
//! total: no branch to forget, no panic to reach, and nothing left for the
//! lint to object to.

use core::ops::{Index, IndexMut};

/// Channels of per-channel state every stage carries.
///
/// Stereo is the common case; the headroom covers a surround bus without a
/// second code path. This is state, not a processing width — a stage costs
/// nothing per unused channel.
pub const MAX_CHANNELS: usize = 8;

/// A channel index that is in range by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Channel(usize);

impl Channel {
    /// The first channel — left, or mono.
    pub const LEFT: Self = Self(0);
    /// The second channel.
    pub const RIGHT: Self = Self(1);

    /// Clamp `index` into range.
    ///
    /// Clamping rather than returning an `Option` is deliberate: this is
    /// called from an audio callback with a channel number the host supplied,
    /// and folding an unexpected channel onto the last one keeps audio
    /// flowing. A host that asks for channel 9 of an 8-channel stage has a
    /// bug, but the answer to that is not silence.
    #[must_use]
    pub const fn new(index: usize) -> Self {
        if index < MAX_CHANNELS {
            Self(index)
        } else {
            Self(MAX_CHANNELS - 1)
        }
    }

    /// The underlying index, in `0..MAX_CHANNELS`.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }

    /// Every channel, in order — for `reset()` loops and per-channel setup.
    pub fn all() -> impl Iterator<Item = Self> + Clone {
        (0..MAX_CHANNELS).map(Self)
    }

    /// The first `count` channels, clamped to what actually exists.
    pub fn upto(count: usize) -> impl Iterator<Item = Self> + Clone {
        (0..count.min(MAX_CHANNELS)).map(Self)
    }
}

/// One `T` per channel, indexed by [`Channel`].
///
/// `PerChannel<f32>` replaces the bare `[f32; MAX_CHANNELS]` that DSP stages
/// carried; the array is still there, still `Copy`, still on the stack, and
/// still allocation-free. What changes is that reaching into it needs a
/// [`Channel`], which cannot be out of range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PerChannel<T>([T; MAX_CHANNELS]);

impl<T> PerChannel<T> {
    /// Wrap an existing array.
    pub const fn new(values: [T; MAX_CHANNELS]) -> Self {
        Self(values)
    }

    /// Every channel's value, in channel order.
    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.0.iter()
    }

    /// Every channel's value, mutably.
    pub fn iter_mut(&mut self) -> core::slice::IterMut<'_, T> {
        self.0.iter_mut()
    }
}

impl<T: Copy> PerChannel<T> {
    /// Every channel set to `value` — the shape a `reset()` wants.
    pub const fn filled(value: T) -> Self {
        Self([value; MAX_CHANNELS])
    }

    /// Set every channel to `value`.
    pub const fn fill(&mut self, value: T) {
        self.0 = [value; MAX_CHANNELS];
    }
}

impl<T: Default + Copy> Default for PerChannel<T> {
    fn default() -> Self {
        Self([T::default(); MAX_CHANNELS])
    }
}

impl<T> Index<Channel> for PerChannel<T> {
    type Output = T;

    fn index(&self, channel: Channel) -> &T {
        // Destructuring a fixed-size array is irrefutable, which gives a
        // fallback that costs no branch to obtain. `Channel` cannot be out of
        // range, so the fallback is unreachable — it exists so this function
        // is total rather than merely believed to be.
        let [first, ..] = &self.0;
        self.0.get(channel.index()).unwrap_or(first)
    }
}

impl<T> IndexMut<Channel> for PerChannel<T> {
    fn index_mut(&mut self, channel: Channel) -> &mut T {
        let [first, rest @ ..] = &mut self.0;
        // Channel 0 lands on `first`; so, unreachably, does anything past
        // the end.
        channel
            .index()
            .checked_sub(1)
            .and_then(|i| rest.get_mut(i))
            .map_or(first, |slot| slot)
    }
}

impl<'a, T> IntoIterator for &'a PerChannel<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut PerChannel<T> {
    type Item = &'a mut T;
    type IntoIter = core::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_channel_cannot_be_built_out_of_range() {
        assert_eq!(Channel::new(0).index(), 0);
        assert_eq!(Channel::new(MAX_CHANNELS - 1).index(), MAX_CHANNELS - 1);
        assert_eq!(Channel::new(MAX_CHANNELS).index(), MAX_CHANNELS - 1);
        assert_eq!(Channel::new(usize::MAX).index(), MAX_CHANNELS - 1);
    }

    #[test]
    fn every_channel_round_trips_through_the_index_impls() {
        let mut state = PerChannel::filled(0_usize);
        for channel in Channel::all() {
            state[channel] = channel.index() + 100;
        }
        for channel in Channel::all() {
            assert_eq!(state[channel], channel.index() + 100);
        }
    }

    #[test]
    fn an_out_of_range_channel_folds_onto_the_last_one() {
        let mut state = PerChannel::filled(0_usize);
        state[Channel::new(usize::MAX)] = 7;
        assert_eq!(state[Channel::new(MAX_CHANNELS - 1)], 7);
    }

    #[test]
    fn reads_and_writes_agree_on_which_slot_is_which() {
        // The two `Index` impls are written differently — one destructures for
        // a fallback, the other offsets past the head — so they are worth
        // checking against each other rather than each against itself.
        let mut state = PerChannel::filled(0_usize);
        for channel in Channel::all() {
            state.fill(0);
            state[channel] = 1;
            assert_eq!(state.iter().sum::<usize>(), 1, "channel {} wrote more than one slot", channel.index());
            assert_eq!(state[channel], 1);
        }
    }

    #[test]
    fn upto_never_exceeds_what_exists() {
        assert_eq!(Channel::upto(2).count(), 2);
        assert_eq!(Channel::upto(MAX_CHANNELS + 100).count(), MAX_CHANNELS);
        assert_eq!(Channel::upto(0).count(), 0);
    }
}
