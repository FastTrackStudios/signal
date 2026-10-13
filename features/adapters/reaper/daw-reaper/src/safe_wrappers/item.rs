//! Safe wrappers for REAPER item/take APIs.
//!
//! Covers both low-level (`medium.low()`) and unsafe medium-level calls
//! so that service code in `item.rs` can be 100% safe Rust.

use std::ffi::CStr;

use super::ReaperLow;
use reaper_medium::{
    DurationInSeconds, ItemAttributeKey, MediaItem, MediaItemTake, MediaTrack,
    ProjectContext as ReaperProjectContext, Semitones, TakeAttributeKey, UiRefreshBehavior,
};

// =============================================================================
// Low-level wrappers (via medium.low())
// =============================================================================

/// Get item state chunk (serialized RPP text).
pub fn get_item_state_chunk(
    low: &ReaperLow,
    item: MediaItem,
    buffer_size: usize,
) -> Option<String> {
    super::buffer::with_string_buffer(buffer_size, |buf, len| unsafe {
        low.GetSetItemState(item.as_ptr(), buf, len)
    })
}

/// Get a track's GUID string.
pub fn get_track_guid(low: &ReaperLow, track: MediaTrack) -> String {
    let mut buf = vec![0u8; 256];
    unsafe {
        low.GetSetMediaTrackInfo_String(
            track.as_ptr(),
            c"GUID".as_ptr(),
            buf.as_mut_ptr() as *mut i8,
            false,
        );
    }
    super::buffer::string_from_buffer(&buf)
}

/// Set a track's GUID (`GUID`, `{XXXXXXXX-...}` spelling). Returns
/// whether REAPER accepted it.
pub fn set_track_guid(low: &ReaperLow, track: MediaTrack, guid: &std::ffi::CStr) -> bool {
    let mut buf = guid.to_bytes_with_nul().to_vec();
    // SAFETY: the buffer is NUL-terminated and outlives the call; with
    // `setNewValue = true` REAPER only reads it.
    unsafe {
        low.GetSetMediaTrackInfo_String(
            track.as_ptr(),
            c"GUID".as_ptr(),
            buf.as_mut_ptr() as *mut i8,
            true,
        )
    }
}

/// Set an item's GUID (`GUID`, `{XXXXXXXX-...}` spelling). Returns
/// whether REAPER accepted it.
pub fn set_item_guid(low: &ReaperLow, item: MediaItem, guid: &std::ffi::CStr) -> bool {
    let mut buf = guid.to_bytes_with_nul().to_vec();
    // SAFETY: as `set_track_guid`.
    unsafe {
        low.GetSetMediaItemInfo_String(
            item.as_ptr(),
            c"GUID".as_ptr(),
            buf.as_mut_ptr() as *mut i8,
            true,
        )
    }
}

/// Check if an item is selected.
pub fn is_item_selected(low: &ReaperLow, item: MediaItem) -> bool {
    unsafe { low.IsMediaItemSelected(item.as_ptr()) }
}

/// Count takes in an item.
pub fn count_takes(low: &ReaperLow, item: MediaItem) -> i32 {
    unsafe { low.CountTakes(item.as_ptr()) }
}

/// Get a take by index within an item.
pub fn get_take(low: &ReaperLow, item: MediaItem, index: i32) -> Option<MediaItemTake> {
    let ptr = unsafe { low.GetTake(item.as_ptr(), index) };
    MediaItemTake::new(ptr)
}

/// Get a take by index, returning a medium-level newtype.
#[deprecated(note = "use get_take instead, which now returns Option<MediaItemTake>")]
pub fn get_take_medium(low: &ReaperLow, item: MediaItem, index: i32) -> Option<MediaItemTake> {
    get_take(low, item, index)
}

/// Set the active take for an item.
pub fn set_active_take(low: &ReaperLow, take: MediaItemTake) {
    unsafe {
        low.SetActiveTake(take.as_ptr());
    }
}

/// Set a take's name.
pub fn set_take_name(low: &ReaperLow, take: MediaItemTake, name: &std::ffi::CString) {
    unsafe {
        low.GetSetMediaItemTakeInfo_String(
            take.as_ptr(),
            c"P_NAME".as_ptr(),
            name.as_ptr() as *mut i8,
            true,
        );
    }
}

/// Read a take's name into a buffer.
pub fn get_take_name(low: &ReaperLow, take: MediaItemTake) -> String {
    let mut buf = vec![0u8; 256];
    unsafe {
        low.GetSetMediaItemTakeInfo_String(
            take.as_ptr(),
            c"P_NAME".as_ptr(),
            buf.as_mut_ptr() as *mut i8,
            false,
        );
    }
    super::buffer::string_from_buffer(&buf)
}

