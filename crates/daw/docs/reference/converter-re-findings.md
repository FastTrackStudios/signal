# PT Reaper Converter binary RE — findings

Session: 2026-05-18. Goal: short-circuit probe-and-diff parity work by
extracting feature → PTX-byte mappings directly from the closed-source
PT Reaper Converter v1.5.4 binary.

## Setup

Project location: Ghidra project `ProTools_RE` at
`~/.ghidra-projects/ProTools_RE/`.

Imported binaries (both slices of the universal Mach-O):

| Program | Slice | Functions | Notes |
|---------|-------|-----------|-------|
| `pt_reaper_converter` | x86_64 | 615 | Mostly Swift Foundation imports; main app code (~3 MB) unanalyzed |
| `ptrc_arm64` | arm64 | 5233 | Full auto-analysis succeeded; **this is the slice to use** |
| `reaper_ptx_import.dylib` | x86_64 | 32 | REAPER importer plug-in; only handles labels for toggles |

The arm64 slice is the one to use because Ghidra's analysis identified
all functions there. The x86_64 slice's code section was treated as
data and never auto-analyzed.

## Binary architecture

Swift app. Module name: `PT_Reaper_Converter`. Discovered classes:

| Class | Role |
|-------|------|
| `PTXParser` | **Parses PTX file → in-memory representation** |
| `PTToReaperConverter` | Main converter orchestration |
| `RPPElement` | RPP output element |
| `AppDelegate` | UI / app delegate |
| `BugReportContext`, `LicenseManager`, `StoreKitManager`, `UpdateChecker` | Auxiliary |

Class method symbols are stripped (release build) — only class metadata
remains. Methods appear as `FUN_<hex>`.

## Block-parse dispatcher

`FUN_100175f6c` is the **universal block locator**. Signature inferred
from call sites:

```
FUN_100175f6c(
    void* ptx_data,      // x0 — base of decrypted PTX buffer
    long  ptx_len,       // x1 — length
    ..., ...,            // x2, x3 — varies (track ref? start offset?)
    void* search_ctx,    // x4
    u8 ct_low,           // w5 — low byte of block content_type
    u8 ct_high,          // w6 — high byte of block content_type
    void* output_blk,    // x7 — out-parameter
    u32 ct_u32?          // stack — sanity check
)
```

It is called from **46 sites** across ~30 distinct functions. Each
caller is a feature-specific decoder.

### Hard-coded CT call sites (immediate `mov w5, #lo; mov w6, #hi`)

| Caller addr | CT |
|-------------|----|
| `0x10016a8d0` | `0x103a` |
| `0x10016b600` | `0x1005` |
| `0x1001269b0` | `0x1029` (TrackMixSettings — confirmed) |
| `0x100128f3c` | `0x260c` (pan mirror) |
| `0x100139f4c` | `0x??25` (high byte register-loaded) |
| (others) | passed via registers — loop over CT array, or computed |

## RPP emit-site offsets (Frida-verified)

From `scripts/frida/harness_emit.js`. ARM64 module offsets (apply to
`pt_reaper_converter.app/.../PT Reaper Converter` after slicing arm64):

| Offset | Emit | Notes |
|--------|------|-------|
| `0x53954` | `TEMPO` | Tempo map emit |
| `0x537e4` | `SAMPLERATE` | Sample rate emit |
| `0x53ef0` | `MARKER` | Marker emit |
| `0x56a78` | `TRACK` / `ID` | Track block start |
| `0x56b20` | `VOLPAN` | Track vol+pan |
| `0x56b94` | `MUTESOLO` | Track mute+solo |
| `0x56bec` | `NCHAN` | Track channel count |
| `0x56ca4` | `MAINSEND` | Master send |
| `0x56d3c` | `FREEMODE` | Free mode flag |
| `0x571c4` | `POSITION` | Item position |
| `0x57248` | `LENGTH` | Item length |
| `0x573ac` | `SOFFS` | Source offset |
| `0x573fc` | `CHANMODE` | Channel mode |
| `0x57558` | `SOURCE` | Source file ref |
| `0x57fe0` | `PEAKCOL` | Track color |
| `0x583e4` | `ISBUS` | Bus flag |
| `0x59698` | `AUXRECV` | Aux send |
| `0x5aa58` | `NOTES` | Track notes (SWS) |
| `0x60b28` | track emit fn entry | per-track emit dispatcher |
| `0x61400` | `MUTESOLO_emit2` | Secondary mute/solo site |
| `0x65dbc` | `FADEIN` | Fade-in |
| `0x65f4c` | `FADEOUT` | Fade-out |
| `0x750bc` | `FXCHAIN` | FX chain emit |
| `0x2355b8` | `PLAYRATE` | Playrate |

### Big per-track emit function

`FUN_100052628` (41,336 bytes!) is the giant per-track emit function
that contains MOST of these emit sites. Too big for the decompiler in
one pass.

