---
version: 1
slug: "apps-desktop-src-rigs-rs"
primary_target: "apps/desktop/src/rigs.rs"
related_targets: ["apps/desktop/src/mobile_view.rs","apps/desktop/src/front_state.rs"]
---

# Front page (rig select) — surface brief

Scope: the app's front door (`RigMenu` in apps/desktop/src/rigs.rs), on iPad, iPhone and desktop. Mode: Operate.

Audience and job: a player picking up the rig — on stage (glance, one tap into the right rig, standing distance) and at home (choose a rig, go to libraries). Stage first, prep a level below.

Shows: ready rigs with what each will open on; the last-played rig lit with Resume; tonight's set; the studio this device plays (in-process or a remote engine). Unready rigs as one quiet line. Libraries a tap away.

## Direction contract

The user rejected the dealt Pedalboard after seeing it built; this replaces it (my own pick, built at their request for "a version that will actually be better").

THESIS: The front page is where you were and what you are playing tonight: one press resumes the rig you left, one press on a song opens the guitar on it. Refuses the launcher grid of equal rig cards.

OWN-WORLD: Signal Touch — near-black desk, zinc panes with 1px hairlines, system sans heavy and tight, tabular numerals; green only for what is playing (the live dot, the song up), the one solid rig-blue button per surface for Resume, stack colours only as chip swatches.

STORY: The player sees the rig they were on, what it plays and the song up, and presses Resume — or presses a song in tonight's set. Other ready rigs are one row away, each saying what it opens on; the studio link says where the sound comes from.

FIRST VIEWPORT: A 56px bar: wordmark, studio dot and name, Libraries at the right. Left 60%: the Resume pane (state dot and label, rig name at 44px, profile at 22px, the song up, the profile's stacks as chips, the blue Resume button), then the other ready rigs as 76px rows with chevrons, then a quiet Coming line at the foot. Right 40%: tonight's set as a run sheet — day and count, set name at 28px, one 60px row per song with number, name, key box, BPM, the song up washed green.

FORM: Resume + run sheet, position 1 on my ordered list. The surface roll ran (`concept-seed --scope surface --mode operate`, printed key 1b7ebb81, dealt 3, 7, 1); the decision page saved no hand record for it, and the dealt lead (Pedalboard) was built, shown and rejected by the user, so this form is my own pick, not a dealt card.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
