# Signal on daw — mixing, routing & the Block-as-plugin model

Status: **proposal / design** (2026-05-31). Cross-repo (`signal` + `daw`). Supersedes the
ad-hoc per-piece mixer idea; folds in the in-progress multichannel-engine / mic-role work.

## 1. Vision

`daw` becomes a full DAW engine — **live** (real-time audio I/O) **and offline** (render) —
that `signal` is a thin client of. The same `#[architect::rpc]` service API targets the
in-process native engine (`daw-standalone` → `daw-native`) today and **REAPER** (via
`daw-reaper`) later, unchanged.

The unlocking reframe: **Blocks are plugins.**

- A **Block** is a plugin instance — either an **effect** (processes audio) or a
  **generator / instrument** (produces audio from MIDI). Signal's sampler is a
  **`Sampler` Block**: an instrument plugin, not an "external live input".
- A **Module** is an **FX container** (a sub-chain / bus node with its own routing).
- A **Layer** is a **track**.

So there is no "feed external audio into a track" gap (the original blocker): the sampler
*lives on a track* as its instrument Block, and the DAW's audio graph processes it like any
plugin. Each Block is internal Rust DSP now; **each is later exposed as a real CLAP/VST3
plugin** (the Sampler, the EQ, the comp…), usable standalone or in any host.

## 2. Concept mapping

| signal concept | daw concept | API surface |
|---|---|---|
| Layer (per-mic / per-channel) | Track | `Tracks::add/set_volume/set_pan/set_muted/set_soloed/set_num_channels` |
| Block (effect) | Fx (plugin) on a track | `Effects::add/set_parameter/set_enabled` |
| Block (`Sampler`, generator) | Instrument Fx at the head of a track's chain | same `Effects` API + MIDI to the track |
| Module | Fx container (sub-chain / bus track) | `Effects` tree (`FxTree`, currently `todo!()`) + bus tracks |
| Layer → bus | Send | `Routing::add_send` |
| Overhead / Room bus | Bus track | a track + sends into it |
| Master | Master | parent-send → master |
| Fader / pan / mute / solo | Track volume / pan / muted / soloed | `Tracks::set_*` |
| Block parameter | Fx parameter | `Effects::set_parameter` |

Reference points in daw: `crates/daw-proto/src/{track,fx,routing}/service.rs` (traits),
`crates/daw-standalone/src/audio_engine/{render,mixer,vst3_host}.rs` (engine),
`crates/daw-standalone/src/sync/daw.rs` (`Standalone`, `ProjectState`).

## 3. The Block plugin model

Define one internal interface the daw graph processes, with both shapes:

```text
Block::process(midi_in, audio_in[], audio_out[], params, ctx)
  - effect:    audio_in → audio_out
  - generator: midi_in  → audio_out   (audio_in empty)   ← Sampler is here
  - multi-out: audio_out has N channels/pairs            ← Sampler per-mic
```

- daw-standalone already hosts **VST3 instruments** (MIDI events → `process()`, see
  `vst3_host.rs`) and CLAP/VST3 effects. The **Block** is a third, *internal* plugin kind
  hosted the same way — a Rust trait the renderer calls per block. signal-sampler's engine
  implements `SamplerBlock`.
- "Synthetic FX" today fall back to 8 generic params; Blocks replace that with real DSP.
- **Plugin exposure (later):** the same Block compiles to a CLAP/VST3 via `nih_plug` (signal
  already ships `fts-signal-controller`); the internal host and the external plugin share the
  Block's DSP + parameter model.

## 4. The Sampler Block & multichannel output

The Sampler Block is **multi-output**: one instance per *piece* (kick) renders all of that
piece's mics as separate output channels (the in-progress per-mic work):

```text
Sampler Block "Kick"  ──>  out: [In 1][In 2][Sub][Overhead][Room Close][Room Far][Room Mono]
```

Each output is tagged with a **`ChannelRole`** (the standardized drum taxonomy, confirmed):

- **`Close`** — direct/spot mics, **per-piece**, 1..N. Sum to *that piece's* track.
- **`Overhead`** — **shared, stereo**. Every piece's OH sums into one OH bus.
- **`Room <name>`** — **shared, named**, any number (Room Close/Far/Mono). Each name combines
  the matching mic across all pieces.