/// Set an item's notes (`P_NOTES`).
///
/// REAPER renders item notes as the item's on-screen label, which makes
/// this the natural place to store what an item *means* — a chord symbol,
/// a key change — so the data is visible, editable by hand, and travels
/// with the item. Pair it with `IMGRESOURCEFLAGS 2` in the item chunk to
/// have REAPER draw it as text.
pub fn set_item_notes(low: &ReaperLow, item: MediaItem, notes: &std::ffi::CString) {
    unsafe {
        low.GetSetMediaItemInfo_String(
            item.as_ptr(),
            c"P_NOTES".as_ptr(),
            notes.as_ptr() as *mut i8,
            true,
        );
    }
}

/// Read an item's notes (`P_NOTES`).
pub fn get_item_notes(low: &ReaperLow, item: MediaItem) -> String {
    let mut buf = vec![0u8; 1024];
    unsafe {
        low.GetSetMediaItemInfo_String(
            item.as_ptr(),
            c"P_NOTES".as_ptr(),
            buf.as_mut_ptr() as *mut i8,
            false,
        );
    }
    super::buffer::string_from_buffer(&buf)
}

/// Move a media item to a different track.
pub fn move_item_to_track(low: &ReaperLow, item: MediaItem, track: MediaTrack) {
    unsafe {
        low.MoveMediaItemToTrack(item.as_ptr(), track.as_ptr());
    }
}

/// Get the media item that owns a take.
pub fn get_take_item(low: &ReaperLow, take: MediaItemTake) -> Option<MediaItem> {
    let ptr = unsafe { low.GetMediaItemTake_Item(take.as_ptr()) };
    MediaItem::new(ptr)
}

/// Get a float info value from a media item.
pub fn get_media_item_info_value(low: &ReaperLow, item: MediaItem, attr: &CStr) -> f64 {
    unsafe { low.GetMediaItemInfo_Value(item.as_ptr(), attr.as_ptr()) }
}

// =============================================================================
// Medium-level wrappers (unsafe reaper_medium calls)
// =============================================================================

/// Get a media item from a track by index.
pub fn get_track_media_item(
    medium: &reaper_medium::Reaper,
    track: MediaTrack,
    index: u32,
) -> Option<MediaItem> {
    unsafe { medium.get_track_media_item(track, index) }
}

/// Get the track that owns a media item.
pub fn get_media_item_track(medium: &reaper_medium::Reaper, item: MediaItem) -> Option<MediaTrack> {
    unsafe { medium.get_media_item_track(item) }
}

/// Count media items on a track.
pub fn count_track_media_items(medium: &reaper_medium::Reaper, track: MediaTrack) -> u32 {
    unsafe { medium.count_track_media_items(track) }
}

/// Get an item attribute value (medium-level, typed key).
pub fn get_item_info_value(
    medium: &reaper_medium::Reaper,
    item: MediaItem,
    key: ItemAttributeKey,
) -> f64 {
    unsafe { medium.get_media_item_info_value(item, key) }
}

/// Set an item attribute value (medium-level, typed key).
pub fn set_item_info_value(
    medium: &reaper_medium::Reaper,
    item: MediaItem,
    key: ItemAttributeKey,
    value: f64,
) {
    unsafe {
        let _ = medium.set_media_item_info_value(item, key, value);
    }
}

/// Set media item position.
pub fn set_media_item_position(
    medium: &reaper_medium::Reaper,
    item: MediaItem,
    position: reaper_medium::PositionInSeconds,
    refresh: UiRefreshBehavior,
) {
    unsafe {
        let _ = medium.set_media_item_position(item, position, refresh);
    }
}

/// Set media item length.
pub fn set_media_item_length(
    medium: &reaper_medium::Reaper,
    item: MediaItem,
    length: DurationInSeconds,
    refresh: UiRefreshBehavior,
) {
    unsafe {
        let _ = medium.set_media_item_length(item, length, refresh);
    }
}

/// Set media item selected state.
pub fn set_media_item_selected(medium: &reaper_medium::Reaper, item: MediaItem, selected: bool) {
    unsafe {
        medium.set_media_item_selected(item, selected);
    }
}

/// Add a new media item to a track.
pub fn add_media_item_to_track(
    medium: &reaper_medium::Reaper,
    track: MediaTrack,
) -> Option<MediaItem> {
    unsafe { medium.add_media_item_to_track(track).ok() }
}

