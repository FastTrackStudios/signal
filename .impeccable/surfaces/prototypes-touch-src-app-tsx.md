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

THESIS: The Signal app's own world, made for touch and polished — the user chose to keep "the same vibe and theme as the old one" (2026-10-06) over the floor-setlist world. Structure and touch layouts from the prototype stay; the look is the incumbent app's.

OWN-WORLD: The app's dark zinc greys (theme.rs: ground #0e0e11, panes, hairlines #222228/#2b2b31, ink #e4e4e7/#a1a1aa/#80808a), its stack colours (perform::folder_color) as tinted chips with a swatch, green for what plays (#22c55e), blue for what is picked (#1b2331 / #bfdbfe), primary blue buttons, the pick pressed into the bar (theme::PRESSED). System sans; tabular numerals; strikes for what is out or done.

STORY: Unchanged — tonight's set, give a section its sound, build presets from Core + Time + blocks with variations, find anything in the library, work the chain by touch in Routing, Undo anything.

FIRST VIEWPORT: Unchanged — bar (modes, profile, Undo, demo status); set sheet left 38%; the song up right with its sections and the picker; chain strip along the bottom.

FORM: Incumbent world inherited (user-pinned); the earlier roll (seed 85132dfe, Floor Setlist) is superseded.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
