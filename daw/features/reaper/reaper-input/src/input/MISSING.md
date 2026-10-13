# SWS Function Port Status

This document tracks the status of porting SWS (SWS Extension) functions from C++ to Rust for the FastTrackStudio REAPER extension.

## 📊 Implementation Status

### ✅ Fully Implemented (25+ functions)

#### Window Detection (`reaper_windows.rs`)
- ✅ `GetArrangeWnd()` - Arrange view window using control ID `0x000003E8`
- ✅ `GetRulerWndAlt()` - Ruler window using control ID `0x000003ED`
- ✅ `GetTransportWnd()` - Transport window (basic, no localization)
- ✅ `GetNotesView()` - MIDI notes view using control ID `0x000003E9`
- ✅ `GetPianoView()` - MIDI piano view using control ID `0x000003EB`
- ✅ `GetTcpWnd()` - TCP window detection with container support
- ✅ `GetTcpTrackWnd()` - Get specific track window in TCP
- ✅ `GetMixerWnd()` - Mixer window detection
- ✅ `GetMixerMasterWnd()` - Mixer master window detection
- ✅ `GetMcpWnd()` - MCP window detection with container support
- ✅ `SearchChildren()` - Window enumeration helper
- ✅ `FindInReaper()` / `FindInReaperDockers()` / `FindFloating()` - Window search helpers

#### Track/Envelope Detection (`reaper_windows.rs`)
- ✅ `HwndToTrack()` - Convert HWND to MediaTrack* (TCP/MCP with spacer support)
- ✅ `HwndToEnvelope()` - Convert HWND to TrackEnvelope* (TCP)
- ✅ `GetTrackSpacerSize()` - Get track spacer size (simplified, returns 0 for now)

#### Arrange View Detection (`utils.rs`)
- ✅ `GetTrackOrEnvelopeFromY()` - Get track/envelope from Y coordinate in TCP
- ✅ `GetTrackAreaFromY()` - Helper for track area detection
- ✅ `GetTrackFromY()` - Helper for track detection (more specific)
- ✅ `IsPointInArrange()` - Check if point is in arrange view
- ✅ `PositionAtArrangePoint()` - Get position at arrange point (uses REAPER APIs)
- ✅ `TranslatePointToArrangeScrollY()` - Translate point to arrange scroll Y (uses CoolSB_GetScrollInfo)

#### Height Calculations (`utils.rs`)
- ✅ `GetTrackHeight()` - Base track height function
- ✅ `GetTrackHeightWithSpacer()` - Track height with spacer support
- ✅ `GetItemHeight()` - Item height calculation (structure complete, simplified calculation)
- ✅ `GetTakeHeight()` - Take height calculation (basic)
- ✅ `GetMasterTcpGap()` - Master track TCP gap constant (TCP_MASTER_GAP = 5)

#### MIDI Utilities (`midi_utils.rs`)
- ✅ `IsOpenInInlineEditor()` - Check if MIDI take is open in inline editor
- ✅ `IsMidiNoteBlack()` - Check if MIDI note is black key

#### Mouse Context (`mouse_context.rs`)
- ✅ `DetermineMouseContext()` - Main mouse context detection
- ✅ `GetContextFromMousePosition()` - Get context from mouse position

#### REAPER API Functions (used directly)
- ✅ `GetItemFromPoint()` - REAPER API for getting item at point
- ✅ `GetSet_ArrangeView2()` - REAPER API for arrange view bounds (used in `position_at_arrange_point`)
- ✅ `GetHZoomLevel()` - REAPER API for horizontal zoom (used in `position_at_arrange_point`)

---

## 🟡 Partially Implemented / Simplified

### Height Calculations
- **`GetItemHeight()`** - Structure matches SWS, but height calculation is simplified
  - **Current**: Returns default 20px
  - **Needed**: Full implementation using `I_FREEMODE`, `I_HEIGHTOVERRIDE`, and other item properties
  - **Location**: `utils.rs::get_item_height_full()`

- **`GetTrackSpacerSize()`** - Basic structure, returns 0
  - **Current**: Returns 0 (no spacer)
  - **Needed**: Full implementation checking `ConfigVar("trackgapmax")`, `HasTrackSpacerBefore()`, etc.
  - **Location**: `reaper_windows.rs::get_track_spacer_size()`

- **`GetTrackHeight()`** - Missing `topGap` and `bottomGap` calculation
  - **Current**: Returns 0 for both gaps
  - **Needed**: Proper gap calculation from track properties
  - **Location**: `utils.rs::get_track_height()`

### Coordinate Conversion
- **`TranslatePointToArrangeScrollY()`** - Basic implementation
  - **Current**: Returns client Y coordinate
  - **Needed**: Full scroll position support using `CF_GetScrollInfo()` or equivalent
  - **Location**: `utils.rs::translate_point_to_arrange_scroll_y()`

---

## 🔴 Not Implemented

