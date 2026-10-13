//! The doc's items through the engine's own services, in this process.
//!
//! The same reads and writes as [`crate::engine`]'s facade path — one
//! item at a time, its active take, its stretch markers, its notes — made
//! as plain calls on the engine ([`Direct`]) instead of one encoded
//! request each. A song's tracks, FX, sends and envelopes are a few
//! hundred calls and stay on the facade; its items are tens of thousands.

use std::collections::BTreeMap;

use daw_proto::item::{ItemRef, ItemSpan, TakeRef};
use daw_proto::midi::MidiTakeLocation;
use daw_proto::primitives::{Duration, PositionInSeconds};
use daw_proto::{ProjectContext, TrackRef};

use crate::diff::ItemField;
use crate::engine::{Direct, EngineIds, MediaRoot, item_state};
use crate::model::ItemState;

/// Every item, keyed by its doc id, with its active take.
#[must_use]
pub fn read_items(
    d: &dyn Direct,
    ctx: &ProjectContext,
    media: &MediaRoot,
    ids: &EngineIds,
) -> BTreeMap<String, ItemState> {
    d.get_all_items(ctx.clone())
        .into_iter()
        .map(|item| {
            let at = ItemRef::Guid(item.guid.clone());
            let take = d.get_active_take(ctx.clone(), at.clone()).map(|take| {
                let stretch = d.get_stretch_markers(ctx.clone(), at.clone(), TakeRef::Active);
                let notes = if take.is_midi {
                    d.notes(location(ctx, &at))
                } else {
                    Vec::new()
                };
                let volume = d.points(ctx.clone(), volume_of(&item.guid, &take.guid));
                (take, stretch, notes, volume)
            });
            (ids.items.doc(&item.guid), item_state(item, take, media))
        })
        .collect()
}

/// Where `item`'s take `take`'s volume envelope is.
fn volume_of(item: &str, take: &str) -> daw_proto::EnvelopeLocation {
    daw_proto::EnvelopeLocation {
        // Ignored for a take's envelope: the item and take say where.
        track: daw_proto::TrackRef::Index(0),
        envelope: daw_proto::EnvelopeRef::Take {
            item_guid: item.to_owned(),
            take_guid: take.to_owned(),
            kind: daw_proto::TakeEnvelopeKind::Volume,
        },
    }
}

fn location(ctx: &ProjectContext, item: &ItemRef) -> MidiTakeLocation {
    MidiTakeLocation {
        project: ctx.clone(),
        item: item.clone(),
        take: TakeRef::Active,
    }
}

/// A new item and its take (a MIDI item under a guid of the engine's own,
/// bound in `ids`); its fields are the caller's to set.
///
/// # Errors
/// When the engine refuses the item.
pub fn add_item(
    d: &dyn Direct,
    ctx: &ProjectContext,
    media: &MediaRoot,
    guid: &str,
    item: &ItemState,
    ids: &mut EngineIds,
) -> daw_proto::DawResult<()> {
    let track = TrackRef::Guid(item.track.clone());
    if item.take.midi {
        if let Some(made) = d.create_midi_item(
            ctx.clone(),
            track,
            item.position,
            item.position + item.length,
        ) && let ItemRef::Guid(made) = made.item
        {
            ids.items.bind(guid, &made);
        }
        return Ok(());
    }
    d.add_item_with_guid(
        ctx.clone(),
        track,
        guid,
        ItemSpan::new(
            PositionInSeconds::from_seconds(item.position),
            Duration::from_seconds(item.length),
        ),
    )?;
    let at = ItemRef::Guid(guid.to_owned());
    if d.add_take(ctx.clone(), at.clone()).is_some()
        && let Some(source) = &item.take.source
    {
        d.set_source_file(ctx.clone(), at, TakeRef::Active, media.to_local(source))?;
    }
    Ok(())
}

/// Delete the item the engine calls `guid`.
///
/// # Errors
/// When the engine refuses.
pub fn delete_item(d: &dyn Direct, ctx: &ProjectContext, guid: &str) -> daw_proto::DawResult<()> {
    d.delete_item(ctx.clone(), ItemRef::Guid(guid.to_owned()))
}

