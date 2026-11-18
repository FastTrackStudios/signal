# Spectrasonics SynthMaster Patch Architecture

What's inside a `.mlt_*` patch (Keyscape, Omnisphere, Trilian) — the engine
features Signal would need to support natively to play factory presets.

`.mlt_*` files are **plain XML**. No encryption. The format is shared across
all three Spectrasonics products (`<SynthMaster vers="…">` root). All numeric
values are stored as either integer strings or **8-character hex of an IEEE
754 32-bit float** (e.g. `3f800000` = `1.0`, `3f400000` = `0.75`).

Locations:
- Defaults / templates: `STEAM/<Product>/Defaults/*.mlt_<ext>`
- Factory init: `…/Defaults/Factory/`
- Real factory presets: `STEAM/<Product>/Settings Library/Patches/Factory/*.db`
  (these wrap the SynthMaster XML in the same plain-XML STEAM container we
  already handle for soundsources).

## Top-level structure

```
<SynthMaster vers="2.2.0b11resize_C">
  <ENTRYDESCR name="…" library="…" ATTRIB_VALUE_DATA="…">
    <!-- patch metadata: author, complexity, genre, type, size -->
  </ENTRYDESCR>

  <SynthMasterEngineParamBlock>
    <MasterEngineBaseParamBlock version="1" gain="…" panic="…" autoLdPatch="…"
                                pPan0..7=… pLevel0..7=… pLatch0..7=…
                                pTrigger0..7=… pSusEn0..7=… pMute0..7=…
                                pSolo0..7=… pGAtten0..7=…
                                p0AuxSnd0..3 … p7AuxSnd0..3
                                out0..7 chan0..7 mg0..7 spn0..7 …>
      <MEffRack Preset="Rack Presets">
        <EFFMODULE Type="…" P0..P14 Active MixLock>×4
      </MEffRack>
    </MasterEngineBaseParamBlock>

    <SynthEngine>×8     <!-- one per Part -->
    <SplitPart>×4       <!-- key/zone splits across the 8 parts -->
    <ARP>×8 (per Part)
  </SynthMasterEngineParamBlock>
</SynthMaster>
```

## Multi (8 parts)

`MasterEngineBaseParamBlock` carries the per-Part mix bus (8 parts):
- **Mixer**: `pPan`, `pLevel`, `pGAtten` (gain-attenuation), `pMute`, `pSolo`
- **MIDI routing**: `chan` (channel), `out` (audio output bus 0–7)
- **Trigger / latch**: `pTrigger`, `pLatch`, `pSusEn` (sustain enable)
- **Mod groups**: `mg0..7` (mod group assignments)
- **Aux sends**: `pNAuxSnd0..3` — 4 aux-bus sends per part (32 sends total)
- **Stereo spread**: `spn0..7` (per-part stereo spread)
- Plus per-pPan/pLevel **MIDI Learn** triplets:
  `pPanNMidiLearnDevice0`, `…IDnum0`, `…Channel0` so any param can be
  bound to an external CC.

Splits use `<SplitPart>` blocks (key range with ramp regions).

## Per-Part: `<SynthEngine>` → 2× `<SynthSubEngine>` (Layer A + Layer B)

Each Part has 2 layers ("A" and "B" in Omnisphere parlance), each a full
synth voice.

### `<SynthSubEngine>` modules

| Block | Role | Attribute count |
|---|---|---|
| `<OSC>` | Oscillator with 4 sub-layers, granular, FM, AM, unison | ~85 attrs |
| `<WAVES>` | Wavetable bank | (refs `.stmwf`) |
| `<FMWAVES>`, `<AMWAVES>` | FM/AM modulator wavetables | |
| `<HARM>` | Harmonic shaper — per-harmonic level/pan/shape/symmetry/tune | `lvlN, panN, shpN, smiN, symmN, synN, tunN, wfmN` |
| `<VOX>`, `<VOICE>` | Vocal/formant modeling | (Omnisphere "Innerspace", etc) |
| `<DIST>` | Distortion | 14 P-slots |
| `<WAVESHAPER>` | Waveshaper | `bc, dpth, gain, mix, pos, srrdc, tone, type` |
| `<FILTER>` | Multi-mode filter | `freq, res, key, keymp, env, envdpth, eqPost, distPost, name, NameStr, bal, sprd, …` (24 attrs) |
| `<FENV>` + `<FENVPARAMS>` | Filter envelope | `attk, hold, decy, sust, rels, dpt, sync, trgMd, velsens, mglvls, mgcrvs, atchs, chsTrg, lp, …` |
| `<AENV>` + `<AENVPARAMS>` | Amp envelope | (same shape as FENV) |
| `<MODENV>` + `<MODENVPARAMS>` | Generic mod envelope | 6 per Part = 48 total |
| `<MOD_ENV2_2>` | Second-gen mod envelope | 5 per Part = 40 total |
| `<LFO>` + `<LFO_SET>` | LFOs | 6 per Part = 48 total. `rate, scale, swing, sync, phase, center, bpolar, envdpth, pulsemix, pulserate, randrev, randrg, resettr, rndtr, unidir` |
| `<EQ12>`, `<EQ2>` | Post EQs | |
| `<MOD_MATRIX>` | Modulation routing | `sourceN, targetN, dampN, defVN, hiN, loN, muteN, nofiltprefix` |
| `<SLICESEQSTEP>` | Step sequencer steps | 33 per Part = 264 total. `END, SLICEINDEX, VEL` |
| `<ThinRule>` | Polyphony / round-robin thinning | `ThinL, ThinV, ThinRR, ThinRelRR, ThinVelRange` |

