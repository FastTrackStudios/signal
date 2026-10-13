//! The `.session` file's text form.
//!
//! Styx on the inside, under an aliased extension so the format can diverge
//! from stock styx later without a rename (#155 decision 1). The file is the
//! readable, hand-editable source of truth — which is a constraint on *how*
//! it is written, not just what:
//!
//! - **`None` fields are omitted, not written as `@`.** `facet-styx` writes
//!   an absent `Option` as a bare `@` tag that its own parser then rejects,
//!   so round-tripping requires the `omit_none` option regardless — but the
//!   readability argument is the stronger one. A track that has no colour
//!   should say nothing about colour.
//! - **Multi-line, not inline.** A hand-editable file wants one field per
//!   line so a diff of one changed value is one changed line. That matters
//!   more here than usual: `<name>.session` is the only thing in the project
//!   that can conflict on sync, and a human resolving that conflict reads
//!   the diff (#155 decision 7).

use crate::document::DawDocument;
use crate::error::{DawError, DawResult};
use styx_format::FormatOptions;

/// Render a document to `.session` text.
///
/// Two passes, because `facet-styx` splits the job: the serializer decides
/// *what* is written (and is the only thing that knows about `omit_none`),
/// and `styx_format::format_source` decides *how* it is laid out. Serializing
/// alone yields one enormous inline line per top-level field, which is
/// unreadable and undiffable; reformatting alone cannot drop the `None`s.
pub fn to_text(document: &DawDocument) -> DawResult<String> {
    let dense = facet_styx::to_string_with_options(document, &FormatOptions::default().omit_none())
        .map_err(|error| DawError::Serialize(error.to_string()))?;
    Ok(styx_format::format_source(
        &dense,
        FormatOptions::default().pretty(72),
    ))
}

