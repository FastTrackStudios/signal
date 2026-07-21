---
title: Markers & Regions
kind: input
type: input
category: markers
---

# Markers & regions

Song structure is data. Instead of hand-typing region names, the marker layer gives you one menu to *insert* a named song-section region and another to *jump* straight to it — so navigation and arrangement both speak in verses and choruses. (Prefix menus new to you? Start with [[Input System|the input layer]].)

## Insert a section region (`kbd:<S-m>` menu)

Select a time range, then press `kbd:<S-m>` and tap the section's letter. Keep Shift held to fire several in a row.

- `kbd:@_FTS_SESSION_INSERT_CHORUS_REGION` — Insert a Chorus (CH) region.
- `kbd:@_FTS_SESSION_INSERT_VERSE_REGION` — Insert a Verse (VS) region.
- `kbd:@_FTS_SESSION_INSERT_PRE_CHORUS_REGION` — Insert a Pre-Chorus (PRE-CH) region.
- `kbd:@_FTS_SESSION_INSERT_BRIDGE_REGION` — Insert a Bridge (BR) region.
- `kbd:@_FTS_SESSION_INSERT_INTRO_REGION` — Insert an Intro (IN) region.

```gif
markers-insert-region
Select a range, press `<S-m>`, and tap a letter to drop a named, colored section region.
```

## Jump to a section (`kbd:m` menu)

Press `kbd:m` and tap the same letters to move the edit cursor to that section — the fastest way around a laid-out song.

- `kbd:@_FTS_SESSION_GOTO_CHORUS_REGION` — Go to the Chorus.
- `kbd:@_FTS_SESSION_GOTO_VERSE_REGION` — Go to the Verse.
- `kbd:@_FTS_SESSION_GOTO_BRIDGE_REGION` — Go to the Bridge.

## Structural markers

Nested under both menus is a structural-marker submenu for the song's skeleton:

- `kbd:@_FTS_SESSION_INSERT_START_MARKER` — Drop a =START marker.
- `kbd:@_FTS_SESSION_INSERT_END_MARKER` — Drop an =END marker.
- `kbd:@_FTS_SESSION_INSERT_COUNT_IN_MARKER` — Drop a COUNT-IN marker.

A plain marker at the cursor is still just `kbd:@40157`; hop between plain markers with the `,` / `.` keys from [[Transport]].