/// Delete a media item from a track.
pub fn delete_track_media_item(medium: &reaper_medium::Reaper, track: MediaTrack, item: MediaItem) {
    unsafe {
        let _ = medium.delete_track_media_item(track, item);
    }
}

/// Execute a REAPER action by command ID.
pub fn main_on_command_ex(
    medium: &reaper_medium::Reaper,
    command_id: reaper_medium::CommandId,
    flag: i32,
    project: ReaperProjectContext,
) {
    medium.main_on_command_ex(command_id, flag, project);
}

/// Get the active take of a media item.
pub fn get_active_take(medium: &reaper_medium::Reaper, item: MediaItem) -> Option<MediaItemTake> {
    unsafe { medium.get_active_take(item) }
}

/// Add a new take to a media item.
pub fn add_take_to_media_item(
    medium: &reaper_medium::Reaper,
    item: MediaItem,
) -> Option<MediaItemTake> {
    unsafe { medium.add_take_to_media_item(item).ok() }
}

/// Get a take attribute value.
pub fn get_take_info_value(
    medium: &reaper_medium::Reaper,
    take: MediaItemTake,
    key: TakeAttributeKey,
) -> f64 {
    unsafe { medium.get_media_item_take_info_value(take, key) }
}

/// Set a take attribute value.
pub fn set_take_info_value(
    medium: &reaper_medium::Reaper,
    take: MediaItemTake,
    key: TakeAttributeKey,
    value: f64,
) {
    unsafe {
        let _ = medium.set_media_item_take_info_value(take, key, value);
    }
}

/// Get the pitch of a take in semitones.
pub fn get_take_pitch(medium: &reaper_medium::Reaper, take: MediaItemTake) -> f64 {
    unsafe { medium.get_set_media_item_take_info_get_pitch(take).get() }
}

/// Set the pitch of a take in semitones.
pub fn set_take_pitch(medium: &reaper_medium::Reaper, take: MediaItemTake, semitones: Semitones) {
    unsafe {
        medium.get_set_media_item_take_info_set_pitch(take, semitones);
    }
}

/// Get the PCM source of a take.
pub fn get_take_source(
    medium: &reaper_medium::Reaper,
    take: MediaItemTake,
) -> Option<reaper_medium::PcmSource> {
    unsafe { medium.get_media_item_take_source(take) }
}

/// Read the type tag of a PCM source via `GetMediaSourceType`. Returns
/// the lowercase tag (e.g. `"midi"`, `"wave"`, `"flac"`, `"video"`,
/// `"empty"`). `None` on read failure / non-UTF-8 / null source.
pub fn get_pcm_source_type(
    low: &reaper_low::Reaper,
    source: reaper_medium::PcmSource,
) -> Option<String> {
    let mut buf = vec![0i8; 64];
    unsafe {
        low.GetMediaSourceType(
            source.as_ptr(),
            buf.as_mut_ptr() as *mut _,
            buf.len() as i32,
        );
    }
    let cstr = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr() as *const _) };
    cstr.to_str().ok().map(|s| s.to_ascii_lowercase())
}

/// Read a take's GUID as a braced string (e.g. `{XXXXXXXX-...}`).
/// Returns the take pointer formatted as a fallback if REAPER's
/// `GetSetMediaItemTakeInfo("GUID")` returns null (empty take, etc).
pub fn get_take_guid_string(low: &reaper_low::Reaper, take: MediaItemTake) -> String {
    let guid_ptr = unsafe {
        low.GetSetMediaItemTakeInfo(take.as_ptr(), c"GUID".as_ptr(), std::ptr::null_mut())
            as *const reaper_low::raw::GUID
    };
    if guid_ptr.is_null() {
        return format!("{:p}", take.as_ptr());
    }
    let g = unsafe { &*guid_ptr };
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.Data1,
        g.Data2,
        g.Data3,
        g.Data4[0],
        g.Data4[1],
        g.Data4[2],
        g.Data4[3],
        g.Data4[4],
        g.Data4[5],
        g.Data4[6],
        g.Data4[7]
    )
}

/// Set `B_PPITCH` (preserve-pitch-when-changing-rate flag) on a take.
/// REAPER stores BOOL attributes as `f64` 0.0/1.0 via the `_Value` API.
pub fn set_take_preserve_pitch(
    medium: &reaper_medium::Reaper,
    take: MediaItemTake,
    preserve: bool,
) {
    set_take_info_value(
        medium,
        take,
        TakeAttributeKey::PPitch,
        if preserve { 1.0 } else { 0.0 },
    );
}

