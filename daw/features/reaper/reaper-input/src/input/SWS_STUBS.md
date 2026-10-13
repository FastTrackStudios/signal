# SWS Function Stubs - Implementation Status

This document tracks which SWS-specific functions from `BR_MouseUtil.cpp` and related files still need to be implemented.

## ✅ Already Implemented (using REAPER APIs)

- ✅ `GetItemFromPoint()` - Using `reaper_rs` low-level API
- ✅ `IsOpenInInlineEditor()` - Using `PCM_SOURCE_EXT_INLINEEDITOR` via medium-level API
- ✅ `GetTakeHeight()` - Basic implementation in `utils.rs` (simplified)
- ✅ `GetArrangeWnd()` - **FULLY IMPLEMENTED** using `GetDlgItem` with control ID `0x000003E8` (platform-independent)
- ✅ `GetRulerWndAlt()` - **FULLY IMPLEMENTED** using `GetDlgItem` with control ID `0x000003ED` (platform-independent)
- ✅ `GetNotesView()` - **FULLY IMPLEMENTED** using `GetDlgItem` with control ID `0x000003E9` (platform-independent)
- ✅ `GetPianoView()` - **FULLY IMPLEMENTED** using `GetDlgItem` with control ID `0x000003EB` (platform-independent)

## 🔴 Critical SWS Functions Still Needed

### Window Detection

1. ~~**`GetRulerWndAlt()`**~~ - ✅ **IMPLEMENTED** in `reaper_windows.rs`
   - **Location**: `reaper_windows.rs::get_ruler_wnd()`
   - **Status**: ✅ Fully implemented using `GetDlgItem` with control ID
   - **Used in**: Ruler detection in `mouse_context.rs`

2. **`GetTransportWnd()`** - Get transport window HWND
   - **Location**: `reaper_windows.rs::get_transport_wnd()`
   - **Status**: ✅ **IMPLEMENTED** (simplified - uses "Transport" name, no localization yet)
   - **Used in**: Transport window detection in `mouse_context.rs`
   - **SWS Source**: `BR_Util.cpp::GetTransportWnd()`
   - **Note**: Currently searches for "Transport" window name. Full implementation would use `__localizeFunc("Transport", "DLG_188", 0)` for localization support

3. ~~**`GetNotesView(HWND midiEditor)`**~~ - ✅ **IMPLEMENTED** in `reaper_windows.rs`
   - **Location**: `reaper_windows.rs::get_notes_view()`
   - **Status**: ✅ Fully implemented using `GetDlgItem` with control ID
   - **Used in**: Determining MIDI editor subview type (notes vs piano)

4. ~~**`GetPianoView(HWND midiEditor)`**~~ - ✅ **IMPLEMENTED** in `reaper_windows.rs`
   - **Location**: `reaper_windows.rs::get_piano_view()`
   - **Status**: ✅ Fully implemented using `GetDlgItem` with control ID
   - **Used in**: Determining MIDI editor subview type (notes vs piano)

### Track/Envelope Detection

5. **`HwndToTrack(HWND hwnd, int* hwndContext, POINT ptScreen)`** - Convert HWND to MediaTrack*
   - **Location**: `mouse_context.rs` (TCP/MCP detection section)
   - **Status**: Fallback to window title matching
   - **Used in**: TCP/MCP track detection
   - **SWS Source**: `BR_Util.cpp::HwndToTrack()`
   - **Returns**: `MediaTrack*` and context flags (TCP, MCP, Spacer)

6. **`HwndToEnvelope(HWND hwnd, POINT ptScreen)`** - Convert HWND to TrackEnvelope*
   - **Location**: `mouse_context.rs` (TCP/MCP detection section)
   - **Status**: Not implemented
   - **Used in**: Envelope detection in TCP
   - **SWS Source**: `BR_Util.cpp::HwndToEnvelope()`

### Arrange View Helpers

