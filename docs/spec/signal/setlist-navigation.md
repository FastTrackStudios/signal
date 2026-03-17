# Setlist Navigation

Runtime navigation through pre-loaded setlists using mute/unmute switching.

## Full-Load Strategy

r[signal.nav.full-load]
The full-load strategy pre-loads all songs as track groups in one REAPER
project. Each song is a `[S]` folder track with `[L]` variation tracks
inside. Switching between songs/sections is instant mute/unmute — no FX
loading or template instantiation needed at runtime.

r[signal.nav.full-load.performance]
Full-load switching is sub-millisecond (mute/unmute API calls) compared
to dynamic loading (~300ms per track for FX instantiation).

## Track Identification

r[signal.nav.track-prefix]
Track prefixes identify the role:
- `[S]` — Song folder track (top level)
- `[L]` — Layer/variation track (child of a song folder)
- `[R]` — Rig folder track
- `[E]` — Engine folder track

## Section Mapping

r[signal.nav.section-mapping]
Each song can have a section-to-variation mapping stored as ExtState on
the `[S]` folder track. Format: `P_EXT:FTS_SECTIONS:mapping` =
`"Intro:Clean,Verse:Clean,Chorus:Crunch,Solo:Lead"`.

r[signal.nav.section-mapping.fallback]
If no section mapping is stored, the navigation falls back to one
section per variation (each `[L]` track becomes its own section).

## Navigation Levels

### Song Navigation

r[signal.nav.song]
Next/previous song mutes the current song's entire folder group and
unmutes the target song's folder + first section's variation. Wraps around.

### Section Navigation

r[signal.nav.section]
Next/previous section follows the song's section order. Multiple sections
can map to the same variation track. When the last section of a song is
reached, advancing wraps to the first section of the next song.

### Variation Navigation

r[signal.nav.variation]
Direct variation cycling ignores section order and steps through the
unique `[L]` tracks within the active song. Useful for manual exploration.

## State Management

r[signal.nav.state]
Navigation state tracks:
- Current song index (which `[S]` folder is active)
- Current section index (position in the section list)
- The full song list with folder GUIDs, variation GUIDs, and section mappings

r[signal.nav.init]
Initialization scans the project for `[S]` folders, reads section mappings
from ExtState, mutes all tracks, then activates the first song's first section.

## Activation

r[signal.nav.activate]
Activating a section:
1. Mute the previously active song's folder + all variations
2. Unmute the target song's folder track
3. Unmute only the variation track that the target section maps to
4. Mute all other variation tracks in the target song

## Dynamic Loading (Alternative)

r[signal.nav.dynamic]
The dynamic loading path uses `SignalController::switch_to_variation(n)`
to swap FX state in-place on a single rig track. This is lower memory
but has switching latency. Controlled by signal_actions.rs.

r[signal.nav.dynamic.context]
The dynamic approach uses `ActiveContext` (Profile, Rig, or Song) to
determine what `switch_to_variation(n)` does:
- Profile context → activates the Nth patch
- Rig context → switches to the Nth scene
- Song context → jumps to the Nth section
