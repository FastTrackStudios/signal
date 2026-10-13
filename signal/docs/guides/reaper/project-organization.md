---
title: Project Organization
kind: concept
type: concept
---

# Project organization

A session you can navigate blindfolded is a session you can work in fast. FastTrackStudio bakes structure into the create-track family and the section regions, so organization is a byproduct of tracking, not a cleanup pass afterward.

## Tracks that name and route themselves

Don't insert blank tracks and rename them. Use the `kbd:<S-n>` create-track menu (see [[Tracks]]) — each entry drops a track that's already named, colored, and routed for its role:

- `kbd:@_FTS_SESSION_CREATE_NEW_LEAD_VOCALS` — a lead-vocal track, colored and bused for vocals.
- `kbd:@_FTS_SESSION_CREATE_NEW_ELECTRIC_GUITAR` — an electric-guitar track in the guitar group.
- `kbd:@_FTS_SESSION_CREATE_NEW_MIX_BUS` — the mix-bus hierarchy (MIX BUS / INSTRUMENTAL / vocal buses).

Keep Shift held and tap letters to build a whole session's worth of tracks in seconds.

## Buses and folders

The create menu routes categories into their buses automatically, so a mix bus tree exists before you've touched the mixer. Add structure as you go from the `kbd:n` track-manager menu:

- `kbd:@_FTS_SESSION_TRACK_MANAGER_ADD_PERFORMER` — group everything one performer plays.
- `kbd:@_FTS_SESSION_TRACK_MANAGER_ADD_MULTI_MIC` — a multi-mic group (e.g. a guitar cab with two mics).

```gif
organization-create-tracks
**Structure falls out of creation.**

- `kbd:<S-n>` tracks arrive named, colored, and routed to the right bus.
- No post-tracking cleanup pass — the session is organized as it grows.
```

## Song structure as regions

Mark sections once and every navigation speaks in verses and choruses (see [[markers-regions|Markers & regions]]):

- `kbd:@_FTS_SESSION_INSERT_CHORUS_REGION` — drop a Chorus region over the time selection.
- `kbd:@_FTS_SESSION_INSERT_VERSE_REGION` — a Verse region.

Colored, named section regions turn the timeline into a map — jump straight to the bridge, loop the chorus, or arrange by dragging sections.

## The payoff

A well-organized session isn't tidiness for its own sake — it's speed. Consistent names and colors mean your mix template lands on the right tracks, section regions make [[comping-takes|comping]] and arrangement navigation instant, and the next person (or the future you) can open the project and immediately know what's what.
