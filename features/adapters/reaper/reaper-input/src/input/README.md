# FTS-Input

A vim-like key sequence system for REAPER, inspired by reaper-keys.

## Architecture

### Components

1. **State Management** (`state.rs`)
   - Tracks current key sequence, mode, and context
   - Uses in-memory storage (TODO: persist to REAPER's external state)
   - Manages macro recording/playback state

2. **Bindings** (`bindings.rs`)
   - Defines key sequence → action mappings
   - Organized by action type (timeline_motion, timeline_operator, etc.)
   - Supports context-specific bindings (main, midi, global)

3. **Matcher** (`matcher.rs`)
   - Matches key sequences against bindings
   - Builds commands from matched sequences
   - Provides completions for partial sequences

4. **Executor** (`executor.rs`)
   - Executes commands by running REAPER actions
   - Handles action composition (e.g., timeline_operator + timeline_motion)
   - Supports repetition counts

5. **Handler** (`handler.rs`)
   - Main entry point using `TranslateAccel` to intercept keypresses
   - Converts key codes to string representations
   - Processes keypresses and builds sequences

## How It Works

1. **Keypress Interception**: Uses `plugin_register_add_accelerator_register()` with `TranslateAccel` to intercept keyboard input BEFORE REAPER processes it

2. **Sequence Building**: Each keypress is converted to a string (e.g., "t", "L") and appended to the current sequence (e.g., "tL")

3. **Matching**: The sequence is matched against bindings organized by:
   - Action type (timeline_motion, timeline_operator, etc.)
   - Context (main, midi, global)
   - Mode (normal, visual_timeline, visual_track)

4. **Command Building**: When a match is found, a Command is built with:
   - Action sequence (e.g., ["timeline_operator", "timeline_motion"])
   - Action keys (e.g., ["PlayAndLoop", "NextMeasure"])

5. **Execution**: The command is executed, which may involve:
   - Running multiple REAPER actions in sequence
   - Special composition logic (e.g., operator acts on motion)
   - Macro recording/playback

## Current Status

✅ **Implemented:**
- TranslateAccel integration for keypress interception
- State management (in-memory)
- Bindings system
- Command matcher
- Command executor
- Basic key code to string conversion
- Handler registration

🚧 **TODO:**
- Action name → REAPER command ID mapping
- Action composition logic (timeline_operator + timeline_motion)
- Macro recording/playback
- Visual feedback UI for completions
- More bindings (expand from defaults)
- Mode transitions
- Context detection improvements (MIDI editor detection)
- Text focus detection (don't intercept when typing)
- State persistence to REAPER's external state
- Timeout/reset for incomplete sequences

## Usage

The system is automatically registered when the extension loads (in `app.rs::initialize()`).

To add bindings, edit `bindings.rs` and extend the `Bindings::default()` function.

## Key Differences from reaper-keys

- **Uses TranslateAccel**: Intercepts at keypress level (like FTS-Extensions)
- **Rust-based**: Type-safe, compiled code
- **Integrated**: Works alongside existing FastTrackStudio systems
- **Action names**: Currently uses action names that need to be mapped to REAPER command IDs
