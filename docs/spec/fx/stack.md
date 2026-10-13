# Stacks — chaining styles inside one plugin

An FTS processor (EQ, compressor, saturator, delay, reverb — any kind that
offers several *styles*: models, profiles, algorithms) can run **more than
one style at once**. The unit of stacking is a **stage** (one style with its
own complete parameter set); a **stack** arranges stages in **lanes** that
run in parallel, each lane a serial chain of stages, summed at the output.
This is what lets one EQ instance be "Pro-Q-style subtractive → Pultec
boost", one compressor be "five flavours of drum-bus compression balanced
together", one delay be "all the vocal delays, in parallel".

Today `delay_dsp::DualDelay` / `reverb_dsp::DualReverb` (`DualRouting`:
Single / Series12 / Series21 / Parallel / Split) are the two-stage special
case of this, exposed only in `signal-fx`; the comp and saturate have one
stage. This spec generalizes that to N stages, every kind, every shell
(CLAP/VST3 plugin, signal native block, and eventually the signal FX chain
itself).

## Model

r[fx.stack.model]
A stack is an ordered list of **lanes**; each lane is an ordered list of
**stages**; a stage is `{ style, enabled, params }` where `params` is the
complete parameter set that a single-style instance of the processor would
have. Lanes carry `{ gain_db, mute, solo }`. The stack carries
`{ sum_mode, output_trim }`. A single-style plugin is a stack of one lane
with one stage — there is no separate "non-stack" mode, so every
behaviour below holds for the one-stage case too.

r[fx.stack.topology]
Within a lane, stages process in order (serial); across lanes, the same
input feeds every lane and the lane outputs are summed (parallel). The
topology is exactly this two-level shape — no arbitrary routing — because
it covers the working cases (serial colour chains, parallel balancing,
parallel chains of serial stages) and stays explainable in one strip.

r[fx.stack.limits]
The stage count is bounded per kind so host parameter tables stay static:
`MAX_STAGES` ≥ 4 for EQ, ≥ 8 for compressor / saturator / delay / reverb.
Bounds are a constant of the kind, documented, and raised only by appending
(`fx.stack.params`).

## Parameters and state

r[fx.stack.params]
Every stage's parameters are **host parameters** (automatable, saved by the
host) with stable ids: stage 0 keeps the processor's existing ids
unchanged (a saved single-stage session loads into stage 0 of a stack
bit-for-bit), and stage *n* ≥ 1 uses `stage_prefix(n) + local id` (string
ids: `s{n}.{local}`; numeric ids: `STAGE_BASE(n) + local`). Stack-level
params (`lane` and `order` of each stage, lane gains/mute/solo, `sum_mode`,
`output_trim`) are appended after all stage blocks. Ids are append-only —
raising `MAX_STAGES` or adding a stage-local param never renumbers
anything that exists.

r[fx.stack.style-id]
A stage's style is persisted by its **stable string id** alongside its
index (exactly as `profile_id` / `model_id` work today): the id wins on
load; the index is a fallback for pre-id sessions. Adding a style to a
kind's list never re-labels a saved stage.