7. **`GetTrackOrEnvelopeFromY(int y, ...)`** - Get track or envelope from Y coordinate
   - **Location**: `mouse_context.rs` (arrange view detection)
   - **Status**: Not implemented
   - **Used in**: Determining which track/envelope the mouse is over in arrange view
   - **SWS Source**: `BR_MouseUtil.cpp::BR_MouseInfo::GetTrackOrEnvelopeFromY()`
   - **Dependencies**: Needs `GetTrackAreaFromY()`, `GetTrackHeightWithSpacer()`, envelope TCP info

8. **`GetTrackHeightWithSpacer(MediaTrack* track, ...)`** - Get track height including spacer
   - **Location**: `utils.rs::get_track_height_with_spacer()` (simplified)
   - **Status**: Basic implementation (missing spacer calculation)
   - **Used in**: Track height calculations
   - **SWS Source**: `BR_Util.cpp::GetTrackHeightWithSpacer()`
   - **TODO**: Add spacer size calculation

9. **`GetItemHeight(MediaItem* item, int* offsetY, ...)`** - Get item height
   - **Location**: `utils.rs::get_item_height()` (simplified)
   - **Status**: Returns default height (20px)
   - **Used in**: Item height calculations for take height
   - **SWS Source**: `BR_Util.cpp::GetItemHeight()`
   - **TODO**: Implement proper item height calculation using `I_FREEMODE` and other factors

10. **`GetTrackFromY(int y, ...)`** - Get track from Y coordinate
    - **Location**: Not directly implemented
    - **Status**: Would be used by `GetTrackOrEnvelopeFromY()`
    - **SWS Source**: `BR_MouseUtil.cpp::GetTrackFromY()`

11. **`GetTrackAreaFromY(int y, ...)`** - Get track area from Y coordinate
    - **Location**: Not directly implemented
    - **Status**: Would be used by `GetTrackOrEnvelopeFromY()`
    - **SWS Source**: `BR_MouseUtil.cpp::GetTrackAreaFromY()`

### Arrange View Coordinate Conversion

12. **`GetSet_ArrangeView2(...)`** - Get/set arrange view time range
    - **Location**: `mouse_context.rs::position_at_arrange_point()` (TODO)
    - **Status**: Returns placeholder values
    - **Used in**: Converting screen coordinates to time positions
    - **SWS Source**: SWS extension function

13. **`GetHZoomLevel()`** - Get horizontal zoom level
    - **Location**: `mouse_context.rs::position_at_arrange_point()` (TODO)
    - **Status**: Returns placeholder value (1.0)
    - **Used in**: Coordinate conversion
    - **SWS Source**: SWS extension function

14. **`CF_GetScrollInfo()` or equivalent** - Get scroll position
    - **Location**: `mouse_context.rs::translate_point_to_arrange_scroll_y()` (TODO)
    - **Status**: Uses client Y directly (missing scroll offset)
    - **Used in**: Converting screen Y to arrange view Y
    - **SWS Source**: `cfillion/cfillion.cpp::CF_GetScrollInfo()` (SWS dependency)

### Envelope Detection

15. **`BR_Envelope` class** - Envelope manipulation and querying
    - **Location**: `mouse_context.rs` (envelope point/segment detection)
    - **Status**: Not implemented
    - **Used in**: 
      - Envelope point detection
      - Envelope segment detection
      - Envelope value queries
    - **SWS Source**: `BR_EnvelopeUtil.cpp` (entire class)
    - **Key Methods Needed**:
      - `IsMouseOverEnvelopeLine()` - Check if mouse is over envelope line
      - `IsMouseOverEnvelopeLineTrackLane()` - Check envelope in track lane
      - `IsMouseOverEnvelopeLineTake()` - Check envelope in take
      - `ValueAtPosition()` - Get envelope value at time
      - `NormalizedDisplayValue()` - Convert value to display coordinate

