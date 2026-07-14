# Signalpack Zone-Format Requirements (Keyscape 44-patch survey)

What the `.signalpack` format must carry so the runtime can play every Keyscape
patch from **embedded authoritative zones** — no filename parsing, one uniform
loader for all libraries. Derived by surveying all 44 STEAM `.db` manifests
(`sc-import zonestats`; `docs/scripts/survey.py`). Each `.db` carries, per
sample: key range (`Pitch N-M` dir), root (`BaseNote`), velocity range
(`HitVelocity Min/Max`), round-robin (`RoundRobinSequenceNum`), gain (`Level`),
tune (`A440`), and mic (the `<LayerHitStack>` XML basename).

## 1. Per-zone fields (every sample)

| Field | `.db` source | Observed range | Patches | `ZoneSpec` today |
|---|---|---|---|---|
| `file` | `AudioFilePath` | — | all | ✅ `file` |
| `key_min`/`key_max` | `Pitch N-M` dir | 0–120; sparse or 1-per-key | all | ✅ |
| `root_key` | `BaseNote` | 0–127 | all | ✅ `root_key` |
| `vel_min`/`vel_max` | `HitVelocity Min/Max` | 1–152 distinct layers/patch | all | ✅ |
| `rr_index` | `RoundRobinSequenceNum` | 0–15 (**up to 16 RR**) | most | ✅ `rr_index` |
| `gain_db` | `Level` (hex f32 → dB) | per-sample | **38/44** | ✅ `gain_db` |
| `tune_cents` | `A440` (hex f32 → cents) | per-sample (e.g. 18.9¢) | **26/44** | ✅ `tune_cents` |
| `mic` | layer-XML basename | 14 distinct ids | multi-mic common | ✅ `mic` |
| `articulation` | soundsource role | body + 7 variants | multi-artic patches | ✅ `articulation` |
| `trigger_mode` | soundsource / layer name | attack/release/pedal-down/up | 33 patches | ✅ `trigger_mode` (+`trigger_cc`) |

**The runtime `ZoneSpec` already models every per-zone field.** The `.db`→zone
mapping is 1:1; nothing new is needed at the per-sample level.

## 2. Dimension value-sets (what the format's enums must admit)

- **Mics (14):** `Main`, `Direct`, `Room`, `Stereo`, `Tube Mic`, `Mono Overhead`,
  `AMP`/`Amp`, `Pickup 1`, `Pickup 2`, `PZM Mic`, `NT5`, `149`, `300`.
  Up to **7 mics/patch** (Wing Upright), **10** (Duo Maps Studio). Multi-mic is
  the common case, not the exception.
- **Articulations / roles:** `body` (43), `release` (30), `mechanical` (4),
  `pedal` (6), plus **body variants** — `mute`, `tremolo`, `vibra`, `wah`,
  `suitcase`, `amp`, `fast`, `slow`. Rhodes Classic alone has 4 body FX variants
  (`amp`/`suitcase`/`wah`/direct); Simone Celeste has body/mute/vibra.
- **Triggers:** `attack` (all), `release` (29), `pedal-down` (2), `pedal-up` (2).
- **Round-robin:** up to **16** slots; needs `cycle` + `random` modes.
- **Velocity layers:** 1 → **152** (Duo Maps Stage), 92 (LA Custom C7 Grand).
  Arbitrary per-zone `[min,max]` bands — not a fixed dynamic ladder.
- **Key-range styles:** 18 full-keyboard, 25 sparse-multisampled
  (range + root + pitch-shift), 1 few-note.

## 3. Special cases needing explicit format/runtime support

1. **Multi-mic groups** — many zones share `(key, vel, rr, articulation)` and
   differ only by `mic`. The format must let the preset declare **which mics are
   active** and the engine fire one zone per active mic to its own bus (or a
   summed default). `MicSpec` + `ZoneSpec.mic` exist; **default/active-mic
   selection is the open runtime piece**.
2. **Multi-articulation select** — body FX variants (amp/suitcase/wah/mute/
   tremolo/vibra) are alternate bodies, not layers. Format needs a declared
   articulation list with a **default** + keyswitch/CC select. (Today's default
   picker is the interim `@Release`/`kind` heuristic — zones make it explicit.)
3. **Release layers** — `trigger_mode=release`, fired on note-off, velocity-
   scaled, blended under the tail. Some patches ship **fast/med/slow release**
   variants (Vintage Vibe `VVRFstr`/`Medr`/`Slor`) selected by note-hold time —
   needs a release-class/hold-time dimension or per-variant `vel`/trigger split.
4. **Pedal-triggered layers** — `Pedal Down.xml`/`Pedal Up.xml` → CC64 threshold
   triggers (`trigger_cc=64`, down/up). Covered by `trigger_mode`+`trigger_cc`.
5. **Mechanical-noise layers** — separate note-on ambience (Rhodes/Wurlitzer/
   Pianet T), velocity-scaled, mixed under the body (Keyscape default −20 dB).
   Format-wise a normal zone; the **blend/level policy** is runtime.
6. **Combo / reference patches** — Duo Maps Stage/Studio have **0 own audio**;
   their zones reference *other patches'* samples. This is the **one true format
   gap**: `ZoneSpec.file` is pack-local. Needs a cross-pack sample reference
   (e.g. `pack:<name>#<file>` or a shared sample store the combo pack points at).
7. **Sparse multisampling** — 25 patches sample every few keys; zones carry a
   `key_min..key_max` span and the engine pitch-shifts from `root_key`. Already
   how zoned mode works.

## 4. Format work items (summary)

The per-zone model is **done** (`ZoneSpec` covers it). What the signalpack
format/runtime still needs:

- **Embed the zone table** in the pack (versioned) + a matching styx declaring
  `mics`, `articulations` (with `kind`/default), `dynamics`, `sections`.
- **Active-mic config** — declare available mics + default active set; engine
  fires per active mic.
- **Articulation default + select** — from the zone data, not a name heuristic.
- **Release-variant selection** — fast/med/slow by hold time (or fold into `vel`).
- **Cross-pack sample references** — for the Duo Maps combo patches only.
- **Runtime blend policy** — release/mechanical/pedal layers mixed under the
  body at Keyscape's default levels (release −10, mech −20, pedal −20 dB).

Everything else (key ranges, roots, velocity bands, RR up to 16, per-sample
gain/tune, the 14 mics, the 8 articulation variants, attack/release/pedal
triggers) maps directly onto the existing `ZoneSpec` — the format just needs to
**carry** it and the loader to **read** it instead of parsing filenames.
