---
version: 1
slug: "prototypes-touch-src-app-tsx"
primary_target: "prototypes/touch/src/App.tsx"
related_targets: []
---

# Signal Touch — management experience prototype

Scope: the whole management surface of the Signal guitar rig, touch-first —
the set (songs, their sections, the sound each section plays), presets
(composed of Core + Time + block picks, with variations), the library
browser/picker (profiles, patches, presets, block presets, songs, setlists),
routing, and the chain as one horizontally scrolled strip. Mode: Operate.
Targets: 11" iPad landscape (1194x834) and 13" Framework touchscreen
(3:2, 1504x1003); laptop with mouse too. Prototype in TypeScript on the rig's
real exported data, to be ported back to Dioxus/Blitz.

Audience/job: a worship guitarist, prep-led (couch/desk, iPad or laptop in
hand) but usable at the pedalboard in rehearsal. First jobs: build and run a
setlist (give each section its sound); make and tweak presets. Must not feel
like a DAW (dense, tiny, mouse-first) or a generic web app (cards, sidebars,
settings forms).

## Direction contract

THESIS: Every set, song and sound reads like the setlist gaffer-taped at your feet — huge, legible at arm's length, struck through as it's played. It refuses the dark modeller tile grid and the SaaS sidebar-and-cards.

OWN-WORLD: Bright copy-paper sheets on a cool off-white desk, one heavy grotesk (Archivo, wide/condensed axis) as the marker voice, hairline-ruled grid under everything. Fluoro gaffer-tape colours carry the stacks (Clean cyan, Crunch violet, Drive orange, Lead pink-red, Ambient green, Special yellow); one ballpoint ultramarine is reserved for the live signal path. Marker strikes and circles are the only hand-drawn marks. Tabular numerals for every value.

STORY: The player opens to tonight's set, sees where the band is, taps a song's section and gives it a sound from big taped lists; builds a preset from Core, Time and blocks with its variations; finds anything in the library grid; undoes what they don't like.

FIRST VIEWPORT: Top bar (56px): wordmark, mode tabs Set · Sounds · Library · Routing, profile, Undo, audio status. Left 38%: the set sheet — title in marker weight, songs as 64px rows (number, name, key tape, BPM), played ones struck, the current circled. Right 62%: the song up — title huge, key/BPM, its form as taped section tabs; the tapped section's sound picker beneath. Primary action: tap a section, tap a sound.

FORM: Floor Setlist (gaffer-taped Sharpie setlist), position 3 of 7 on the grounded list; seed key 85132dfe. Raises: hairline-ruled grid (design annual); nothing vanishes, it cancels (ticket wallet); one reserved colour for the live path (orienteering); tabular numerals (datamatics); whole-cell library grid with circled picks (circle catalog).

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