/// Set `fields` of the item the engine calls `guid` to `item`'s.
///
/// # Errors
/// On the first field the engine refuses.
pub fn set_item_fields(
    d: &dyn Direct,
    ctx: &ProjectContext,
    media: &MediaRoot,
    guid: &str,
    item: &ItemState,
    fields: &[ItemField],
) -> daw_proto::DawResult<()> {
    let c = || ctx.clone();
    let at = || ItemRef::Guid(guid.to_owned());
    for field in fields {
        match field {
            ItemField::Track => d.move_to_track(c(), at(), TrackRef::Guid(item.track.clone()))?,
            ItemField::Position => {
                d.set_position(c(), at(), PositionInSeconds::from_seconds(item.position))?;
            }
            ItemField::Length => d.set_length(c(), at(), Duration::from_seconds(item.length))?,
            // The engine's own state takes these ([`crate::whole`]).
            ItemField::SnapOffset
            | ItemField::Timing
            | ItemField::TakeSettings
            | ItemField::TakeEnvelopes
            | ItemField::Midi
            | ItemField::Takes => {}
            ItemField::Muted => d.set_muted(c(), at(), item.muted)?,
            ItemField::Locked => d.set_locked(c(), at(), item.locked)?,
            ItemField::Volume => daw_proto::Items::set_volume(d, c(), at(), item.volume)?,
            ItemField::FadeIn | ItemField::FadeInShape => d.set_fade_in(
                c(),
                at(),
                Duration::from_seconds(item.fade_in),
                crate::engine::fade_shape(&item.fade_in_shape),
            )?,
            ItemField::FadeOut | ItemField::FadeOutShape => d.set_fade_out(
                c(),
                at(),
                Duration::from_seconds(item.fade_out),
                crate::engine::fade_shape(&item.fade_out_shape),
            )?,
            ItemField::Label => d.set_label(c(), at(), &item.label)?,
            ItemField::Color => daw_proto::Items::set_color(d, c(), at(), item.color)?,
            ItemField::Take => {
                daw_proto::Takes::set_name(d, c(), at(), TakeRef::Active, item.take.name.clone())?;
                if let Some(source) = &item.take.source {
                    d.set_source_file(c(), at(), TakeRef::Active, media.to_local(source))?;
                }
                d.set_start_offset(
                    c(),
                    at(),
                    TakeRef::Active,
                    Duration::from_seconds(item.take.start_offset),
                )?;
                d.set_play_rate(c(), at(), TakeRef::Active, item.take.playrate)?;
            }
            ItemField::FixedLane => {
                if let Some(lane) = item.fixed_lane {
                    d.set_fixed_lane(c(), at(), lane)?;
                }
            }
            ItemField::Loop => d.set_loop_source(c(), at(), item.loop_source)?,
            ItemField::Stretch => d.set_stretch_markers(
                daw_proto::StretchTakeRef::new(c(), at(), TakeRef::Active),
                item.take
                    .stretch
                    .iter()
                    .map(|m| daw_proto::StretchMarker {
                        position: m.at,
                        source_position: m.source,
                        slope: m.slope,
                    })
                    .collect(),
            )?,
            ItemField::TakeVolume => {
                let ItemRef::Guid(guid) = at() else {
                    return Ok(());
                };
                if let Some(take) = d.get_active_take(c(), at()) {
                    let location = volume_of(&guid, &take.guid);
                    let had = d.points(c(), location.clone()).len();
                    for i in (0..had).rev() {
                        d.delete_point(c(), location.clone(), u32::try_from(i).unwrap_or(u32::MAX));
                    }
                    for g in &item.take.volume {
                        d.add_point(
                            c(),
                            location.clone(),
                            daw_proto::automation::AddPointParams::new(
                                daw_proto::PositionInSeconds::from_seconds(g.at),
                                g.value,
                                daw_proto::automation::EnvelopeShape::Linear,
                            ),
                        );
                    }
                }
            }
            ItemField::Notes => {
                let here = location(ctx, &at());
                d.write_take(
                    here.clone(),
                    daw_proto::midi::MidiTakeContent::default(),
                    daw_proto::midi::WriteMode::Replace,
                );
                d.add_notes_ppq(
                    here,
                    item.take
                        .notes
                        .iter()
                        .map(|n| daw_proto::MidiNoteCreate {
                            channel: n.channel,
                            pitch: n.pitch,
                            velocity: n.velocity,
                            start_ppq: n.start,
                            length_ppq: n.length,
                        })
                        .collect(),
                );
            }
        }
    }
    Ok(())
}
