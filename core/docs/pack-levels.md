# Pack levels — normalizing a sampled library

Sampled libraries are mastered wherever their author liked. Measured with the
same gesture, the spread across one keys rig is more than 20 dB:

| pack | integrated | trim to −18 |
|---|---|---|
| OB-8 PWM Big Strings | −17.07 LUFS | −0.9 dB |
| Rhodes - LA Custom | −27.99 LUFS | **+10.0 dB** |
| LA Custom C7 Grand | −37.73 LUFS | **+19.7 dB** |
| Microcosm Pad 1 | −39.58 LUFS | +21.6 dB |

With every fader at unity the pad buries the piano by nearly 21 dB. No fader
ride fixes that — and it is not "Omnisphere is loud", either: Microcosm Pad 1
is quieter than the C7. It is per-pack mastering, and it makes a mixer
meaningless, because a fader position stops describing a mix decision and
starts describing a correction.

So each pack carries a **trim** toward a common target, applied *under* the
module fader. Unity then means "this sound's normal level" whatever the
library was mastered at, and the faders go back to being mix decisions.

## Measuring a pack

```bash
cargo run -p signal-sampler --release --example pack_lufs -- <pack.signalpack> [target_lufs]
```

It renders a fixed gesture — a mid-register chord (C3·G3·E4·C5) at velocity
96, held two seconds and released, four seconds total — and reports the
integrated loudness (ITU-R BS.1770, via `signal_sampler::loudness`), the peak,
and the trim toward the target.

**The gesture is the method.** "A piano's loudness" depends entirely on what
you play, so the number is only meaningful because every pack plays the same
notes at the same velocity. Comparing a pack measured with a chord against one
measured with a single note tells you nothing.

Output ends with the line to paste into the level book:

```
  {name "LA Custom C7 Grand", lufs -37.73}
```

## The level book

`~/.config/signal/keys/pack-levels.styx` (override with `FTS_KEYS_PACK_LEVELS`):

```styx
target_lufs -18

packs ({name "LA Custom C7 Grand", lufs -37.73} {name "Rhodes - LA Custom", lufs -27.99})
```

- `lufs` — a measurement. The trim is `target − lufs`.
- `trim_db` — a hand-set override, for when a measurement reads right and the
  pack still sounds wrong. Wins over `lufs`.
- Omit the field you are not using. The serializer writes `@` for an absent
  `Option`; the parser rejects it.
- **`name` is the pack's file stem** — `"LA Custom C7 Grand"`, not the library
  name the browser shows (`"Keyscape LA Custom C7 Grand"`). That stem is what a
  module's `patch` holds, and it is what the lookup keys on. A wrong name
  matches nothing and silently applies no trim; the engine logs every trim it
  resolves (`keys: pack level trim pack=… trim_db=…`) so that failure is
  visible.
- Unlisted packs get 0 dB. An unmeasured library plays exactly as its author
  mastered it rather than being guessed at.
- Trims clamp at ±24 dB. The C7 legitimately needs 19.7; beyond 24 a pack wants
  re-packing, not a bigger number, and a bad measurement should not be able to
  blow up a service.

`signal_keys::normalize` ships the measured values for the packs above as
built-in defaults, so a fresh rig with no config file is still level. The file
overrides them per pack.

## Target

−18 LUFS. A keys patch stacks up to four modules per lane and several lanes per
engine, so the target has to leave room for that sum before the master clips.

## Still to do

The measurement is per-pack and manual. The library is ~8,400 packs, so the
real fix is a pass that walks it, measures each, and writes the whole book —
best as an `fts signal pack level` subcommand, cached, since this is offline
analysis and not something to do at load. Until then only the packs listed
above are corrected.