Callers:
- `FUN_100019cbc` at `0x10001b4d0`
- `FUN_10003bf50` at `0x10003c874`

## CLI flags discovered (mostly from strings)

```
Usage: PTReaperConverter --convert <input> <output> [flags] [--options '{"key":bool,...}']

Flags (PTX→RPP only):
  --mediaDir <path>      Output dir for merged/poly audio
  --copyAllAudio         Copy all session audio files
  --copyVideo            Copy video files into <out>/Video Files/

Options JSON keys (all default true):
  clipsAndMedia, clipGain, clipMute, fadeSettings,
  mainVolumeAutomation, mainPanAutomation, mainMuteAutomation, mainRoutingAssignments,
  sendVolumeAutomation, sendPanAutomation, sendMuteAutomation, sendRoutingAssignments,
  clipColors, trackColors, trackNotes, soloState, soloSafe,
  playlists, trackMarkers, tempoMapAndTimeSignature, routingFolderNesting

Hidden flags also found in strings:
  --debug-dump-spec       (effect: unclear — produces no extra output with --convert)
  --regen-clean-template  (effect: unclear)
  PTREAPER_DEBUG_LINK     env var
```

## All converter feature toggles (the "Track Data" UI list)

These appear with `td_` prefix in the binary as the internal flag set:

- `td_clipsAndMedia`
- `td_mainMuteAutomation`, `td_mainPanAutomation`, `td_mainVolumeAutomation`, `td_mainRoutingAssignments`
- `td_sendMuteAutomation`, `td_sendPanAutomation`, `td_sendVolumeAutomation`, `td_sendRoutingAssignments`
- `td_tempoMapAndTimeSignature`
- `td_trimOverlappingClips`
- `td_routingFolderNesting`

## Per-clip identifier names (converter's internal JSON IR field names)

- `clip_color_r`, `clip_color_g`, `clip_color_b` — 3-channel RGB clip color
- `clip_gain_db` — static clip gain (dB)
- `clip_gain_breakpoints` — dynamic clip gain envelope
- `original_clip_name`, `source_file`, `source_file_channel`, `source_in`
- `direction`, `curve_slope` (fade shape)
- `equal_gain`, `equal_power` (fade curve types)

## Per-track / surround identifier names

- `volume`, `mute`, `muted`, `pan`, `frontPan`, `rearPan`
- `volumeModeDirect`
- `divergenceFront`, `divergenceRear`, `divergenceFR`
- `centerPercent`
- `frontRear`
- `mainRoutingAssignments`, `sendRoutingAssignments`, `routingFolderNesting`

## State strings (PT track state markers)

```
states-hidden
states-inactive
states-solo
states-muteauto
states-trackcolor
states-buscolor
states-interleaved-only
states-noninterleaved
```

## Frida hook for offset extraction

`scripts/frida/trace_ptx_reads.js` — attempts to capture PTX-buffer
reads at each emit site by recording large allocations (malloc/mmap)
and comparing register values at emit time against allocation
boundaries.

**Status**: hooks fire correctly (TEMPO emit captured), but
malloc/mmap hooks need correct library names (got "TypeError: not a
function" — Module.findExportByName returned null on current macOS).
Need to either resolve via `Process.findModuleByName('libsystem_c.dylib')`
or use ObjC Foundation hooks for NSData allocation.

## Next steps for future RE sessions

1. **Fix the Frida malloc/mmap hooks** so allocations are tracked.
   Then run with a feature-rich fixture (LotF session) to capture
   reads correlated with each emit.

2. **Decompile each of the ~30 CT-handler functions** to find byte
   offsets they read from the parsed block. The 1029 handler
   (`FUN_1001267e0`) is already partially decompiled and reveals
   the call pattern.

3. **Decompile chunks of `FUN_100052628`** (the per-track emit
   function). Although too big to decompile whole, individual ~1KB
   segments around each emit site should work.

4. **Use `--debug-dump-spec` and `--regen-clean-template`** with
   different argument combinations — the flags exist in strings but
   produced no visible effect with `--convert`. May require a
   specific positional arg structure.

5. **Hook the JSON IR** — the converter uses `JSONEncoder` /
   `JSONDecoder` internally. Hook those to capture the full
   intermediate representation between PTX parse and RPP emit.

## Files

- `/tmp/ptrc_arm64` — extracted arm64 slice of the converter
- `/tmp/ptrc_universal` — full universal Mach-O
- `/tmp/reaper_ptx_import_universal.dylib` — REAPER importer plugin
- `/tmp/arm_funcs.json` — full function list (5233 entries)
- `/tmp/arm_syms.json` — full symbol list (71,813 entries)
- `/tmp/arm_strings.json` — strings table (10,346 entries)
- `/tmp/xrefs_175f6c.json` — all 46 callers of block-parse helper
