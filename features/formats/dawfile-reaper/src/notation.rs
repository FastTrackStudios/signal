//! REAPER's MIDI notation events, in and out of item chunks.
//!
//! Inside a `<SOURCE MIDI>` block, notation lives in `<X>` blocks whose
//! payload is base64:
//!
//! ```text
//! <X 480 0 4294966816 -1
//!   /w9UUkFDIGR5bmFtaWMgcHA=
//! >
//! ```
//!
//! Decoded that is `FF 0F` — a sequencer-specific meta event — followed
//! by ASCII beginning `TRAC`:
//!
//! ```text
//! TRAC dynamic pp
//! TRAC text "Something Here"
//! ```
//!
//! This is the editable half of REAPER's notation. Unlike the project's
//! `<KEYSIG>` block (see [`crate::keysig`]), item chunks *can* be
//! round-tripped on a live project — `GetItemStateChunk` /
//! `SetItemStateChunk` both exist — so anything expressible here can be
//! written by a tool.
//!
//! ## What is not here
//!
//! Key signatures. Two real projects' worth of evidence — one of them
//! built specifically to exercise key signatures, with ten changes — put
//! every key signature in the project-level `<KEYSIG>` block and none in
//! any item. No standard MIDI key-signature meta (`FF 59`) either. If
//! REAPER can write a per-track key signature (its MIDI editor has a
//! "Key signature changes affect all tracks" toggle, action 41616), that
//! would produce a file shape not represented in either project, and
//! guessing its bytes would be inventing a format rather than reading
//! one.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;

/// A REAPER notation event attached to a position in a take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotationEvent {
    /// Position within the take, in the source's PPQ.
    pub ppq: u64,
    /// The command after the `TRAC` prefix, e.g. `dynamic pp` or
    /// `text "Something Here"`.
    pub command: String,
}

impl NotationEvent {
    pub fn new(ppq: u64, command: impl Into<String>) -> Self {
        Self {
            ppq,
            command: command.into(),
        }
    }

    /// The event's kind — the first word of the command (`dynamic`,
    /// `text`, …).
    pub fn kind(&self) -> &str {
        self.command.split_whitespace().next().unwrap_or("")
    }

    /// Encode to the base64 payload REAPER stores.
    pub fn encode(&self) -> String {
        let mut bytes = vec![0xFF, 0x0F];
        bytes.extend_from_slice(b"TRAC ");
        bytes.extend_from_slice(self.command.as_bytes());
        B64.encode(bytes)
    }

    /// Decode one payload, or `None` if it isn't a `TRAC` notation event.
    pub fn decode(ppq: u64, payload: &str) -> Option<Self> {
        let raw = B64.decode(payload.trim()).ok()?;
        let rest = raw.strip_prefix(&[0xFF, 0x0F])?;
        let text = std::str::from_utf8(rest).ok()?;
        let command = text.strip_prefix("TRAC ")?;
        Some(Self::new(ppq, command))
    }
}

/// Every notation event in a chunk, in file order.
///
/// Works on an item chunk, a take, or a whole project — it scans for
/// `<X>` blocks and ignores everything else.
pub fn parse(chunk: &str) -> Vec<NotationEvent> {
    let mut out = Vec::new();
    let mut lines = chunk.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("<X ") else {
            continue;
        };
        let Some(ppq) = rest
            .split_whitespace()
            .next()
            .and_then(|t| t.parse::<u64>().ok())
        else {
            continue;
        };
        // The payload is the next non-empty line before the closing `>`.
        if let Some(payload) = lines.peek().map(|l| l.trim().to_string()) {
            if payload != ">" {
                if let Some(event) = NotationEvent::decode(ppq, &payload) {
                    out.push(event);
                }
            }
        }
    }
    out
}

/// Render an event as the `<X>` block REAPER writes.
///
/// The three numbers after the position are flags REAPER uses for its own
/// bookkeeping; `0 0 -1` is what it accepts for a freshly authored event.
pub fn render_block(event: &NotationEvent, indent: &str) -> String {
    format!(
        "{indent}<X {} 0 0 -1\n{indent}  {}\n{indent}>\n",
        event.ppq,
        event.encode()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_SIGS: &str = include_str!("../tests/fixtures/key-signatures.RPP");

    #[test]
    fn reads_the_notation_events_from_a_real_project() {
        let events = parse(KEY_SIGS);
        assert_eq!(events.len(), 4);
        assert_eq!(events[0], NotationEvent::new(480, "dynamic pp"));
        assert_eq!(
            events[1],
            NotationEvent::new(720, "text \"Something Here\"")
        );
    }

    #[test]
    fn kinds_come_from_the_first_word() {
        let events = parse(KEY_SIGS);
        let kinds: Vec<&str> = events.iter().map(NotationEvent::kind).collect();
        assert_eq!(kinds, ["dynamic", "text", "dynamic", "dynamic"]);
    }

    /// Encoding must reproduce REAPER's own bytes exactly — this is the
    /// property that makes writing safe.
    #[test]
    fn encoding_round_trips_reapers_payloads() {
        for event in parse(KEY_SIGS) {
            let re_decoded = NotationEvent::decode(event.ppq, &event.encode())
                .expect("our own encoding must decode");
            assert_eq!(re_decoded, event);
        }
    }

    /// The exact payload REAPER wrote for `dynamic pp`, byte for byte.
    #[test]
    fn matches_reapers_literal_encoding() {
        let event = NotationEvent::new(480, "dynamic pp");
        assert_eq!(event.encode(), "/w9UUkFDIGR5bmFtaWMgcHA=");
    }

    #[test]
    fn rendered_blocks_parse_back() {
        let event = NotationEvent::new(1920, "text \"Chorus\"");
        let block = render_block(&event, "        ");
        assert_eq!(parse(&block), vec![event]);
    }

    /// Anything that isn't a TRAC notation event is skipped rather than
    /// mis-parsed — `<X>` blocks also carry other payloads.
    #[test]
    fn non_notation_payloads_are_ignored() {
        let chunk = "<X 100 0 0 -1\n  AAECAwQ=\n>\n";
        assert!(parse(chunk).is_empty());
    }
}