Bus names are the mic ids verbatim. Role is an **explicit field on the mic** (`MicSpec.role`),
set at import (classified from the mic id; future libraries can override).

### Topology (a kit as a daw session)

```text
Sampler Block (Kick, multi-out) ─ Close outs ─> [Kick] track ──┐
                                 ─ OH out ──────> [Overhead] bus ┤
                                 ─ Room outs ───> [Room *] buses ┤
Sampler Block (Snare, multi-out)─ Close outs ─> [Snare] track ──┤──> [Master]
                                 ─ OH/Room ─────> shared buses ──┤
…hats/toms/cymbals…                                              ┘
```

Per-piece **Close** tracks (Kick, Snare, Tom1…) + shared **Overhead** + **Room** bus tracks
+ **Master**. Faders = `Tracks::set_volume`. This is the Superior-Drummer / GGD-mixer layout,
and it's REAPER-swappable.

A signal **Engine = this routed set of tracks** for a piece (its multichannel track + bus
routing), not a single track. The Sampler Block sits at the head of the piece's track; its
per-mic outputs ride that track's **channels** (tracks are multichannel, up to 128 ch like
REAPER), and **channel-mapped sends** (`TrackRoute.source_channels`/`dest_channels`) carry
each mic to its Close/OH/Room destination.

## 5. Audio engine requirements (the daw build)

`daw-standalone`'s `ProjectRenderer` today mixes **items/takes (decoded files)** with
automation/fades. To be a full DAW it must also do **live instrument/effect processing** in
the graph. Concretely, in `daw` (we own this repo):

1. **Block FX kind** — an internal effect/instrument kind alongside CLAP/VST3; `render_block`
   calls `Block::process` for each FX on a track (effects) and for the head generator.
2. **Instrument/generator support in the graph** — a track whose head Block is a generator
   produces audio from the track's MIDI (extend the existing VST3-instrument path to internal
   Blocks; route note events the way signal currently routes them).
3. **Multichannel tracks (up to 128 ch)** — the renderer currently mixes stereo only; grow it
   to N-channel track buffers + **channel-mapped sends** (`TrackRoute.{source,dest}_channels`
   already exist in the proto). This is what lets one track carry all of a piece's per-mic
   outputs and fan them to buses.
4. **FxContainers / FxTree** — implement `Effects::tree()` + nested FX containers so a Module
   (folder of Blocks) is a real container with its own internal routing.
5. **Audio + MIDI on every track and send** — REAPER parity: a track carries both signal
   types, and **MIDI sends** are first-class (MIDI travels with sends). The DAW delivers MIDI
   to a track's instrument Block; the Block owns its internal note dispatch. daw's routing has
   `midi_channel_mapping` types but no MIDI-send creation yet.