/// Set a take's custom color. Pass `None` to clear the color (0).
/// REAPER expects `(native_color | 0x1000000)` to actually display; we
/// OR that flag in for callers.
pub fn set_take_color(medium: &reaper_medium::Reaper, take: MediaItemTake, color: Option<u32>) {
    let raw = match color {
        Some(c) => (c | 0x0100_0000) as f64,
        None => 0.0,
    };
    set_take_info_value(medium, take, TakeAttributeKey::CustomColor, raw);
}

/// Create a new PCM source from a file path. Returns `None` if REAPER
/// can't load the file. The returned source is owned by REAPER once
/// passed to `set_take_source`; otherwise the caller must free it via
/// `PCM_Source_Destroy` (not currently exposed).
pub fn pcm_source_create_from_file(
    low: &reaper_low::Reaper,
    path: &str,
) -> Option<reaper_medium::PcmSource> {
    let cpath = std::ffi::CString::new(path).ok()?;
    let raw = unsafe { low.PCM_Source_CreateFromFile(cpath.as_ptr()) };
    reaper_medium::PcmSource::new(raw)
}

/// Replace a take's PCM source. The previous source is leaked unless
/// the caller frees it explicitly.
pub fn set_take_source(
    low: &reaper_low::Reaper,
    take: MediaItemTake,
    source: reaper_medium::PcmSource,
) {
    unsafe {
        low.SetMediaItemTake_Source(take.as_ptr(), source.as_ptr());
    }
}

/// Read or write a media item's full state chunk.
///
/// Pass `value = None` to read the current chunk (returns it as a
/// String). Pass `Some(s)` to write — returns `Some(String::new())` on
/// success or `None` on rejection. The buffer scales as needed.
pub fn get_set_item_state(
    low: &reaper_low::Reaper,
    item: MediaItem,
    value: Option<&str>,
) -> Option<String> {
    match value {
        // Write path: allocate a writable null-terminated buffer.
        Some(s) => {
            let mut buf: Vec<u8> = s.as_bytes().to_vec();
            buf.push(0);
            let ok = unsafe {
                low.GetSetItemState(item.as_ptr(), buf.as_mut_ptr() as *mut i8, buf.len() as i32)
            };
            if ok { Some(String::new()) } else { None }
        }
        // Read path: try increasing buffer sizes until the chunk fits.
        None => {
            for size in [16 * 1024, 64 * 1024, 256 * 1024, 1024 * 1024] {
                let mut buf = vec![0u8; size];
                let ok = unsafe {
                    low.GetSetItemState(
                        item.as_ptr(),
                        buf.as_mut_ptr() as *mut i8,
                        buf.len() as i32,
                    )
                };
                if !ok {
                    return None;
                }
                let s = super::buffer::string_from_buffer(&buf);
                // If we filled within ~95% of the buffer assume truncation
                // and grow.
                if s.len() < (size * 95 / 100) {
                    return Some(s);
                }
            }
            None
        }
    }
}

// ─── Take markers ──────────────────────────────────────────────────────────

/// One take marker as returned by `GetTakeMarker`.
#[derive(Clone, Debug)]
pub struct TakeMarker {
    pub index: u32,
    pub name: String,
    pub source_position_seconds: f64,
    pub color: Option<u32>,
}

/// Number of take markers on the take.
pub fn count_take_markers(low: &reaper_low::Reaper, take: MediaItemTake) -> u32 {
    let n = unsafe { low.GetNumTakeMarkers(take.as_ptr()) };
    n.max(0) as u32
}

/// Read a single take marker by index. Returns `None` if `index` is out of
/// range (REAPER signals that with a negative position).
pub fn get_take_marker(
    low: &reaper_low::Reaper,
    take: MediaItemTake,
    index: u32,
) -> Option<TakeMarker> {
    let mut name_buf = vec![0i8; 256];
    let mut color: i32 = 0;
    let pos = unsafe {
        low.GetTakeMarker(
            take.as_ptr(),
            index as i32,
            name_buf.as_mut_ptr(),
            name_buf.len() as i32,
            &mut color,
        )
    };
    if pos < 0.0 {
        return None;
    }
    let cstr = unsafe { std::ffi::CStr::from_ptr(name_buf.as_ptr()) };
    let name = cstr.to_string_lossy().into_owned();
    Some(TakeMarker {
        index,
        name,
        source_position_seconds: pos,
        // REAPER returns color OR'd with 0x1000000 to mark "set"; mask it
        // out and surface None for "default".
        color: if color != 0 {
            Some((color as u32) & 0x00FF_FFFF)
        } else {
            None
        },
    })
}

