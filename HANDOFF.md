# Handoff — the native guitar rig, levelled (2026-09-23)

Everything is **committed locally, nothing pushed** (the user's standing
rule: commit locally, never push/PR/tag unless asked). Branch
`macos-signal-rig` in `signal`, `processor` and `daw`, on
`/Volumes/dev-drive`. The previous handoff (the rig in a browser) is in git
history at `b2df36c1`.

The user's direction for this stretch: **get the native desktop app fully
playable first; port it to wasm after.** The browser rig (a page where any
computer becomes the rig, all audio in the tab) is planned — see
*Browser rig* below — but not started.

## Build and run — read this first

- **One build line:** `just app` =
  `cargo build --profile release-fast -p signal-desktop --features signal-keys-rig`.
  `just guitar` and `just desktop-run` use the same line. Building
  `signal-desktop` with a *different* feature set or profile makes cargo
  recompile every shared crate (that was the 15-minute rebuilds). The CLI
  builds alongside with no penalty:
  `cargo build --profile release-fast -p signal-desktop -p signal-cli --features signal-desktop/signal-keys-rig`.
- **release-fast** (root `Cargo.toml`): release opt, incremental, 256 CGUs.
  An edit in `signal-sampler` rebuilds the app in ~15 s. Local only.
  **dev** now has the render stack (Blitz/Stylo/Taffy/Parley/Vello/wgpu) at
  opt-level 3, so a dev build is usable too.
- `bin/signal-latest-bin` prints the newest of `target/{release-fast,release,debug}/signal-desktop`;
  `Signal Rig.app`'s launcher and the menu bar's login agent hard-link it,
  so **whatever you built last is what launches**.
- Launch: `open "/Volumes/dev-drive/bin/Signal Rig.app"` (config
  `XDG_CONFIG_HOME=/Volumes/dev-drive/config`). Logs:
  `/Volumes/dev-drive/logs/signal-app-YYYYMMDD.log` — note it grows ~1 GB/day
  from a repeating usvg "Rect has invalid height" warning (see *Open*).
- **Never build while the user is playing** — a build takes every core and
  causes dropouts. Never run a benchmark while a build runs.

## What changed (commits)

`processor`: `38e5259` one GPU device for every canvas · `5f52aea` comp shader
fix for the browser · `439379a` per-thread DI sidechain + a meter channel per
compressor.
`signal`: `b9da6a38` menu bar · `629be64d` build profiles · `690a07f5` the rig
work below.

### Loudness — the model now

**Levels live on the blocks, are measured by the rig's own engine, and are
built into the chain.** Bottom up:

1. **Amp module snapshots** (`modules.styx`) carry `level_db` / `level2_db`:
   the amp through its own cab, levelled to **−23 LUFS** (`TARGET_LUFS`,
   `signal-sampler/src/patch_level.rs`).
2. **Drive options** (`drive-presets.styx`) carry `level_db`: engaging the
   pedal at drive 0.5 is unity.
3. **Preset snapshots** (`presets.styx`) `level_db` balances only the effects
   around the amp (median +2.7 dB).
4. **Patches**: `level_db` is an *offset on its snapshot's* (compose.rs
   `flatten` adds, it used to overwrite — which silently discarded every
   patch-level correction). Plus the player's own `trim_db`.

