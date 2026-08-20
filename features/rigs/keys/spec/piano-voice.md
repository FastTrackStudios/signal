+++
title = "Piano voice — Color, Dynamic Range, Resonances, Space"
description = "The NI Essential Pianos control layer, reproduced over the Signal sampler."
weight = 20
+++

# Piano Voice

The four NI Essential Pianos (The Grandeur, The Maverick, The Gentleman, The
Giant) share one Kontakt script family, and four of its controls are what make
them sound like instruments rather than multisample dumps: **Color**, **Dynamic
Range**, **Resonances** and **Space**. This is what those controls actually do,
recovered from the shipped KSP, and what Signal must do to match.

Source of truth: `Sampled/Keys/The Grandeur Library/The Grandeur/script_0.ksp`
(line refs below) plus `persistent_0.tsv` for the shipped defaults.

**Two scripts, not one.** The Grandeur, The Maverick and The Gentleman share
`NI ESSENTIAL PIANOS - MAIN SCRIPT`; The Giant ships `THE GIANT - MAIN SCRIPT`,
its own engine. They implement the same Color law, but only the shared script
carries the per-instrument offsets below — The Giant has no offset term at all.
An earlier draft of this spec treated all four as one engine; they are not. The pack
side is `features/rigs/keys/specs/ni-pianos.styx` +
`signal-sampler --example build_ni_packs`.

The headline is that almost none of this is DSP. Color and Dynamic Range are
**velocity-domain** transforms, Resonances is a **group mix level**, and only
Space is a real audio process. That is why the sampler can host these pianos
without a Kontakt-shaped engine behind it.

## Color

r[keys.piano.color.velocity-offset]
`Color` MUST be applied as a signed offset to the incoming note velocity before
zone selection, not as a filter sweep. The instrument's own help text calls it
"readjusting the sample mapping" (`script_0.ksp:169`), and the KSP is literal
about it (`:865`):

```
vel := vel + $Mas_sliToneColor + $COLOR_OFFSET
```

r[keys.piano.color.clamp]
The offset velocity MUST be clamped to `1..=127` after the offset and before
any downstream lookup (`:867-873`). An offset that would push velocity out of
range saturates; it never wraps and never selects a nonexistent layer.

r[keys.piano.color.gain-compensation]
Offsetting velocity changes loudness as a side effect, and the instrument
compensates. Signal MUST apply the same trim (`:857-863`, factor at `:1340`):

```
factor := ((Color + COLOR_OFFSET) * 100 / -50 * 12) / 10
Color == 0   ->  boost := 0
Color <  0   ->  boost := (vel + 20)   * factor
Color >  0   ->  boost := (-vel + 150) * factor
```

Note the asymmetry — the two arms are not mirror images, so a single signed
formula will not do.

r[keys.piano.color.three-effects]
The offset velocity MUST feed all three of its consumers, which is why Color
sounds like more than a level change:

1. **velocity-layer selection** — a different recorded sample;
2. **`%VolumeTabelle[vel]`** — the per-velocity trim;
3. **low-pass cutoff** — `set_engine_par($ENGINE_PAR_CUTOFF, %FilterTabelle[vel], …)`
   (`:879`), so brighter velocities open the filter.

Implementing (1) alone is the obvious mistake: it gets the sample right and
still sounds wrong, because the tone curve did not move with it.

r[keys.piano.color.per-instrument-offset]
`$COLOR_OFFSET` is **not the same for every piano**, so the Color law MUST be
parameterised per instrument rather than shared. The shared script picks its
constants by which group is present (`find_group("<n>_DRY_A#-1")`):

| instrument | `$DYN_OFFSET` | `$KK_DYN_OFFSET` | `$LOW_KEYS_OFFSET` | `$COLOR_OFFSET` |
|---|---|---|---|---|
| The Maverick (0) | −50 | −25 | 10 | **−5** |
| The Grandeur (1) | −55 | −25 | 0 | 0 |
| The Gentleman (3) | −50 | −10 | 22 | 0 |
| The Giant (own script) | — | — | — | *no offset term* |

The Maverick being −5 means its Color knob is biased: at a nominal 0 it is
already playing five velocity steps softer than the others. Getting this wrong
would sound like "the Maverick is a bit dull" and would be very hard to trace.

These constants were invisible until the extractor was fixed — `extract_script`
took the longest run of printable ASCII, so the umlaut in a German comment
truncated 73 KB off the head of every `script_0`, taking the whole `on init`
block with it. See the `nkx-extract` commit "extract_script truncated every
script with a non-ASCII byte".