### `<OSC>` highlights (~85 attributes)

- **4 layers** within a single oscillator: `LayerNAct`, `LayerNMt` (mute), `LayerNVol`
- **Waveform / synth mode**: `kind`, `type`, `wfm`
- **Pitch**: `oct`, `semi`, `tune`, `tuneFine`, `port` (portamento), `portAct`
- **Granular synthesis**: `gran`, `ngrains` (1–6), `granExp`, `granMix`,
  `granPAccum`, `granPDir`, `granPos`, `granSprd`, `granTrg`, `granV1`,
  `grain3..6`
- **FM**: `fm`, `fmwf`, `fmscl`, `fmpd`, `fmpw`, `fmkt`, `fmhs`,
  `moddepth`, `modint`
- **AM**: `am`, `amwf`, `amscl`, `ampd`, `ampw`, `amkt`, `amhs`,
  `AMmoddepth`, `AMmodint`
- **Unison**: `ucnt` (count), `uoct`, `uwdth`, `udprg`, `udpth`, `udrft`,
  `uvps`, `utrfpsl`
- **Hard-sync**: `hrdsnc`, `srxfade`, `sana`, `spha`, `srevpb`
- **Mogrify / spectrosynth**: `mogrify`, `timbre`, `eiFrLoc`, `enssnrl`,
  `csshdamp` (CSS harmonic damping)
- **Pulse / PWM**: `ampw`, `fmpw`, `pwidth`, `pdepth`
- Per-layer `relVol` (release volume)

## Per-Part FX

Each `<SynthEngine>` carries:

- 4× `<EFFRACK>` (`AEffRack0..3` for aux-A, `MEffRack` for main, plus
  `AUXEFFRACK` for system buses)
- Each rack has 4× `<EFFMODULE>` slots
- Each module has 15 generic params `P0..P14` + `Active`, `MixLock`, `Type`

Total per multi: 24 EFFRACKs × 4 modules ≈ 100 effect slots, plus 4 master
modules.

## Arpeggiator / Step Sequencer

`<ARP>` (one per Part):

```
ArpOnOff, ArpClock, ArpSpd, ArpSwing, ArpOct, ArpLen, ArpSpan, ArpPhase,
ArpVelMix, ArpFeelGrooveOnOff, ArpGrooveName, ArpSnapToGridOnOff, ArpSnpGrvty
```

Plus `<ARPSEQ2>` and `<ARPFEELSEQ>` blocks carrying `TIMESIGNUM`,
`TIMESIGDENOM`, `TICKSPERQUARTER` for groove patterns.

## Macros / "Custom" knobs

`<Custom0>…<CustomNN>` blocks (often 16–32) — user-assignable macro layer
that the front-panel knobs and ORB drive into the mod matrix.

## What Signal needs to support to play these patches

Tiered roadmap, easiest first:

### Tier 1 — minimum viable Spectrasonics playback (samples-only patches)

Many Keyscape and Omnisphere patches are essentially **sample players** with
a filter and amp env. Supporting just these gets the Rhodes / Wurli / piano
factory patches playing.

- 8-Part multi mixer (level / pan / mute / solo / out / channel / aux sends)
- Per-Part: 1 oscillator using sample mode (`OSC.kind == sample`) →
  reference into the soundsource's zonemap
- Filter (multi-mode, with key tracking)
- AENV + FENV (ADSR + hold + curves)
- 1 LFO (rate, sync, scale, polarity)
- 1 mod envelope
- Mod matrix (limited: vel→amp, vel→filter, mod-wheel→cutoff, LFO→pitch)
- Aux send → 1 reverb slot

Skip: granular, FM/AM, harmonic shaper, vox/voice, mogrify, full FX rack.

### Tier 2 — Omnisphere wavetable + dual-layer