`nodes::apply_block_levels` writes (1)/(2) into each NAM block's
`input_trim_db`/`output_trim_db` **when the chain is built**. Nothing
applies gain at switch time any more (`apply_all_drives` is a no-op kept as
the switch paths' single hook). A drive knob moved live
(`session::apply_drive`) corrects *relative* to the block's built level via
the cached drive curve.

Commands, in order: `signal rig level-modules` → `signal rig level-presets`
→ `signal rig level <Profile>` (all `--dry-run` capable). A full cold pass is
~7 min; unchanged chains are cache hits (seconds). The app's **Level
patches** writes per-patch offsets the same way.

**One code path, measured.** `GuitarRig::open_offline(sr)` builds the
identical project/track/slots/probe/chains as `open` and renders through the
same `ProjectRenderer`, pulled by `render_offline` (no device). Input is the
probe's test signal (`start_test_signal`), output the new `OutputTap` after
the chain (`measure_output`). `features/rigs/guitar/src/measure.rs`:
`patch_lufs` (cached via `patch_level::level_cached`, engine key `rig-v3`),
`apply_chain_bypass` (the switch-in step both live and offline call). The old
hand-rolled `render_lufs` is no longer used by the guitar rig. Every
divergence found on the way was a second code path: live applied drive
compensation only to amps (a definition lookup missed composed patches'
pedals); offline skipped chain bypass; snapshot levelling used a different
chain builder. If live and offline ever disagree again, look for the
second path.

Verified: every patch in all four profiles measures −23.0 LUFS on the
engine; switching through a whole profile on one offline rig is repeatable
(±1 dB of settled). The user confirms "the patches sound level now".

### Other fixes this stretch

- **Engine deadlock** (the "wedge"): `nodes()`/`perf()` took `profile_def`
  then `rig`; `patches()`/`resync_blocks()` the reverse. LOCK ORDER (rig
  before profile_def) is documented on `GuitarRigBackend::rig`. Found with
  `sample` + lldb (`__psynch_mutexwait` x3 = owner tid).
- **Switch stalls**: a footswitch used to run a 5.5 s drive-curve sweep for
  an uncached model inside the request. Startup now pre-measures every NAM
  any patch plays, in parallel.
- **Footswitches**: `midi.styx` `tap_notes (1 2 3 4 5)` — the AIRSTEP (BLE,
  connected via Audio MIDI Setup → MIDI Studio → Bluetooth) sends Note On /
  Note Off per switch; tap/hold from timing (`rig-host/src/gestures.rs`).
  The user set Note On velocity to 1: midicore's CoreMIDI input *re-encodes*
  `90 n 00` as `80`, losing the press (upstream fix pending).
- **Preset tab = audition**: `choose_preset` plays the snapshot alone (the
  `compose::snapshot_patch` template levelling measures), never writes the
  profile. It used to repoint and save the live patch on every click.
- **Presets named for gear** (`signal rig regroup-presets`): the
  profile-named presets (Worship Clean …) were dissolved into the amp
  presets; profiles repointed. Worship Drive → Fender Deluxe Reverb ·
  Cranked (user asked for "more normal").
- **Compressor panels**: a meter channel per block (Pre Comp 1, Post Comp 2,
  Limiter 3 — `profiles::meter_channel`), `RigEvent::CompWave(CompTrace)`
  names its block. Red on the IN/OUT strips = within 3 dB of clipping.
- **Menu bar** (`--menubar`, `apps/desktop/src/menubar.rs`): runs from
  `bin/signal-menubar` (NOT inside the .app — Launch Services then treats
  the app as already running). Login agent:
  `~/Library/LaunchAgents/com.fasttrackstudio.signal-menubar.plist`.
- Level target −23 LUFS (was −18; peaks hit the limiter).
- `SIGNAL_SWITCH_PROBE=1` writes `logs/switch-<patch>-<t>-{in,out}.wav` (4 s
  after each switch) to re-render a suspicious transition offline. The app
  is currently launched with it on.

## User config (not in any repo)

`/Volumes/dev-drive/config/signal/rig/`: `presets.styx`, `modules.styx`,
`drive-presets.styx`, `profiles/*.styx`, `midi.styx` were rewritten this
session. Every edit left a timestamped `.bak-*` beside the file.
`calibration/di-reference.wav` is the user's own guitar (three DIs joined;
the old GuitarLSTM DI is `di-reference.guitarlstm-ts9.wav`).

## Open — in the order I would do them

