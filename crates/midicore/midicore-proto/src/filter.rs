//! Declarative event matching — the ergonomic way to say "notes on channel 1"
//! or "any CC" without hand-writing `match` arms at every call site.

use crate::event::MidiEvent;
use crate::number::{Channel, ControllerNumber};

/// A predicate over [`MidiEvent`]s. Compose with [`Filter::and`] / [`Filter::or`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Filter {
    /// Matches everything.
    Any,
    /// Matches nothing.
    None,
    /// Channel-voice messages on this channel.
    OnChannel(Channel),
    /// Note-on / note-off (either).
    Notes,
    /// Control-change messages, optionally for one controller number.
    ControlChange(Option<ControllerNumber>),
    /// System-realtime (clock/start/continue/stop/sensing/reset).
    Realtime,
    /// Logical AND.
    And(Box<Filter>, Box<Filter>),
    /// Logical OR.
    Or(Box<Filter>, Box<Filter>),
    /// Logical NOT.
    Not(Box<Filter>),
}

impl Filter {
    /// Does `event` satisfy this filter?
    pub fn matches(&self, event: &MidiEvent) -> bool {
        use MidiEvent::*;
        match self {
            Filter::Any => true,
            Filter::None => false,
            Filter::OnChannel(ch) => event.channel() == Some(*ch),
            Filter::Notes => matches!(event, NoteOn { .. } | NoteOff { .. }),
            Filter::ControlChange(which) => match event {
                ControlChange { controller, .. } => which.is_none_or(|c| c == *controller),
                _ => false,
            },
            Filter::Realtime => matches!(
                event,
                Clock | Start | Continue | Stop | ActiveSensing | Reset
            ),
            Filter::And(a, b) => a.matches(event) && b.matches(event),
            Filter::Or(a, b) => a.matches(event) || b.matches(event),
            Filter::Not(f) => !f.matches(event),
        }
    }

    pub fn and(self, other: Filter) -> Filter {
        Filter::And(Box::new(self), Box::new(other))
    }
    pub fn or(self, other: Filter) -> Filter {
        Filter::Or(Box::new(self), Box::new(other))
    }
    // Deliberately a builder method like `and`/`or` (readable chained:
    // `Filter::Notes.not()`), not the `std::ops::Not` operator.
    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Filter {
        Filter::Not(Box::new(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_on_channel_one() {
        use crate::number::{ControllerValue, KeyNumber, Velocity};
        let f = Filter::Notes.and(Filter::OnChannel(Channel::new(0)));
        let note = MidiEvent::NoteOn {
            channel: Channel::new(0),
            key: KeyNumber::new(64),
            velocity: Velocity::new(90),
        };
        let cc = MidiEvent::ControlChange {
            channel: Channel::new(0),
            controller: ControllerNumber::new(1),
            value: ControllerValue::new(0),
        };
        assert!(f.matches(&note));
        assert!(!f.matches(&cc));
    }
}