r[keys.piano.color.range]
`Color` MUST expose the range `-50..=+50` with a default of `0`
(`persistent_0.tsv: $Mas_sliToneColor 0`), and report as an integer, matching
the plugin's own readout (`:563-569`).

## Dynamic Range

r[keys.piano.dynamics.velocity-law]
`Dynamic Range` MUST compress (negative) or expand (positive) using a
velocity-derived gain, keeping every velocity sample in play — it never
narrows the sampled set (`:843-854`):

```
helper := DynRange + DYN_OFFSET          (+ KK_DYN_OFFSET when $Ana_mnuVelo == 4)
DynRange <= 0  ->  gain := (vel - 127) * helper
DynRange >  0  ->  gain := (127 - vel) * helper * -1
```

r[keys.piano.dynamics.default]
Default `0`; range `-200..=+200` (`persistent_0.tsv: $Mas_sliAnaDyn 0`,
control table `:10`). The `$Ana_mnuVelo == 4` branch is the shipped default
(`$Ana_mnuVelo 4`), so `KK_DYN_OFFSET` is in play out of the box.

## Resonances

r[keys.piano.resonance.bus-level]
`Resonances` MUST be a mix level on the resonance sample group, not a DSP
reverb or a synthesized sympathetic model (`:1428`):

```
set_engine_par($ENGINE_PAR_VOLUME, $Mas_sliAnaReso, -1, -1, $NI_BUS_OFFSET)
```

r[keys.piano.resonance.pedal-gate]
Resonance voices MUST only sound while the sustain pedal is down and the
control is above zero (`:964`): `if ($Mas_sliAnaReso > 0 and $PedalDown = 1)`.

r[keys.piano.resonance.on-demand-pack]
The resonance group ships as its **own** `.signalpack` (`<Library> -
Resonance.signalpack`) and MUST be loadable independently of the piano pack.
It is ~30% of the zone count and silent until a foot goes down, so a rig
running to a tight memory budget can leave it unloaded and still play.

## Space

r[keys.piano.space.convolver]
`Space` MUST be a convolution reverb over a menu of shipped impulse responses,
not an algorithmic reverb (`:684`, `:727`):
`load_ir_sample(!SpacePaths[$Spa_mnuType] & ".ncw", $IRSlot, 0)`.

r[keys.piano.space.controls]
Space MUST expose on/off, send amount, pre-delay, size and IR selection
(`:177-181`; automation names at `:508-510`).

r[keys.piano.space.ir-table]
The IR menu is 29 entries (`declare const $NUM_IRS := 26` spaces over 29 IRs),
declared as parallel `!SpaceNames` / `!SpacePaths` arrays — display name
("Concert Hall 1", "Cham. M. Hall 1") against sample stem
(`Space_GI_CONCERT HALL`, `Space_GI_CHAMBER MUSIC HALL`). Signal MUST present
the same names in the same order so a preset written against the plugin means
the same thing here.

**Partly unblocked**: the name/path tables came back with the extractor fix, so
the menu is known. Still outstanding is pulling the IR audio itself — the
`.ncw` files are addressed by `load_ir_sample` path rather than by a zone, so
the zone-driven extractor does not reach them. See K1/K5 in
`crates/signal/docs/keys-rig-patch-buildout.md`.

## Noise groups

r[keys.piano.noises.toggles]
Release, Hammer, Damper, Pedal, String-noise and Overtone/SSR groups MUST each
be independently switchable (`:204-218`), because each is a separate sample
group in the pack and each costs memory. The shipped defaults have Release on
and the rest off (`persistent_0.tsv`) — matching that matters, since a piano
with every noise layer enabled is both louder and much heavier than the one
the library ships.

## Defaults

r[keys.piano.defaults.from-persistent]
A pack's shipped state MUST come from the library's own `persistent_0.tsv`
rather than from invented values — that file is the authoritative "what this
patch sounds like out of the box" (52 variables, including the Tone EQ and
compressor state).

## Verification

r[keys.piano.verify.ab-against-kontakt]
Correctness MUST be established by A/B render against real Kontakt, not by
listening — same discipline as `css-ab-harness` / `css-reference-matching`.
The grid that matters: Color at `-50 / 0 / +50` × Dynamic Range at
`-200 / 0 / +200`, comparing level ratio and spectral shape. Color especially,
because a partial implementation (r[keys.piano.color.three-effects]) is
plausible-sounding and wrong.