/// All take markers on this take, in REAPER's enumeration order.
pub fn list_take_markers(low: &reaper_low::Reaper, take: MediaItemTake) -> Vec<TakeMarker> {
    let n = count_take_markers(low, take);
    (0..n)
        .filter_map(|i| get_take_marker(low, take, i))
        .collect()
}

/// Append a new take marker. Returns the new marker's index, or `None` on
/// failure.
pub fn add_take_marker(
    low: &reaper_low::Reaper,
    take: MediaItemTake,
    name: &str,
    source_position_seconds: f64,
    color: Option<u32>,
) -> Option<u32> {
    let cname = std::ffi::CString::new(name).ok()?;
    let mut pos = source_position_seconds;
    let mut color_val = color
        .map(|c| ((c & 0x00FF_FFFF) | 0x0100_0000) as i32)
        .unwrap_or(0);
    let color_ptr = if color.is_some() {
        &mut color_val as *mut i32
    } else {
        std::ptr::null_mut()
    };
    let idx = unsafe {
        low.SetTakeMarker(
            take.as_ptr(),
            -1, // -1 = append
            cname.as_ptr(),
            &mut pos,
            color_ptr,
        )
    };
    if idx < 0 { None } else { Some(idx as u32) }
}

/// Modify an existing take marker in place. Pass `None` for fields that
/// should keep their current value. Returns `true` if REAPER accepted the
/// call.
pub fn update_take_marker(
    low: &reaper_low::Reaper,
    take: MediaItemTake,
    index: u32,
    name: Option<&str>,
    source_position_seconds: Option<f64>,
    color: Option<Option<u32>>,
) -> bool {
    // Read-modify-write: REAPER's `SetTakeMarker` overwrites all fields, so
    // we read the existing marker and only touch the requested ones.
    let existing = match get_take_marker(low, take, index) {
        Some(m) => m,
        None => return false,
    };
    let new_name = name.unwrap_or(&existing.name);
    let new_pos = source_position_seconds.unwrap_or(existing.source_position_seconds);
    let new_color = match color {
        Some(c) => c,
        None => existing.color,
    };
    let cname = match std::ffi::CString::new(new_name) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let mut pos = new_pos;
    let mut color_val = new_color
        .map(|c| ((c & 0x00FF_FFFF) | 0x0100_0000) as i32)
        .unwrap_or(0);
    let color_ptr = if new_color.is_some() {
        &mut color_val as *mut i32
    } else {
        std::ptr::null_mut()
    };
    let result = unsafe {
        low.SetTakeMarker(
            take.as_ptr(),
            index as i32,
            cname.as_ptr(),
            &mut pos,
            color_ptr,
        )
    };
    result >= 0
}

/// Delete the take marker at the given index. Returns `true` on success.
pub fn delete_take_marker(low: &reaper_low::Reaper, take: MediaItemTake, index: u32) -> bool {
    unsafe { low.DeleteTakeMarker(take.as_ptr(), index as i32) }
}

/// A take's source format: sample rate and channel count.
///
/// Read from the `PCM_source`, not from the audio accessor, because the
/// accessor has no way to report it — which is why a caller wanting to
/// read a take at its native rate has to be told separately.
pub fn get_take_source_format(
    medium: &reaper_medium::Reaper,
    take: MediaItemTake,
) -> Option<(f64, u32)> {
    let source = unsafe { medium.get_media_item_take_source(take)? };
    let low = medium.low();
    let rate = unsafe { low.GetMediaSourceSampleRate(source.as_ptr()) };
    let channels = unsafe { low.GetMediaSourceNumChannels(source.as_ptr()) };
    if rate <= 0 || channels <= 0 {
        return None;
    }
    Some((rate as f64, channels as u32))
}

/// Get the source file path for a take (returns None for MIDI/empty takes).
pub fn get_take_source_file_path(
    medium: &reaper_medium::Reaper,
    take: MediaItemTake,
) -> Option<String> {
    let source = unsafe { medium.get_media_item_take_source(take)? };
    let low = medium.low();
    let mut buf = vec![0u8; 4096];
    unsafe {
        low.GetMediaSourceFileName(
            source.as_ptr(),
            buf.as_mut_ptr() as *mut i8,
            buf.len() as i32,
        );
    }
    let path = super::buffer::string_from_buffer(&buf);
    if path.is_empty() { None } else { Some(path) }
}