- 2× SynthSubEngine per Part (Layer A + B with crossfade)
- Wavetable mode in OSC (consume `.stmwf` / SignalPack `WavetableSpec`)
- 4-layer OSC sub-mixer (`Layer0..3 Vol/Act/Mt`)
- More mod envelopes / LFOs (3–5 each per Part)
- 12-band EQ + waveshaper + distortion modules in FX rack
- Step sequencer (SLICESEQSTEP)

### Tier 3 — Full feature parity

- Granular synthesis engine
- FM / AM oscillator modulation
- Harmonic shaper (`<HARM>` per-harmonic level/shape/symmetry)
- Voice/Vox formant modeling (Innerspace etc)
- Mogrify / Spectrosynth resynthesis
- Hard-sync with anti-aliasing crossfade
- Unison engine (count, detune, drift, phase spread)
- Arpeggiator with feel/groove patterns
- Full FX rack types (Pro-Verb, Lo-Fi, Tube Saturator, Phasers, Comb, etc)
- ORB (XY) controller mapping
- ThinRule polyphony manager
- Per-param MIDI Learn

### Tier 4 — Live Mode / Stack (Keyscape-specific)

- Stack profiles (`Settings Library/Presets/Factory/Stack.db`)
- Live multi-instrument splits/layers loaded from a Stack file

## Immediate value of this RE

Even without supporting all of it, having the format documented means we can:

1. **Author Signal-native versions** of factory patches by hand (start simple,
   pick a Rhodes patch, mirror the OSC + filter + AENV settings).
2. **Build a `.mlt_*` → Signal patch importer** once the synth engine matches
   Tier 1 — convert hundreds of patches automatically by mapping
   SynthMaster XML attributes to the Signal patch format.
3. **Validate Signal's zone-mode playback** against the simplest patches
   (those that only use sample mode, no filter, no LFO).

## Settings Library `.db` patches

Factory presets live in `STEAM/<Product>/Settings Library/Patches/Factory/`
as `.db` files using the same plain-XML STEAM container format as
soundsources. The binary region holds patch XML blobs — not audio. Each
blob is a `.prt_<ext>` file (single-Part patch, like `.mlt_*` but with an
`<AmberPart>` root instead of `<SynthMaster>`/multi). Inside is the same
SynthMaster XML schema documented above — `<SynthEngine>`, `<ARP>`, `<OSC>`,
`<FILTER>`, `<LFO>`, `<MOD_MATRIX>`, etc.

Counts per product:

| Product   | `.db` files in `Patches/Factory/` | Patches per `.db` (sample) |
|-----------|-----------------------------------:|----------------------------:|
| Keyscape   |  2                                 | (TBD)                       |
| Omnisphere | 26                                 | 1,575 in `Ambient Dreams.db` |
| Trilian    |  5                                 | (TBD)                       |

Omnisphere alone has on the order of **tens of thousands** of factory
patches. Extraction reuses our existing plain-XML STEAM container parser:
read the manifest, dump each `<FILE>` entry's bytes (no decryption needed,
since the binary region is already plaintext XML).

`.prt_omn` example head:

```xml
<AmberPart>
  <SynthEngine>
    <ARP ArpMode="8" ArpClock="4" ArpLen="3f800000" ArpSwing="0" …>
      <ARPSEQ2 TEMPO="…">…</ARPSEQ2>
    </ARP>
    <SynthSubEngine>×2  <!-- Layer A, B -->
    <MOD_MATRIX>
    <SLICESEQSTEP>×33
    <EFFRACK>×4
  </SynthEngine>
</AmberPart>
```

Same engine, same modules, just wrapped as a single Part instead of an 8-Part
multi.

## Suggested order of operations

1. **Extract all `.prt_*` factory patches** via sc-import (mirror the soundsource
   pipeline; Settings/Patches/Factory `.db` files use the plain-XML STEAM
   format already supported). Output: `Sampled/Synth/<Product>-Patches/`
   trees of `.prt_omn` / `.prt_key` / `.prt_trl` XML files.
2. **Parse a few `.prt_*`** in Python or Rust to extract the soundsource
   names referenced. Cross-check that the names line up with our extracted
   soundsources from `Soundsources/`.
3. **Build Tier 1 synth engine** alongside the existing sampler — sample
   oscillator + filter + AENV + 1 LFO + minimal mod matrix. Test against
   the simplest factory patches (e.g. raw Rhodes presets in Keyscape).
4. **Write `.prt_*` → Signal patch importer** that maps SynthMaster XML
   attributes to the Signal patch format. Validate by round-tripping a
   known patch (load → save → diff).
5. **Iterate Tier 2 / 3 features** as needed for specific patches we want
   to support.
