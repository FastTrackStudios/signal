# Reverb work handoff — BigSky MX parity (worktree `fx-dsp`)

**COMPLETE as of 2026-07-08 — all passes landed and merged to main.**
Authoritative behavior spec: `features/fx/reverb/spec/bigsky-mx-reference.md`.

| Pass | Commit | Contents |
|---|---|---|
| A | `5b558cbf` | DualReverb routing, per-slot pan, wet tremolo, copy_params |
| B | `ad19ba1a` | Impulse live params (decay/tail/attack/stretch/direction/feedback), ImpulseReshaper worker + deadlock fix |
| C | `76e69343` | Shimmer dual shift + feedback modes, Magneto ping-pong, NonLinear chop/gate/late stage |
| D | `d3e52929` | Cloud Ensemble (PLL string layer), Bloom Harmonics (POG), Chorale choir/voice/mod |
| E | `2a1be108` | Voice pairs (MX/Classic), Hall mid EQ + swell, named-size unification |

signal-fx `REVERB_PARAMS` ids 0–38 cover the full surface.
Cab Filter deliberately skipped (rig-level NAM/cab exists).

Verification: `cargo test -p reverb-dsp` (92 tests) +
`cargo test -p reverb-dsp --release --test ir_metrics` +
`cargo check --workspace`, all green at merge time.
