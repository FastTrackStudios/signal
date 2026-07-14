# Songs & Setlists

The top of the performance layer: a **Song** is a section-based structure that
recalls tones as it plays, and a **Setlist** is an ordered run of songs for a
service/show. This spec covers the data model and the **switch semantics** (what
happens on recall); [setlist-navigation.md](setlist-navigation.md) covers the
runtime strategies (full-load mute/unmute vs dynamic load). Reference:
`features/rigs/guitar/src/profiles.rs` (`SongDef`, `SetlistDef`,
`SetlistEntryDef`, `StackDefaultDef`).

## Songs

r[signal.song]
A song defines a section-based performance structure over a [profile](profile.md):
it carries a default key and tempo, its named sections (parts), and per-song stack
tuning. A song is a [Preset](hierarchy.md) (`signal.preset.kind` = Song).

r[signal.song.section]
A song has ordered **sections** (parts) — Intro, Verse, Chorus, Bridge, Solo… —
each mapping to the tone to activate: a patch reference or a direct rig-scene
reference. Selecting a section activates its tone.

r[signal.song.stack-default]
A song MAY re-point stacks for its duration via `StackDefaultDef { stack, patch }`
entries (`signal.stack.song-default`), so the footswitches land where the song
needs them. These apply on recall and are cleared when another song is recalled.

## Setlists

r[signal.setlist]
A setlist is an ordered sequence of songs for one performance, driving
next/previous navigation and (optionally) a fully pre-instantiated project. A
setlist is a [Preset](hierarchy.md) (`signal.preset.kind` within Rack/Setlist).

r[signal.setlist.entry]
Each setlist **entry** references a song plus optional per-set key and tempo
overrides (`SetlistEntryDef { song, key, bpm }`), so the same song can sit in
different keys/tempos on different sets without editing the song itself. An empty
key/zero bpm means "use the song's default".

## Switch semantics

r[signal.switch.song]
Recalling a song (next/previous or direct) is atomic: it (1) applies the setlist
entry's key/tempo override (or the song default), (2) applies the song's stack
defaults and **resets every stack cursor** to its landing patch
(`signal.stack.song-default`), and (3) activates the song's first section. It
MUST NOT re-host the running graph.

r[signal.switch.section]
Advancing/selecting a section activates that section's tone
(`signal.song.section`) through the live path. Sections wrap: advancing past the
last section moves to the next song's first section.

r[signal.switch.performance]
Switching is real-time-safe and fast enough to be inaudible mid-performance —
sub-millisecond under the full-load strategy (mute/unmute), bounded under dynamic
loading. The chosen strategy is a runtime concern (see
[setlist-navigation.md](setlist-navigation.md)), not a data-model one.
