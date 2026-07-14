# Profiles & Stacks

The performance-configuration layer over the [hierarchy](hierarchy.md): a
**Profile** is a curated pool of tones plus the **patches** that recall them, and
**Stacks** are the footswitch groupings that step through those patches live.
Reference: `features/rigs/guitar/src/profiles.rs` (`ProfileDef`, `PresetDef`,
`PatchDef`, `StackDef`).

## Profiles

r[signal.profile]
A profile is a named collection of **patches** — recallable tone configurations —
built over a rig. It bundles a preset pool, the patches that point into it, and
the footswitch stacks that group them, and is itself a [Preset](hierarchy.md)
(`signal.preset.kind` = Profile).

r[signal.profile.preset-pool]
A profile owns a **preset pool**: a small set of named presets (e.g. amp/tone
captures) that its patches reference by name. Several patches MAY share one pool
preset and differ only by their overrides, so the pool stays small.

r[signal.profile.patch]
A **patch** is one recallable tone: it names a pool preset (or targets a specific
hierarchy level directly) plus per-patch adjustments. Each patch targets a level
of the hierarchy.

r[signal.profile.patch.target]
A patch target is one of: `BlockSnapshot` (swap a block's state), `ModuleSnapshot`
(swap a module's state), `LayerSnapshot` (swap a layer's chain), `EngineScene`
(switch an engine's scene), `RigScene` (switch the rig's scene), or `Patch`
(cross-reference another patch).

r[signal.profile.override]
A patch MAY carry **overrides** on top of its referenced preset — a level trim/
boost and a set of parameter writes (`{ module, block, param, value }`) — so two
patches share a preset yet differ (e.g. "Clean" vs "Clean Verb" = same preset +
a reverb-mix override). Overrides apply after the preset loads.

r[signal.profile.activation]
Activating a patch resolves its target and applies the state change via the live
path: dynamic loading writes FX parameters in place; full-load switching
mutes/unmutes track groups. Activation MUST NOT re-host the running graph.

## Stacks

r[signal.stack]
A **stack** is a footswitch grouping: a name plus an ordered **rotation** of
patch references. A stack is the live control a performer steps — one footswitch
= one stack — abstracting "which patches this switch cycles" from the patches
themselves.

r[signal.stack.cursor]
Each stack holds a **cursor** into its rotation. Engaging the stack advances (or
selects) the cursor, activating that patch (`signal.profile.activation`). Cursor
state is per-stack and runtime; recall resets it (`signal.stack.song-default`).

r[signal.stack.song-default]
A Song MAY re-point a stack's **landing patch** for the duration of that song (a
`StackDefaultDef { stack, patch }`), so the same footswitch is dialed per song
(e.g. the Clean stack lands on "Clean Verb" in one song). Recalling a song
applies its stack defaults and **resets every stack cursor** to its landing patch.
See [song-setlist.md](song-setlist.md).
