//! `impl Takes for Reaper` — sync trait + REAPER C API.
//!
//! The architect::rpc bridge dispatches each call to REAPER's main
//! thread through `HasDispatcher`, so method bodies assume main-thread
//! execution. Helpers `ReaperTake::resolve_take` and
//! `ReaperTake::media_take_to_take` are still hosted on `item.rs` and
//! reused here; subscribe + broadcaster scaffolding retired alongside
//! the architect::rpc port.

use std::ffi::CString;

use daw_proto::Takes;
use daw_proto::{
    AddTakeMarkerAtPositionRequest, DawError, DawResult, Duration, ItemRef, ProjectContext,
    SourceType, Take, TakeMarker, TakeMarkerCreate, TakeMarkerUpdate, TakeRef,
};
use reaper_high::Reaper as ReaperHigh;
use reaper_medium::{
    ItemAttributeKey, MediaItem, ProjectContext as ReaperProjectContext, Semitones,
    TakeAttributeKey,
};

use crate::item::{ReaperItem, ReaperTake};
use crate::project_context::resolve_project_context;
use crate::safe_wrappers::item as item_sw;

const EPS: f64 = 0.001;

fn item_not_found(item: &ItemRef) -> DawError {
    DawError::not_found("Item", &format!("{item:?}"))
}

fn take_not_found(take: &TakeRef) -> DawError {
    DawError::not_found("Take", &format!("{take:?}"))
}

impl Takes for crate::Reaper {
    fn get_takes(&self, _project: ProjectContext, item: ItemRef) -> Vec<Take> {
        let Some(item_ptr) = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
        else {
            return Vec::new();
        };
        let low = ReaperHigh::get().medium_reaper().low();
        let count = item_sw::count_takes(low, item_ptr);
        let mut out = Vec::with_capacity(count.max(0) as usize);
        for i in 0..count {
            if let Some(take) = item_sw::get_take(low, item_ptr, i)
                && let Some(t) = ReaperTake::media_take_to_take(item_ptr, take, i as u32)
            {
                out.push(t);
            }
        }
        out
    }

