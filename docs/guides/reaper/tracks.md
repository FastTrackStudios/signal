---
title: Tracks
kind: input
type: input
category: tracks
---

# Tracks

Track work is where the which-key layer shines: one plain chord for the raw insert, and two prefix menus — the track manager and the create-track family. (New to prefix menus? Read [[Input System|the input layer]] first.)

## The raw insert

- `kbd:@40001` — Insert a new empty track below the selection, exactly like stock REAPER.

## The track manager (`kbd:n` menu)

Press `kbd:n` to open the Track Manager menu. The overlay lists every follow-up key — each letter is mnemonic for the entity being added.

- `kbd:n n` — New blank track.
- `kbd:@40702` — Duplicate the selected tracks.
- `kbd:@_FTS_SESSION_TRACK_MANAGER_ADD_ARRANGEMENT` — Add an arrangement.
- `kbd:@_FTS_SESSION_TRACK_MANAGER_ADD_CHANNEL` — Add a channel.
- `kbd:@_FTS_SESSION_TRACK_MANAGER_ADD_LAYER` — Add a layer.
- `kbd:@_FTS_SESSION_TRACK_MANAGER_ADD_MULTI_MIC` — Add a multi-mic group.
- `kbd:@_FTS_SESSION_TRACK_MANAGER_ADD_PERFORMER` — Add a performer.

```gif
tracks-manager-menu
Press `n` and the which-key overlay lists every follow-up key; tap a letter to add that entity.
```

## Create categorized tracks (`kbd:<S-n>` menu)

Press `kbd:<S-n>` to create fully-configured session tracks — named, routed, and colored for their role. Keep Shift held and tap letters to create several in a row.

- `kbd:@_FTS_SESSION_CREATE_NEW_DRUM_KIT` — Drum kit.
- `kbd:@_FTS_SESSION_CREATE_NEW_LEAD_VOCALS` — Lead vocals.
- `kbd:@_FTS_SESSION_CREATE_NEW_ELECTRIC_GUITAR` — Electric guitar.
- `kbd:@_FTS_SESSION_CREATE_NEW_BASS_GUITAR` — Bass guitar.
- `kbd:@_FTS_SESSION_CREATE_NEW_PIANO` — Piano.

Branches nest: `kbd:<S-n> s` opens a synth submenu — arp, bass, lead, pad:

- `kbd:@_FTS_SESSION_CREATE_NEW_SYNTH_ARP` — Synth arp.

```gif
tracks-create-category
Hold Shift and tap letters from the `<S-n>` menu to spin up named, routed, colored tracks in a row.
```

Next: get a take down with [[Transport]] and comp it in [[Recording]].