1. ~~**Latency.**~~ DONE (`673dac53`, daw `51d4b1c8`). Cause: on macOS
   `rig-host` aliased `DuplexEngine` to the cpal engine — input and output
   as two streams bridged by a ring that drained one block per callback and
   never shed backlog, so every stall added latency for the rest of the
   session. The guitar rig now runs on daw's CoreAudio HAL IOProc (log line
   `coreaudio duplex: started … round_trip_ms=6.33` at 64 frames on the
   MiniFuse; the rest is the interface's own converters). The keys rig stays
   on cpal (CoreAudio buffer size is per process per device). The cpal ring
   now drops backlog past one spare block. DSP proven zero-latency by
   `cargo run --profile release-fast -p signal-guitar --example latency_probe
   [-- <Profile>] [--per-block]`: every block answers on the impulse's own
   sample except NAM amps / cab IRs (0–47 samples: the captured gear's own
   response, not buffering).
2. ~~**Dual-amp blends.**~~ DONE (`9fc659b4`). Not a loudness problem:
   `prepare_chain` wrapped the blend stage with
   `if let (Some(role), Some(inner)) = (role, slot.take())`, which takes
   every box before matching, so a blend patch played its two amps and
   nothing else (no gate/comps/trim/FX/limiter). Levelling could never
   converge, which is where +14–25 dB came from. Now `amp_blend::wrap`,
   tested; blends level at +5–9 dB. `examples/blend_probe`.
3. **Output Level knob in the UI** — the user asked for "an output gain
   setting in the block". Levels are stored on modules/drive options and
   built into the trims, but not yet shown or editable per block.
4. midicore-macos: keep the raw status byte (a velocity-0 Note On press).
5. The usvg warning flood (~1 GB/day of log): the gate visualiser still
   draws zero-height rects somewhere.
6. `signal rig level` (non-dry-run) writes `patch.level_db`; with the new
   offset semantics that is right, but `levelling::level_profile` measures
   with offsets zeroed, so its dry-run shows the *raw* figure, not the
   final one — confusing when verifying.

## Compressors and the gain bias (2026-09-23, `eea9e0e3`, processor `f897de7`)

- Compressor block: `makeup` (dB, output level after the blend) and `mix`
  (parallel blend), both zero-latency.
- **Pre Comp** presets hear only the guitar: dialled once to the DI
  reference (`examples/comp_dial`) and valid everywhere — Clean Sustain,
  Funk Squash, Country Squash, Swell Sustain, Drive Tighten. Never re-dial
  unless the guitar/interface gain changes.
- **Post Comp** presets (Live Glue 2 dB, Clean Punch 4, Lead Sustain 5,
  Rhythm Catch 1 — `target_gr_db` in blocks.styx) carry the character; each
  preset snapshot's threshold is its own override, dialled on its own chain
  by `signal rig dial-post-comp` (all 113 within ±0.2 dB; thresholds span
  8 dB between amps). `examples/comp_verify` checks them.
- Every snapshot picks a Pre and Post Comp in presets.styx by gain class
  (Amp modules no longer set Post Comp). New variations: Twin Reverb ·
  Funk, Deluxe Reverb · Country (not in any profile yet).
- `gain_bias_db` per snapshot: loudness target = −23 + bias (clean 0, edge
  +1, crunch +2, high-gain rhythm +2.5, lead +3). All three levelling paths
  use `compose::loudness_target`. Measuring it (peak-to-loudness of the gain
  stages) was tried and does not sort by gain.
- **Order after changing a comp preset or an amp:** `signal rig
  dial-post-comp` → `level-presets` → `level <Profile>` ×4 (dial ~18 min,
  presets ~4 min).
- Sidebar shows each patch's variation under its preset.

## Browser rig (next phase, after native is done)

Plan from this session (research, no code): mirror the keys rig —
`apps/desktop/src/web_keys_rig.rs` + `web_keys_backend.rs` — as
`web_guitar_rig.rs` (route `/rigs/guitar`) + `web_guitar_backend.rs`: an
in-page `Rig` implementation over `rig.json` (`signal rig web-bundle`)
driving `features/rigs/guitar/worklet`, mounting the real
`GuitarRigRemote`. `GuitarRigBackend` itself is too native to compile for
wasm. Phase 1: stacks into the bundle, `setParamByName` in the worklet, the
~22 `Rig` methods the UI calls to play, web-stage wiring, COOP/COEP
headers. Hosting (the public link's domain, which captures may be
redistributed) is the user's decision.