/// Parse `.session` text into a document.
///
/// `origin` names what is being parsed, for the error message only.
pub fn from_text(text: &str, origin: &str) -> DawResult<DawDocument> {
    let mut document: DawDocument =
        facet_styx::from_str(text).map_err(|error| DawError::Parse {
            path: origin.to_string(),
            message: error.to_string(),
        })?;
    // Index fields are a cache of list position, so they are re-derived on
    // load rather than trusted from the file. A hand-edit that moves a
    // track and forgets to renumber is then simply correct.
    document.reindex();
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{EnvelopeNode, ItemNode, SourceRef, TakeNode, TrackNode};
    use crate::id::EntityId;

    fn a_document() -> DawDocument {
        let mut document = DawDocument::new("Belief");
        document.sample_rate = Some(48_000);

        let track_id = EntityId::adopt("{6D5E0D4E-F122-9D41-3EDF-A96725F69EE6}");
        let item_id = EntityId::adopt("{3DFC3872-E015-3341-8047-B06479C423CE}");
        let take_id = EntityId::adopt("{6BF34048-0383-0E8C-66CC-E939082B2F5E}");

        let mut track = daw_proto::track::Track::new(track_id.to_string(), 0, "Kick".into());
        track.volume = 0.8;

        let mut item = daw_proto::item::Item {
            guid: item_id.to_string(),
            track_guid: track_id.to_string(),
            ..Default::default()
        };
        item.position = daw_proto::PositionInSeconds::from_seconds(16.0);
        item.length = daw_proto::Duration::from_seconds(18.25);

        let take = daw_proto::item::Take {
            guid: take_id.to_string(),
            item_guid: item_id.to_string(),
            is_active: true,
            name: "01-Kick".into(),
            ..Default::default()
        };

        document.tracks.push(TrackNode {
            id: track_id,
            track,
            parent: None,
            envelopes: Vec::new(),
            items: vec![ItemNode {
                id: item_id,
                item,
                takes: vec![TakeNode {
                    id: take_id,
                    take,
                    source: SourceRef::File {
                        path: "Media/kick.wav".into(),
                        kind: "WAVE".into(),
                    },
                    stretch_markers: Vec::new(),
                    envelopes: Vec::new(),
                }],
                envelopes: Vec::new(),
            }],
            fx_chain: None,
            input_fx_chain: None,
            comping: daw_proto::track::LaneComping::default(),
            receives: Vec::new(),
        });
        document.reindex();
        document
    }

    #[test]
    fn a_document_survives_a_text_roundtrip() {
        let document = a_document();
        let text = to_text(&document).expect("serialize");
        let back = from_text(&text, "<memory>").expect("parse");

        assert_eq!(back.name, document.name);
        assert_eq!(back.sample_rate, document.sample_rate);
        assert_eq!(back.tracks.len(), 1);
        assert_eq!(back.tracks[0].id, document.tracks[0].id);
        assert_eq!(back.tracks[0].track.volume, 0.8);
        assert_eq!(
            back.tracks[0].items[0].takes[0].source,
            document.tracks[0].items[0].takes[0].source
        );
    }

    #[test]
    fn absent_options_are_omitted_rather_than_written_as_unit_tags() {
        let text = to_text(&a_document()).expect("serialize");
        assert!(
            !text.lines().any(|line| line.trim_end().ends_with(" @")),
            "a bare `@` means an absent Option leaked into the file:\n{text}"
        );
    }

    #[test]
    fn the_text_is_one_field_per_line_so_diffs_are_readable() {
        let text = to_text(&a_document()).expect("serialize");
        assert!(text.contains("name Belief"), "unexpected shape:\n{text}");
        assert!(text.lines().count() > 20, "collapsed to inline:\n{text}");
    }

    #[test]
    fn indexes_are_rederived_on_load_not_trusted_from_the_file() {
        let mut document = a_document();
        // A hand-edit that renumbers wrongly. The loader must not care.
        document.tracks[0].track.index = 99;
        document.tracks[0].items[0].item.index = 42;
        let text = to_text(&document).expect("serialize");
        let back = from_text(&text, "<memory>").expect("parse");
        assert_eq!(back.tracks[0].track.index, 0);
        assert_eq!(back.tracks[0].items[0].item.index, 0);
    }

    #[test]
    fn envelope_nodes_survive_the_roundtrip() {
        let mut document = a_document();
        let track_id = document.tracks[0].id.clone();
        document.tracks[0].envelopes.push(EnvelopeNode {
            id: EntityId::derived(&track_id, "VOLENV"),
            envelope: daw_proto::automation::Envelope {
                track_guid: track_id.to_string(),
                envelope_type: daw_proto::automation::EnvelopeType::Volume,
                name: "Volume".into(),
                fx_guid: None,
                param_index: None,
                visible: true,
                armed: false,
                automation_mode: daw_proto::primitives::AutomationMode::TrimRead,
                // In a lane of its own, 44 tall — so the round trip is
                // asserted on an envelope that actually carries lane
                // facts rather than on defaults.
                in_own_lane: true,
                lane_height: 44,
                automation_item_count: 0,
                point_count: 0,
            },
            points: vec![daw_proto::automation::EnvelopePoint {
                index: 0,
                time: daw_proto::PositionInSeconds::from_seconds(1.0),
                value: 0.5,
                shape: daw_proto::automation::EnvelopeShape::Linear,
                tension: 0.0,
                selected: false,
            }],
        });
        document.reindex();

        let text = to_text(&document).expect("serialize");
        let back = from_text(&text, "<memory>").expect("parse");
        assert_eq!(back.tracks[0].envelopes.len(), 1);
        assert_eq!(back.tracks[0].envelopes[0].points.len(), 1);
        assert_eq!(back.tracks[0].envelopes[0].points[0].value, 0.5);
        assert_eq!(back.tracks[0].envelopes[0].envelope.point_count, 1);
        // The lane facts are part of the document, not a runtime detail:
        // an envelope saved in its own lane must come back in one.
        assert!(back.tracks[0].envelopes[0].envelope.in_own_lane);
        assert_eq!(back.tracks[0].envelopes[0].envelope.lane_height, 44);
    }
}