### MIDI Editor Detection

16. **`BR_MidiEditor` class** - MIDI editor state querying
    - **Location**: `mouse_context.rs` (MIDI editor segment detection)
    - **Status**: Not implemented
    - **Used in**:
      - CC lane detection
      - Note row detection
      - Piano roll mode detection
      - MIDI editor zoom/position queries
    - **SWS Source**: `BR_MidiUtil.cpp` (entire class)
    - **Key Methods Needed**:
      - `GetCCLanesFullheight()` - Get total CC lane height
      - `GetCCLaneHeight(int i)` - Get height of specific CC lane
      - `GetCCLane(int i)` - Get CC lane number
      - `CountCCLanes()` - Count visible CC lanes
      - `GetVZoom()` - Get vertical zoom
      - `GetVPos()` - Get vertical position
      - `GetHZoom()` - Get horizontal zoom
      - `GetStartPos()` - Get start position
      - `GetPianoRoll()` - Get piano roll mode
      - `GetNoteshow()` - Get note show mode
      - `GetUsedNamedNotes()` - Get visible note rows
      - `GetTimebase()` - Get timebase mode
      - `GetActiveTake()` - Get active take

### Stretch Marker Detection

17. **Stretch marker APIs** - Various functions for stretch marker detection
    - **Location**: `mouse_context.rs` (stretch marker detection TODO)
    - **Status**: Not implemented
    - **Used in**: Detecting if mouse is over a stretch marker
    - **SWS Source**: `BR_MouseUtil.cpp::IsMouseOverStretchMarker()`
    - **Dependencies**: 
      - `FindClosestStretchMarker()`
      - `GetTakeNumStretchMarkers()`
      - `GetTakeStretchMarker()`
      - `IsStretchMarkerVisible()`

## 📊 Implementation Priority

### High Priority (Core Functionality)
1. ~~`GetRulerWndAlt()`~~ - ✅ **DONE** - Needed for ruler detection
2. ~~`GetTransportWnd()`~~ - ✅ **DONE** (basic - localization can be added later) - Needed for transport detection
3. `HwndToTrack()` - Needed for TCP/MCP detection
4. `GetTrackOrEnvelopeFromY()` - Needed for arrange view track detection

### Medium Priority (Enhanced Detection)
5. `HwndToEnvelope()` - Needed for envelope detection
6. ~~`GetNotesView()` / `GetPianoView()`~~ - ✅ **DONE** - Needed for MIDI editor subview detection
7. `GetSet_ArrangeView2()` / `GetHZoomLevel()` - Needed for accurate coordinate conversion
8. `GetTrackHeightWithSpacer()` - Complete implementation (add spacer)
9. `GetItemHeight()` - Complete implementation

### Low Priority (Advanced Features)
10. `BR_Envelope` class - Needed for envelope point/segment detection
11. `BR_MidiEditor` class - Needed for CC lane and note row detection
12. Stretch marker APIs - Needed for stretch marker detection

## 🔧 Implementation Options

### Option 1: FFI Bindings to SWS
- Create Rust FFI bindings to SWS library
- Requires SWS to be loaded as a dependency
- Most accurate but adds external dependency

### Option 2: Re-implement Using REAPER APIs
- Use REAPER's public APIs where possible
- More portable but may be less accurate
- Some functions may not be possible without SWS

### Option 3: Hybrid Approach
- Use REAPER APIs where available
- FFI to SWS for functions that can't be re-implemented
- Best of both worlds but more complex

## 📝 Notes

- `GetItemFromPoint()` is now available in REAPER API v5.975+ (✅ implemented)
- `IsOpenInInlineEditor()` uses `PCM_SOURCE_EXT_INLINEEDITOR` (✅ implemented)
- `GetArrangeWnd()` has a basic implementation but SWS version is more robust
- Many functions depend on chunk parsing or internal REAPER state that's not exposed via public API