    fn get_take(&self, _project: ProjectContext, item: ItemRef, take: TakeRef) -> Option<Take> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)?;
        let take_ptr = ReaperTake::resolve_take(item_ptr, &take)?;
        let index = match take {
            TakeRef::Index(idx) => idx,
            _ => {
                let low = ReaperHigh::get().medium_reaper().low();
                let mut found = 0u32;
                let count = item_sw::count_takes(low, item_ptr);
                for i in 0..count {
                    if item_sw::get_take(low, item_ptr, i) == Some(take_ptr) {
                        found = i as u32;
                        break;
                    }
                }
                found
            }
        };
        ReaperTake::media_take_to_take(item_ptr, take_ptr, index)
    }

    fn get_active_take(&self, _project: ProjectContext, item: ItemRef) -> Option<Take> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)?;
        let medium = ReaperHigh::get().medium_reaper();
        let low = medium.low();
        let take_ptr = item_sw::get_active_take(medium, item_ptr)?;
        let mut index = 0u32;
        let count = item_sw::count_takes(low, item_ptr);
        for i in 0..count {
            if item_sw::get_take(low, item_ptr, i) == Some(take_ptr) {
                index = i as u32;
                break;
            }
        }
        ReaperTake::media_take_to_take(item_ptr, take_ptr, index)
    }

    fn take_count(&self, _project: ProjectContext, item: ItemRef) -> u32 {
        let Some(item_ptr) = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
        else {
            return 0;
        };
        let low = ReaperHigh::get().medium_reaper().low();
        item_sw::count_takes(low, item_ptr).max(0) as u32
    }

    fn add_take(&self, _project: ProjectContext, item: ItemRef) -> Option<String> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)?;
        let medium = ReaperHigh::get().medium_reaper();
        let take = item_sw::add_take_to_media_item(medium, item_ptr)?;
        let low = medium.low();
        Some(item_sw::get_take_guid_string(low, take))
    }

    fn delete_take(&self, _project: ProjectContext, item: ItemRef, take: TakeRef) -> DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let take_ptr =
            ReaperTake::resolve_take(item_ptr, &take).ok_or_else(|| take_not_found(&take))?;

        // REAPER's API has no DeleteTakeFromMediaItem; action 40129
        // ("Item: Delete active take from items") is the cleanest path.
        // Snapshot selection → isolate target → fire → restore.
        let medium = ReaperHigh::get().medium_reaper();
        let low = medium.low();
        let proj_ctx = ReaperProjectContext::CurrentProject;

        let total = medium.count_media_items(proj_ctx);
        let mut prior: Vec<MediaItem> = Vec::with_capacity(total as usize);
        for i in 0..total {
            if let Some(it) = medium.get_media_item(proj_ctx, i)
                && item_sw::is_item_selected(low, it)
            {
                prior.push(it);
            }
        }
        for it in &prior {
            item_sw::set_media_item_selected(medium, *it, false);
        }
        item_sw::set_media_item_selected(medium, item_ptr, true);
        item_sw::set_active_take(low, take_ptr);

        item_sw::main_on_command_ex(medium, reaper_medium::CommandId::new(40129), 0, proj_ctx);

        item_sw::set_media_item_selected(medium, item_ptr, false);
        for it in prior {
            item_sw::set_media_item_selected(medium, it, true);
        }
        Ok(())
    }

    fn set_active_take(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let take_ptr =
            ReaperTake::resolve_take(item_ptr, &take).ok_or_else(|| take_not_found(&take))?;
        let low = ReaperHigh::get().medium_reaper().low();
        item_sw::set_active_take(low, take_ptr);
        Ok(())
    }

    fn set_name(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        name: String,
    ) -> DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let take_ptr =
            ReaperTake::resolve_take(item_ptr, &take).ok_or_else(|| take_not_found(&take))?;
        let low = ReaperHigh::get().medium_reaper().low();
        let cname = CString::new(name)
            .map_err(|e| DawError::operation_failed(format!("invalid take name: {e}")))?;
        item_sw::set_take_name(low, take_ptr, &cname);
        Ok(())
    }

    fn set_color(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        color: Option<u32>,
    ) -> DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let take_ptr =
            ReaperTake::resolve_take(item_ptr, &take).ok_or_else(|| take_not_found(&take))?;
        item_sw::set_take_color(ReaperHigh::get().medium_reaper(), take_ptr, color);
        Ok(())
    }

    fn set_volume(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        volume: f64,
    ) -> DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let take_ptr =
            ReaperTake::resolve_take(item_ptr, &take).ok_or_else(|| take_not_found(&take))?;
        item_sw::set_take_info_value(
            ReaperHigh::get().medium_reaper(),
            take_ptr,
            TakeAttributeKey::Vol,
            volume,
        );
        Ok(())
    }

    fn set_play_rate(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        rate: f64,
    ) -> DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let take_ptr =
            ReaperTake::resolve_take(item_ptr, &take).ok_or_else(|| take_not_found(&take))?;
        item_sw::set_take_info_value(
            ReaperHigh::get().medium_reaper(),
            take_ptr,
            TakeAttributeKey::PlayRate,
            rate,
        );
        Ok(())
    }

    fn set_pitch(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        semitones: f64,
    ) -> DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let take_ptr =
            ReaperTake::resolve_take(item_ptr, &take).ok_or_else(|| take_not_found(&take))?;
        let medium = ReaperHigh::get().medium_reaper();
        let pitch = Semitones::new(semitones)
            .map_err(|e| DawError::operation_failed(format!("invalid pitch: {e:?}")))?;
        item_sw::set_take_pitch(medium, take_ptr, pitch);
        Ok(())
    }

    fn set_preserve_pitch(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        preserve: bool,
    ) -> DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let take_ptr =
            ReaperTake::resolve_take(item_ptr, &take).ok_or_else(|| take_not_found(&take))?;
        item_sw::set_take_preserve_pitch(ReaperHigh::get().medium_reaper(), take_ptr, preserve);
        Ok(())
    }

    fn set_start_offset(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        offset: Duration,
    ) -> DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let take_ptr =
            ReaperTake::resolve_take(item_ptr, &take).ok_or_else(|| take_not_found(&take))?;
        item_sw::set_take_info_value(
            ReaperHigh::get().medium_reaper(),
            take_ptr,
            TakeAttributeKey::StartOffs,
            offset.as_seconds(),
        );
        Ok(())
    }

    fn set_source_file(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        path: String,
    ) -> DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let take_ptr =
            ReaperTake::resolve_take(item_ptr, &take).ok_or_else(|| take_not_found(&take))?;
        let low = ReaperHigh::get().medium_reaper().low();
        let new_source = item_sw::pcm_source_create_from_file(low, &path).ok_or_else(|| {
            DawError::operation_failed(format!(
                "PCM_Source_CreateFromFile returned null for path: {path}"
            ))
        })?;
        item_sw::set_take_source(low, take_ptr, new_source);
        Ok(())
    }

    fn get_source_type(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> SourceType {
        let Some(item_ptr) = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
        else {
            return SourceType::Unknown;
        };
        let Some(take_ptr) = ReaperTake::resolve_take(item_ptr, &take) else {
            return SourceType::Unknown;
        };
        let medium = ReaperHigh::get().medium_reaper();
        let low = medium.low();
        let Some(source) = item_sw::get_take_source(medium, take_ptr) else {
            return SourceType::Empty;
        };
        let tag = item_sw::get_pcm_source_type(low, source).unwrap_or_default();
        match tag.as_str() {
            "midi" | "midipool" => SourceType::Midi,
            "video" => SourceType::Video,
            "" | "empty" => SourceType::Empty,
            _ => SourceType::Audio,
        }
    }

    fn get_take_markers(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> Vec<TakeMarker> {
        let Some(item_ptr) = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
        else {
            return Vec::new();
        };
        let Some(take_ptr) = ReaperTake::resolve_take(item_ptr, &take) else {
            return Vec::new();
        };
        let low = ReaperHigh::get().medium_reaper().low();
        item_sw::list_take_markers(low, take_ptr)
            .into_iter()
            .map(|m| TakeMarker {
                index: m.index,
                name: m.name,
                source_position_seconds: m.source_position_seconds,
                color: m.color,
                // REAPER keeps neither an id nor a hit's properties.
                ..TakeMarker::default()
            })
            .collect()
    }

    fn add_take_marker(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        marker: TakeMarkerCreate,
    ) -> Option<u32> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)?;
        let take_ptr = ReaperTake::resolve_take(item_ptr, &take)?;
        let low = ReaperHigh::get().medium_reaper().low();
        item_sw::add_take_marker(
            low,
            take_ptr,
            &marker.name,
            marker.source_position_seconds,
            marker.color,
        )
    }

    fn set_take_marker(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        update: TakeMarkerUpdate,
    ) -> DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let take_ptr =
            ReaperTake::resolve_take(item_ptr, &take).ok_or_else(|| take_not_found(&take))?;
        let low = ReaperHigh::get().medium_reaper().low();
        item_sw::update_take_marker(
            low,
            take_ptr,
            update.index,
            update.name.as_deref(),
            update.source_position_seconds,
            update.color,
        );
        Ok(())
    }

    fn delete_take_marker(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        index: u32,
    ) -> DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let take_ptr =
            ReaperTake::resolve_take(item_ptr, &take).ok_or_else(|| take_not_found(&take))?;
        let low = ReaperHigh::get().medium_reaper().low();
        item_sw::delete_take_marker(low, take_ptr, index);
        Ok(())
    }

    fn set_take_markers(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        markers: Vec<TakeMarker>,
    ) -> DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let take_ptr =
            ReaperTake::resolve_take(item_ptr, &take).ok_or_else(|| take_not_found(&take))?;
        let low = ReaperHigh::get().medium_reaper().low();
        // Last first, so each delete leaves the indices below it alone.
        let existing = item_sw::list_take_markers(low, take_ptr);
        for m in existing.iter().rev() {
            item_sw::delete_take_marker(low, take_ptr, m.index);
        }
        for m in &markers {
            item_sw::add_take_marker(low, take_ptr, &m.name, m.source_position_seconds, m.color);
        }
        Ok(())
    }

    fn add_take_marker_at_position(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        request: AddTakeMarkerAtPositionRequest,
    ) -> Option<u32> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)?;
        let take_ptr = ReaperTake::resolve_take(item_ptr, &take)?;
        let medium = ReaperHigh::get().medium_reaper();
        let low = medium.low();

        let project_seconds = if let Some(t) = request.position.time {
            t.as_seconds()
        } else if let Some(m) = request.position.musical {
            let measure = (m.measure - 1).max(0) as f64;
            let beat = m.beat as f64;
            let sub = (m.subdivision as f64) / 1000.0;
            let total_qn = measure * 4.0 + beat + sub;
            let reaper_ctx = resolve_project_context(&project);
            medium
                .time_map_2_qn_to_time(
                    reaper_ctx,
                    reaper_medium::PositionInQuarterNotes::new_panic(total_qn),
                )
                .get()
        } else {
            tracing::warn!(
                "add_take_marker_at_position: Position has no time/musical form; nothing to do"
            );
            return None;
        };

        let item_pos = item_sw::get_item_info_value(medium, item_ptr, ItemAttributeKey::Position);
        let item_len = item_sw::get_item_info_value(medium, item_ptr, ItemAttributeKey::Length);
        let item_end = item_pos + item_len;
        if project_seconds < item_pos - EPS || project_seconds > item_end + EPS {
            tracing::warn!(
                project_seconds,
                item_pos,
                item_end,
                "add_take_marker_at_position: position outside item bounds; no-op"
            );
            return None;
        }

        let play_rate = item_sw::get_take_info_value(medium, take_ptr, TakeAttributeKey::PlayRate);
        let start_offset =
            item_sw::get_take_info_value(medium, take_ptr, TakeAttributeKey::StartOffs);
        let play_rate = if play_rate <= 0.0 { 1.0 } else { play_rate };

        let source_seconds = start_offset + (project_seconds - item_pos) * play_rate;
        if source_seconds < -EPS {
            tracing::warn!(
                source_seconds,
                "add_take_marker_at_position: computed source position negative; no-op"
            );
            return None;
        }
        let source_seconds = source_seconds.max(0.0);

        item_sw::add_take_marker(low, take_ptr, &request.name, source_seconds, request.color)
    }

    fn run_take_rating_action(
        &self,
        _project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
        command_id: u32,
    ) -> DawResult<()> {
        let item_ptr = ReaperItem::resolve_item(&item, ReaperProjectContext::CurrentProject)
            .ok_or_else(|| item_not_found(&item))?;
        let take_ptr =
            ReaperTake::resolve_take(item_ptr, &take).ok_or_else(|| take_not_found(&take))?;
        let medium = ReaperHigh::get().medium_reaper();
        let low = medium.low();
        let proj_ctx = ReaperProjectContext::CurrentProject;

        let total = medium.count_media_items(proj_ctx);
        let mut prior: Vec<MediaItem> = Vec::with_capacity(total as usize);
        for i in 0..total {
            if let Some(it) = medium.get_media_item(proj_ctx, i)
                && item_sw::is_item_selected(low, it)
            {
                prior.push(it);
            }
        }
        for it in &prior {
            item_sw::set_media_item_selected(medium, *it, false);
        }
        item_sw::set_media_item_selected(medium, item_ptr, true);
        item_sw::set_active_take(low, take_ptr);

        item_sw::main_on_command_ex(
            medium,
            reaper_medium::CommandId::new(command_id),
            0,
            proj_ctx,
        );

        item_sw::set_media_item_selected(medium, item_ptr, false);
        for it in prior {
            item_sw::set_media_item_selected(medium, it, true);
        }
        Ok(())
    }
}
