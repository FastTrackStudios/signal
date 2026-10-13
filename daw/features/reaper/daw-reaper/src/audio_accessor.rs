//! `impl AudioAccessors for Reaper` — sync trait + REAPER's audio
//! accessor C API. Handle-based: opaque string IDs map to
//! `AudioAccessor*` pointers stored in a module-level Mutex.

use daw_proto::{
    AudioAccessors, AudioSampleData, GetSamplesRequest, ItemRef, ProjectContext, TakeRef, TrackRef,
};
use reaper_high::Reaper;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tracing::warn;

use crate::project_context::find_project_by_guid;
use crate::safe_wrappers::audio_accessor as aa_sw;
use crate::track::resolve_track_pub;

use aa_sw::SendableAccessorPtr;
use daw_control::lock::LockExt;

/// One open accessor, plus the format of what it reads.
///
/// The format is carried separately because REAPER's accessor cannot be
/// asked: `GetAudioAccessorSamples` *converts* to whatever rate and
/// channel count you name, and never tells you what the source was. A
/// caller that guesses gets a resampled take, and edits made against a
/// resampled take land in the wrong place.
#[derive(Clone, Copy)]
struct Accessor {
    ptr: SendableAccessorPtr,
    /// `None` for track accessors, which have no single source.
    format: Option<(f64, u32)>,
}

struct AccessorRegistry {
    accessors: Mutex<HashMap<String, Accessor>>,
    next_id: Mutex<u64>,
}

static REGISTRY: OnceLock<AccessorRegistry> = OnceLock::new();

fn registry() -> &'static AccessorRegistry {
    REGISTRY.get_or_init(|| AccessorRegistry {
        accessors: Mutex::new(HashMap::new()),
        next_id: Mutex::new(1),
    })
}

fn next_id() -> String {
    let mut counter = registry().next_id.lock_recoverable("audio_accessor");
    let id = *counter;
    *counter += 1;
    format!("aa-{id}")
}

pub fn store(ptr: SendableAccessorPtr) -> Option<String> {
    store_with_format(ptr, None)
}

fn store_with_format(ptr: SendableAccessorPtr, format: Option<(f64, u32)>) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let id = next_id();
    registry()
        .accessors
        .lock_recoverable("audio_accessor")
        .insert(id.clone(), Accessor { ptr, format });
    Some(id)
}

fn get_entry(id: &str) -> Option<Accessor> {
    registry()
        .accessors
        .lock_recoverable("audio_accessor")
        .get(id)
        .copied()
}

pub fn get_ptr(id: &str) -> Option<SendableAccessorPtr> {
    get_entry(id).map(|a| a.ptr)
}

pub fn remove_ptr(id: &str) -> Option<SendableAccessorPtr> {
    registry()
        .accessors
        .lock_recoverable("audio_accessor")
        .remove(id)
        .map(|a| a.ptr)
}

fn resolve_project(ctx: &ProjectContext) -> Option<reaper_high::Project> {
    match ctx {
        ProjectContext::Current => Some(Reaper::get().current_project()),
        ProjectContext::Project(guid) => find_project_by_guid(guid),
    }
}

impl AudioAccessors for crate::Reaper {
    fn create_track_accessor(&self, project: ProjectContext, track: TrackRef) -> Option<String> {
        let proj = resolve_project(&project)?;
        let t = resolve_track_pub(&proj, &track)?;
        let raw = t.raw().ok()?;
        let low = Reaper::get().medium_reaper().low();
        let accessor = aa_sw::create_track_audio_accessor(low, raw);
        store(SendableAccessorPtr::new(accessor))
    }

    fn create_take_accessor(
        &self,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> Option<String> {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();
        let reaper_project_ctx = match &project {
            ProjectContext::Current => reaper_medium::ProjectContext::CurrentProject,
            ProjectContext::Project(guid) => {
                let proj = find_project_by_guid(guid)?;
                reaper_medium::ProjectContext::Proj(proj.raw())
            }
        };
        let midi_item = crate::midi::resolve_item(medium, reaper_project_ctx, &item)?;
        let midi_take = crate::midi::resolve_take(medium, midi_item, &take)?;
        let low = medium.low();
        let format = crate::safe_wrappers::item::get_take_source_format(medium, midi_take);
        let accessor = aa_sw::create_take_audio_accessor(low, midi_take);
        store_with_format(SendableAccessorPtr::new(accessor), format)
    }

    fn has_state_changed(&self, accessor_id: &str) -> bool {
        let Some(ptr) = get_ptr(accessor_id) else {
            warn!("has_state_changed: unknown accessor ID '{}'", accessor_id);
            return false;
        };
        let low = Reaper::get().medium_reaper().low();
        aa_sw::audio_accessor_state_changed(low, ptr.get())
    }

    fn get_samples(&self, request: GetSamplesRequest) -> AudioSampleData {
        let Some(entry) = get_entry(&request.accessor_id) else {
            warn!("get_samples: unknown accessor ID '{}'", request.accessor_id);
            return AudioSampleData::default();
        };
        let ptr = entry.ptr;

        // A zero in either field means "tell me what you have", which is
        // how a caller discovers the source format before reading in
        // earnest — and the only way it can avoid naming a rate that
        // would silently resample.
        if request.sample_rate <= 0.0 || request.num_channels == 0 {
            let (rate, channels) = entry.format.unwrap_or((0.0, 0));
            return AudioSampleData {
                samples: Vec::new(),
                sample_rate: rate,
                num_channels: channels,
                num_samples: 0,
            };
        }

        let low = Reaper::get().medium_reaper().low();
        let buf_size = (request.num_channels * request.num_samples) as usize;
        let mut buf = vec![0.0f64; buf_size];
        let result = aa_sw::get_audio_accessor_samples(
            low,
            ptr.get(),
            request.sample_rate as i32,
            request.num_channels as i32,
            request.start_time,
            request.num_samples as i32,
            &mut buf,
        );
        // `GetAudioAccessorSamples` returns 0 for "no audio here" and 1
        // for "filled the buffer" — it is *not* a sample count. Reading
        // it as one truncated every read to a single frame, which looked
        // like a take with no audio in it rather than like a bug.
        if result <= 0 {
            return AudioSampleData::default();
        }
        AudioSampleData {
            samples: buf,
            sample_rate: request.sample_rate,
            num_channels: request.num_channels,
            num_samples: request.num_samples,
        }
    }

    fn destroy_accessor(&self, accessor_id: &str) {
        let Some(ptr) = remove_ptr(accessor_id) else {
            warn!("destroy_accessor: unknown accessor ID '{}'", accessor_id);
            return;
        };
        let low = Reaper::get().medium_reaper().low();
        aa_sw::destroy_audio_accessor(low, ptr.get());
    }
}