6. **Live transport + real-time safety** — block-accurate processing under the cpal callback
   (allocation-free hot path, matching signal-sampler's existing discipline).
7. **Offline render** — the same graph driven faster-than-real-time (largely present via
   `render_block`; ensure Block + multichannel processing participate).

This *is* daw-native's core. Nothing here is signal-specific — any instrument/effect benefits.

## 6. Signal-side build

- **`signal-daw` backend** — owns a `Standalone` (or the `daw_control::Daw` client), builds a
  session from a `.signalpreset`: a Sampler Block per piece, FX Blocks per Module, tracks per
  Close channel, shared OH/Room bus tracks, sends, master.
- **Sampler Block** — wrap signal-sampler's engine as a `Block` (generator, multi-out per mic,
  role-tagged). It **encapsulates its own MIDI dispatch** — note_routing, articulation, choke,
  one-shot, RR all live inside the Block; the DAW just hands it raw track MIDI. Audio out per
  mic. Params exposed as daw `FxParameter`s (signal's `ParamOverride`/`BlockParams` adapt).
- **Preset → session builder** — translate kit + routing into daw `Tracks`/`Effects`/`Routing`
  calls. The drum taxonomy (§4) drives which outputs hit which bus.
- **Mixer UI** (`apps/native`) — drives `Tracks::set_volume/pan/muted/soloed` + Block params;
  reads track/bus list from the session. Faders become real because the DAW mixes.
- Signal keeps voice allocation, round-robin, choke groups, one-shot, the preset/library model,
  and the UI. daw owns mixing, routing, FX hosting, transport, render.

## 7. Decisions (settled 2026-05-31)

1. **Per-mic → multichannel tracks.** Tracks are **multichannel up to 128 channels** (REAPER
   parity). A piece's per-mic outputs ride channels on a multichannel track; channel-mapped
   sends fan them to the Close/OH/Room bus tracks. (No thin-track-per-mic.) The daw renderer —
   currently stereo-only — must grow N-channel buffers + channel-mapped routing.
2. **Adopt daw `FxParameter`.** signal's `ParamOverride` / `BlockParams` map onto daw's
   `FxParameter` model; a Block exposes its params as `FxParameter`s. signal keeps a thin
   adapter for its existing spec files.
3. **Block is an internal FX kind daw hosts.** daw already hosts VST3/CLAP; **Block is a third,
   internal effect/instrument kind** hosted the same way. The Block trait lives in daw; DSP
   impls live wherever (signal-sampler implements the Sampler Block).
4. **Modules = FxContainers (FxTree) now.** A Module *is* an FX container of Blocks (a folder
   of plugins), so daw's `Effects::tree()` / `FxContainer` / `FxTree` (currently `todo!()`)
   gets implemented **up front**, not deferred — Modules need it from the start.
5. **Engine = a routed set of tracks.** A signal "Engine" (a kit piece) isn't one track — it's
   its per-mic track(s) + bus routing, grouped. The Sampler Block (generator) is the head of
   the piece's track; its multichannel output routes out per role.

6. **MIDI dispatch lives inside the Block.** note_routing, articulation selection, and choke
   groups are **elements of the Sampler Block**, isolated within it. The DAW delivers raw MIDI
   to the track; the Block does its own note → articulation → choke → voice logic. No
   reconciliation needed — signal's existing dispatch moves wholesale into the Sampler Block.
7. **Tracks (and sends) carry audio *and* MIDI**, REAPER-style. Every track has both signal
   types; **MIDI sends** are first-class (MIDI travels with sends, not just audio). daw's
   routing has `midi_channel_mapping` types but no MIDI-send creation yet — that's a build item.

## 8. Phased plan

- **P0 — this doc + agreement.** Model + §7 decisions locked.
- **P1 — Block FX kind + host in daw.** Internal effect/instrument kind; `render_block`
  processes Block effects; a trivial generator Block proves live audio end-to-end (track with a
  test-tone Block → master → cpal). Stand up **FxContainers/FxTree** here too (Modules need it).
- **P2 — Multichannel tracks.** Grow the renderer from stereo to N-channel buffers + channel-
  mapped sends (up to 128 ch). Prerequisite for per-mic on one track.
- **P3 — Sampler Block.** Wrap signal-sampler as a multi-out generator Block (per-mic, role-
  tagged, `FxParameter`s); one piece plays through a multichannel daw track.
- **P4 — Kit session builder + buses.** `signal-daw` builds the full kit from a `.signalpreset`:
  per-piece Close tracks + shared OH/Room buses + master + channel-mapped sends.
- **P5 — Mixer UI.** Faders/pan/mute/solo on `apps/native` driving the daw API; Block params.
- **P6 — Offline render + plugin exposure.** Faster-than-real-time render; begin exposing
  Blocks (Sampler first) as CLAP/VST3.
- **P7 — REAPER target.** Same session built against `daw-reaper`; verify parity.

## 9. Relationship to in-flight work

The multichannel-engine + `MicSpec.role` taxonomy work (started in signal-sampler) is **not
wasted** — it becomes the Sampler Block's output spec (per-mic, role-tagged). The per-piece
fader idea is replaced by real daw tracks/buses. The `apps/native` app stays the surface; its
mixer is rebuilt on the daw API in P4.

## See also

- [Sampler File Formats & Mapping](/sampler-file-formats/) — Block/Module/Layer/Pack today.
- [GGD Modern & Massive 2 Map](/ggd-modern-massive-2-mapping/) — the kit this targets.
- daw: `crates/daw-standalone` (engine), `crates/daw-proto/src/{track,fx,routing}` (API).