r[fx.stack.params-share]
Stages do not share parameters implicitly. A user gesture MAY copy settings
from one stage to another (e.g. "duplicate stage"), but there is no linked
parameter state across stages — each stage is a complete, independent
instance of its style. (Hardware faces with fewer controls simply leave the
rest of their stage's params at their profile-mapped values.)

## Gestures

r[fx.stack.add]
**Shift-click** on a style in the rail (or in a style menu) adds a stage
of that style **after the focused stage in the same lane** (serial).
**Ctrl+Shift-click** (Cmd+Shift on macOS) adds it as a **new lane**
(parallel). A plain click on a style still *replaces* the focused stage's
style, as it does today — so a single-stage plugin behaves exactly as
before until the user shift-clicks.

r[fx.stack.strip]
Whenever a stack has more than one stage, the shell shows a **stack strip**
along the rail: one chip per stage in topology order, lanes separated
visibly, each chip showing the style badge, an enable toggle, and whether
it is focused. Lanes show a small gain and mute/solo. Clicking a chip
focuses that stage — the face/controls now show *that* stage's parameters.
Dragging a chip reorders within its lane or moves it to another lane
(dropping on the gap between lanes creates a lane). A chip's context menu
offers: Remove, Duplicate, Move to new lane, Solo lane, Replace style….
Removing the last stage of a lane removes the lane; removing the last
stage of the stack leaves one default stage.

r[fx.stack.focus]
Exactly one stage is focused at a time; the focused stage is what the
rail's plain click replaces, what the face edits, what the graph's band
gestures add to, and what the meters read (unless the Total view is on).
Focus is UI state (not a parameter), persisted in the editor state so it
survives a close/reopen of the editor.

## Visualisation — the Total view

r[fx.stack.visualize]
A stack offers a **Total** view alongside the per-stage faces: the
composite effect of the whole stack. Per kind: EQ — the summed magnitude
response of all lanes (each lane: product of its stages; lanes summed as
complex responses per `sum_mode`), drawn over the per-stage curves in muted
colours; compressor — the composite static transfer curve and the total
gain-reduction meter, with per-stage GR as thin bars; saturator — the
composite transfer curve and harmonic spectrum; delay — the combined tap
pattern / impulse response on a beat grid; reverb — the combined decay
envelope. The Total view is read-mostly: it exposes the lane gains, mutes,
solos and `sum_mode`, and clicking a stage's curve/bar focuses that stage.

r[fx.stack.visualize-live]
The Total view is computed from the stages' *current* parameters (and the
live meters), updates as any stage's control moves, and never requires a
render/bounce.

## Processing

r[fx.stack.process]
The stack processor is a DSP type shared by every kind (generic over the
kind's stage processor), `no_std + alloc`, allocation-free after
`prepare`: per-lane scratch buffers sized at prepare, serial stages
processed in place, lanes summed into the output with the `sum_mode` law
(`fx.gain-comp.stack`). Disabled stages are bypassed click-free; muted
lanes contribute nothing; a soloed lane silences the others.

r[fx.stack.latency]
Stack latency = max over lanes of (sum over the lane's stages' latencies).
Lanes shorter than the max are delay-compensated internally so parallel
lanes stay phase-aligned; the stack reports the max to the host. Changing
topology or a stage's latency-affecting setting updates the report.

r[fx.stack.sum]
Parallel lanes are summed with `sum_mode` (coherent 1/N default, power
1/√N, raw) over the *active* (enabled, unmuted, or soloed) lanes, then the
`output_trim`. Dry signal is never double-counted: a stage's own `mix`
blends inside the stage; the lane sum is of lane outputs only.

r[fx.stack.gain-comp]
Each stage is gain-compensated individually (`fx.gain-comp.*`); adding,
reordering, enabling, or removing a stage therefore does not change the
overall level, and a parallel stack at equal lane gains is as loud as one
stage.

## Scope across shells

r[fx.stack.every-shell]
The stack is implemented once per kind in the kind's facade/DSP crates
(`*-dsp` / `*` facade + `*-ui`), and exposed identically by the CLAP/VST3
shell in `apps/plugins/*` and the native block in `signal-fx` — the
`DualRouting` special cases in `signal-fx` migrate onto the stack (Series12
= one lane [A,B], Series21 = [B,A], Parallel = two lanes, Split = two lanes
with per-lane L/R pan) and are removed.

r[fx.stack.signal-chain]
The same stack model (lanes of serial stages, per-lane gain/mute/solo,
sum law, Total view, shift-click-to-add) is the target shape for the
signal FX chain itself — a rig's chain of native blocks and hosted plugins
— so that a user learns one topology and one set of gestures. The
`fx-stack` core crate is written so that a *stage* can be any
`PluginInstance`, not only an in-kind style; the signal chain adoption is a
later phase that reuses it unchanged.