### Advanced Envelope Detection
- **`BR_Envelope` class** - Entire envelope manipulation class from `BR_EnvelopeUtil.cpp`
  - **Why needed**: Detect envelope points and segments under mouse
  - **Complexity**: Very High - entire class
  - **Key methods**:
    - `IsMouseOverEnvelopeLine()` - Check if mouse over envelope line
    - `IsMouseOverEnvelopeLineTrackLane()` - Check envelope in track lane
    - `IsMouseOverEnvelopeLineTake()` - Check envelope in take
    - `ValueAtPosition()` - Get envelope value at time
    - `NormalizedDisplayValue()` - Convert value to display coordinate

### Advanced MIDI Editor Detection
- **`BR_MidiEditor` class** - Entire MIDI editor state class from `BR_MidiUtil.cpp`
  - **Why needed**: Detect CC lanes, note rows, piano roll segments
  - **Complexity**: Very High - entire class
  - **Key methods**:
    - `GetCCLanesFullheight()` - Get total CC lane height
    - `GetCCLaneHeight(int i)` - Get height of specific CC lane
    - `GetCCLane(int i)` - Get CC lane number
    - `CountCCLanes()` - Count visible CC lanes
    - `GetVZoom()` / `GetVPos()` / `GetHZoom()` - Zoom and position queries
    - `GetPianoRoll()` / `GetNoteshow()` - Display mode queries
    - `GetUsedNamedNotes()` - Get visible note rows

### Stretch Marker Detection
- **Stretch marker APIs** - Various helper functions
  - **Why needed**: Detect if mouse is over a stretch marker
  - **Complexity**: Medium - requires stretch marker iteration
  - **Functions needed**:
    - `FindClosestStretchMarker()`
    - `GetTakeNumStretchMarkers()`
    - `GetTakeStretchMarker()`
    - `IsStretchMarkerVisible()`

### Other Missing Functions
- None - all basic functions are implemented!

- ~~**`CF_GetScrollInfo()` or equivalent**~~ - ✅ **COMPLETED**
  - **Status**: Using `CoolSB_GetScrollInfo()` from REAPER low-level API
  - **Location**: `utils.rs::translate_point_to_arrange_scroll_y()`

---

## 📁 File Organization

### `reaper_windows.rs`
Window detection and HWND-to-object conversion functions:
- Window detection (arrange, ruler, transport, TCP, MCP, mixer, MIDI views)
- `HwndToTrack()` / `HwndToEnvelope()`
- Window search helpers

### `utils.rs`
Track/item/take height calculations and arrange view detection:
- Height calculations (`GetTrackHeight`, `GetTrackHeightWithSpacer`, `GetItemHeight`, `GetTakeHeight`)
- Arrange view detection (`GetTrackOrEnvelopeFromY`, `GetTrackAreaFromY`, `GetTrackFromY`)
- Coordinate conversion (`IsPointInArrange`, `PositionAtArrangePoint`, `TranslatePointToArrangeScrollY`)

### `midi_utils.rs`
MIDI-specific utilities:
- Inline editor detection
- MIDI note utilities

### `mouse_context.rs`
High-level mouse context detection:
- Main context determination
- Mouse position-based context detection

---

## 🎯 Current Capabilities

### ✅ What Works Now
- **Window Detection**: All major REAPER windows can be detected (arrange, TCP, MCP, mixer, MIDI editor views)
- **Track Detection**: Can detect which track mouse is over in TCP/MCP using HWND or Y coordinate
- **Envelope Detection**: Can detect which envelope mouse is over in TCP
- **Arrange View**: Can detect tracks/envelopes from Y coordinate in arrange view
- **Coordinate Conversion**: Can convert screen coordinates to time positions in arrange view
- **MIDI Editor**: Can detect MIDI editor windows and inline editor status

### 🔴 What's Missing for Full Feature Parity
- **Envelope Points/Segments**: Cannot detect specific envelope points or segments under mouse
- **MIDI CC Lanes**: Cannot detect which CC lane or note row mouse is over
- **Stretch Markers**: Cannot detect stretch markers under mouse
- **Advanced Item Height**: Item height calculation is simplified (returns default 20px)

---

## 🚀 Next Steps

### High Priority (for basic functionality)
1. ✅ **Window Detection** - **DONE**
2. ✅ **Track/Envelope Detection** - **DONE**
3. ✅ **Arrange View Detection** - **DONE**
4. 🟡 **Item Height Calculation** - Structure done, needs full implementation

### Medium Priority (for enhanced features)
1. 🔴 **BR_Envelope class** - For envelope point/segment detection
2. 🔴 **BR_MidiEditor class** - For CC lane/note row detection

### Low Priority (nice to have)
1. 🔴 **Stretch marker detection** - For stretch marker interaction
2. 🟡 **Full spacer size calculation** - For accurate track spacing

---

## 📝 Notes

- Most critical functions for basic mouse context detection are **complete**
- Advanced features (envelope points, MIDI CC lanes) require porting entire SWS classes
- Some functions use simplified implementations that work but may not match SWS exactly
- REAPER API functions are used directly where available (no need to port)
