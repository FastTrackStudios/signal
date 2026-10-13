# Plugin Bridge — PTX → RPP plugin state conversion

Goal: carry plugin settings across the PT→Reaper conversion. First target:
**Omnisphere** (Spectrasonics). Designed as a growable library (Rust registry
now, community-editable config later).

## Key insight: portable state blobs

Many modern plugins serialize their **entire patch state as one
format-agnostic blob** — identical bytes whether hosted as AAX (Pro Tools) or
VST3/AU (Reaper). For these, conversion is **extract the blob from the PT
chunk + re-wrap in the host's chunk framing** — no per-parameter mapping.
Omnisphere, FabFilter, Kontakt, Serum, etc. are this class. Only plugins with
format-divergent parameter IDs need the harder per-param map.

## Omnisphere — verified format (both sides)

State blob = an XML document `<SynthMaster vers="3.0.2c"> … </SynthMaster>`
(patch params as hex-float attributes, library refs, etc.). Byte-for-byte the
same structure in PT and Reaper.

### PT side (AAX) — extraction
- State lives in block `0x1038`, inside a Spectrasonics chunk tagged
  `SpecAMBRstRTChnk`, followed by the XML.
- Extract bytes from `<SynthMaster` to the end of `</SynthMaster>` (the
  trailing `\n ` belongs to host framing, not the document).
- N instances per session (e.g. 8 in ALL THAT I AM).

### Reaper side (VST3) — synthesis
The `<VST …>` block body is **three base64 segments** concatenated (each
base64-encoded independently, wrapped at 128 chars, each starting on a fresh
line). On read, Reaper decodes the whole stream to `seg0‖seg1‖seg2`:

- **seg0** (172 B): VST3 component header — class-ID magic `6d532b06ee5eedfe`
  + IO-pin table. **Mostly constant, but byte offset 160 (u32 LE) holds the
  seg1/state length** and MUST be patched to the synthesized seg1 length —
  otherwise Reaper reads a truncated state and the plugin loads blank/silent.
  (This was the cause of the first PoC loading Omnisphere with no patch.)
- The root state tag is `<SynthMaster vers=` — note `<SynthMaster` alone also
  matches the nested `<SynthMasterEngineParamBlock`. There is exactly one
  `</SynthMaster>` per instance (the root close).
- **seg1**: `[u32 size@0 = L+62][20 B mid header][u32 size@24 = L+3][4 B]` +
  XML (length `L`) + 46-B `JUCEPrivateData` trailer. Only the two size fields
  and the XML vary; everything else is constant.
- **seg2** (15 B): `\0Program 1\0\0\0\0\0` (program name) — constant.

`<VST>` header line (Omnisphere VST3 identity, constant):
```
<VST "VST3i: Omnisphere (Spectrasonics)" Omnisphere.vst3 0 "" 103502701{84E8DE5F9255222296FAE4133C935A18} ""
```

Synthesis (`examples/omni_convert_test.rs`) reproduces a Reaper-saved chunk
byte-exactly from its own XML, and produces a structurally-identical chunk
from a PT-extracted patch (validated: whole-stream decode = correct length,
VST3 magic, XML, trailer, program name). **Pending: confirm Omnisphere loads
the converted patch in Reaper** (`~/Downloads/OMNISPHERE_CONVERTED_TEST.rpp`
on voyager).

## Architecture (planned)

`plugin_bridge` registry. Each plugin = a serde-ready `PluginMapping`:
- **match**: PT identity (AAX bundle id / 4CC) → from parsed `PluginEntry`.
- **target**: Reaper `<VST>` header line (VST3 name, `.vst3` file, class GUID).
- **strategy**:
  - `PortableChunk { template }` — extract blob + re-wrap (Omnisphere). The
    template (seg0 / seg1-header / trailer / seg2 / size formulas) is data.
  - `ParamMap { params }` — per-parameter AAX→VST3 id/range table (future,
    pure config).

Rust defines the strategies; per-plugin data lives in
`plugin_bridge/assets/` (e.g. `omnisphere_template.json`), so plugins are
added as data and the registry can later load community config files.

## Status: integrated into the converter ✅

- **Extraction** (`dawfile-protools/src/parse/plugin_states.rs`): scans the
  per-track plugin container `0x2621` (track name = length-prefixed string at
  payload `+18`), extracts the `<SynthMaster vers=…>…</SynthMaster>` blob, and
  populates `ProToolsSession.plugin_states` (`PluginInstanceState { track_name,
  kind, state }`).
- **Registry / synthesis** (`daw-reaper/src/plugin_bridge/`): `PluginStateKind
  → PortableChunkTemplate` (Omnisphere first). `convert()` re-wraps the blob
  into the Reaper VST3 chunk (3 base64 segments, seg0 state-length field
  patched). The template is plain data → loadable from community config later.
- **Emission** (`daw-reaper/src/project_import.rs`): each converted FX is
  claimed by its track (exact track-name match first, then normalized `.NN`
  playlist-suffix match, once per track) and attached via the FX chain
  (`TrackBuilder::fx` + `FxBuilder::raw_block`).

Verified end-to-end across the 6 PNG tracking sessions: every Omnisphere
instance lands on its own instrument track with the full patch
(`ALLTHATIAM_FULL_with_Omnisphere.rpp` on voyager). The entire plugin state
transfers — all parameters, FX, mod matrix, arp, all parts — not just the
library reference.

## Adding more plugins
1. For a portable-chunk plugin: capture a Reaper-saved chunk, add a
   `PortableChunkTemplate` const + a `PluginStateKind` variant, and a detector
   in `parse_plugin_states`.
2. For format-divergent plugins: add a `ParamMap` strategy (per-parameter
   AAX→VST3 id/range table) — pure config.
3. Eventually: load the registry from community-editable config files.
