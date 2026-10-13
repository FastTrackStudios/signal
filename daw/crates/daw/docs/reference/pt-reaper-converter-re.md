# RE notes — PT Reaper Converter v1.5.4

The third-party "PT Reaper Converter" app (Mac universal binary in
`~/Downloads/pt-reaper-converter-v1.5.4.dmg`) converts both directions
between `.ptx` and `.rpp` and supports far more fields than our parser
currently does — including correctly distinguishing track-level mute
from clip-level mute and send-level mute. Extracting some structural
hints here for future RE.

## Binaries

| Path | Size | Role |
|---|---|---|
| `PT Reaper Converter.app/Contents/MacOS/PT Reaper Converter` | 6.7 MB | Standalone converter (Swift, universal x86_64 + arm64). The actual `.ptx` parser. |
| `reaper_ptx_import.dylib` | 178 KB | REAPER plugin wrapper. Just spawns the standalone app via temp file. Not useful for RE. |

## Build characteristics

- Language: **Swift** (with Objective-C runtime, SwiftUI)
- Mangled class prefix: `_TtC19PT_Reaper_Converter` (Swift v3) → e.g.
  `_TtC19PT_Reaper_Converter9PTXParser` = `PT_Reaper_Converter.PTXParser`
- Some debug symbols stripped — only 6 `.swift` file paths survive
- Type metadata for PTX entities visible as plain ASCII strings

## Swift classes related to mute (read side)

```
PTXParser                — top-level .ptx reader
PTXTrack                 — a parsed track
PTXTrackSpec             — track definition (name, channels)
PTXTrackStateSpec        — track mix state (vol, pan, mute…)
PTXMutePoint             — single mute automation breakpoint
```

## Mute field nomenclature (used by the writer side; same vocab as read)

| Field | Meaning |
|---|---|
| `mainMute` | Track-level mute (the M button) — boolean |
| `mainMuteAutomation` | Time-varying main mute envelope |
| `clipMute` | Per-clip / per-region mute |
| `sendMuteAutomation` | Send-level mute over time |

The user's confusion about "wrongly muted" tracks in our parser may be
that `0x1029 +5` is **NOT** `mainMute`. It might be `clipMute` or a
default-clip-mute applied to bounced/print tracks. The converter app
treats these as separate concepts; our parser conflates them.

## Confirmed: `0x1029` is referenced in the binary

Binary scan for the immediate constant `0x1029` (LE bytes `29 10`) as
either a `cmp` or `push` instruction:

| File offset | Pattern |
|---|---|
| `0x10241b` | `3D 29 10 00 00` — `cmp eax, 0x1029` |
| `0x15b4a3` | `68 29 10 00 00` — `push 0x1029` |
| `0x1a0302` | `68 29 10 00 00` — `push 0x1029` |
| `0x2378b9` | `3D 29 10 00 00` — `cmp eax, 0x1029` |

So the parser does dispatch on content_type=`0x1029`. The work needed
is to follow the cmp/push targets to see what fields it reads and at
what payload offsets.

## What's blocking deeper analysis

1. Swift symbol mangling — Ghidra's auto-analysis identified only
   1229 functions of which only ~32 had recognized names.
2. Significant inlining + optimizations — the relevant logic is
   spread across many small leaves.
3. Standard ObjC/Swift analyzer in Ghidra does not unmangle Swift v3
   class refs reliably for this binary.

## Update: Swift type metadata recovered via ipsw

`ipsw swift-dump --demangle` extracts the complete Swift type model
from the binary even though function symbols are stripped, because
Swift embeds class/struct/enum layout metadata for reflection. Full
dump is in `docs/pt-reaper-converter-swift-dump.txt`. The mute-relevant
structs:

```swift
struct PTXTrackStateSpec {
    var color: UInt8?       // ← static color
    var active: Bool?
    var hidden: Bool?
    var solo: Bool?
    var soloSafe: Bool?
    var mute: Bool?         // ← static M button
    var sends: [String: PTXSendStateSpec]
}

struct PTXSendStateSpec {
    var mute: Bool?
    var preFader: Bool?
    var active: Bool?
}

struct PTXTrackSpec {
    var name: String
    var type: String
    var format: String
    var output: String?
    var input: String?
    var color: UInt8?
    var active: Bool
    var hidden: Bool
    var solo: Bool
    var soloSafe: Bool
    var volume: [(pos: Int, raw: Int)]?  // ← automation breakpoints
    var mute: [(pos: Int, raw: Int)]?    // ← MUTE AUTOMATION
    var pan: [(pos: Int, raw: Int)]?
    var surroundPan: PTXSurroundPanSpec?
    var sends: [String: PTXSendSpec]
    var playlists: [PTXPlaylistSpec]
    var notes: String
    var children: [String]
    var clips: [PTXClipSpec]
}

struct PTXClipSpec {
    var poolFile: String
    var start: Int
    var end: Int
    var sourceIn: Int
    var name: String?
    var fades: [PTXFadeSpec]
    var clipGain: PTXClipGain?
    var color: UInt8?
    var muted: Bool         // ← per-clip mute
}

struct PTXMutePoint {
    let positionSamples: Int
    let muted: Bool         // ← one automation breakpoint
}
```

**Conclusion on mute architecture:** PT stores mute in FOUR distinct
places (static track mute, mute automation envelope, per-clip mute,
per-send mute). Our parser reads `0x1029 +5` which is most likely
`PTXTrackStateSpec.mute`. The user-visible mute state in PT's UI is
the *effective* mute = `mute || (automation evaluated at playhead)`.

For the over-muted tracks (SYZ, AC GTR, El Gtr 1, Bass Demo): their
`PTXTrackStateSpec.mute` is `true` but they likely have a mute
*automation envelope* that overrides it to `false` at playhead, OR
PT treats `active: false` as a distinct visual state from "muted".

For the under-muted Inst dups: those alternate playlists inherit
the parent's mute state via the PT mixer — not stored directly on
the alternate.

## Other recovered field models (selected highlights)

```swift
struct PTXClipGainBreakpoint {
    let positionSamples: Int
    let valueDB: Double
}

struct PTXMarker {
    let number: Int
    let name: String
    let positionSamples: Int
    let color: UInt8?       // ← MARKERS HAVE COLORS too
}

struct PTXAudioFileInfo {
    var filename: String
    var durationSamples: Int
    var sampleRate: Int
    var channels: Int
    var bitDepth: Int
    var hash1: Data?         // ← Avid Unique ID
    var hash2: Data?
    var fileUUID: Data?
}

struct PTXRegion {
    let name: String
    let index: Int
    var sampleRate: Int
    var durationSamples: Int
    var sourceIn: Int
    var sourceLength: Int
    var clipDuration: Int
    var sourceFile: String
    var sfIdx: Int          // ← the source-file index we couldn't read
}

struct PTXBus {
    let id: Int
    let name: String
    let format: String
    let channels: Int
    let isPhysicalOutput: Bool
    let parentBusId: Int?   // ← bus hierarchy
}

struct PTXFade {
    let trackName: String
    let regionName: String
    let timelineStart: Int
    let fadeDirection: String
    let curveSlope: String
    let curveShape: String
    let durationSamples: Int
    let crossfadeStart: Int
}
```

These give us the complete field set the upstream tool extracts.
Promote each as a roadmap target.

## PTXBlocks enum case content_type values

By scanning the binary for `mov ax, imm16` followed by `pop rbp; ret`
(the Swift enum `rawValue.getter` accessor pattern), recovered the
full enum-case-value list. Each case is a content_type the upstream
parses:

| case_va         | content_type | Our parser knows it as |
|-----------------|--------------|----|
| 0x1000d91a0     | 0x1028 | SessionSampleRate ✓ |
| 0x1000d91b0     | **0x204d** | **unknown** |
| 0x1000d91c0     | 0x2028 | TempoBlock ✓ |
| 0x1000d91d0     | 0x1054 | AudioRegionTrackMapNew ✓ |
| 0x1000d91e0     | 0x1052 | AudioRegionTrackMapEntriesNew ✓ |
| 0x1000d91f0     | 0x1050 | AudioRegionTrackEntryNew ✓ |
| 0x1000d9200     | 0x104f | AudioRegionTrackSubEntryNew ✓ |
| 0x1000d9210     | **0x0031** | **unknown** (small block) |
| 0x1000d9220     | 0x251a | MidiTrackInfo ✓ |
| 0x1000d9230     | 0x2629 | AudioRegionNew ✓ |
| 0x1000d9240     | 0x2628 | CompoundRegionGroup ✓ |
| 0x1000d9250     | **0x2637** | **unknown** |
| 0x1000d9260     | 0x262f | FadeDef ✓ |
| 0x1000d9270     | **0x2624** | **unknown** (very large block) |
| 0x1000d9280     | 0x261b | TrackContainer ✓ |
| 0x1000d9290     | 0x261c | TrackContainer (inner) ✓ |
| 0x1000d92a0     | **0x261e** | **unknown** |
| 0x1000d92b0     | **0x261f** | **unknown** |
| 0x1000d92c0     | 0x260a | (per-track aux entry, unnamed) |
| 0x1000d92d0     | 0x103a | WavNames ✓ |
| 0x1000d92e0     | 0x260d | TrackMixWrapper ✓ |
| 0x1000d92f0     | 0x260c | (per-track aux entry, unnamed) |
| 0x1000d9300     | **0x2627** | **unknown** (large per-region cache) |
| 0x1000d9310     | 0x1003 | WavMetadata ✓ |

24 block IDs total. Six are unknown to our parser — and one of them
almost certainly holds mute automation envelope data:

- `0x204d` (between session info & tempo) — likely session preferences
- `0x0031` — small. Could be ProductString version v2.
- `0x2637` — unknown
- `0x2624` — very large (multi-MB on user session); audio cache?
- `0x261e` / `0x261f` — both unknown. Strong candidates for the
  per-track AUTOMATION envelope blocks (mainMuteAutomation,
  mainVolumeAutomation, mainPanAutomation).
- `0x2627` — per-region waveform-overview cache.

## Verification of the 6 unknown content_types (on Lord of the Fight)

| ct | occurrences | inspection finding |
|---|---|---|
| `0x0031` | (likely 1) | small — probably PT product/version v2 string |
| `0x204d` | (likely 1) | between 0x1028 and 0x2028 — session-wide preference? |
| `0x2624` | 1, ~3.4 MB | huge — likely audio waveform overview cache |
| `0x2627` | 29 instances, large | per-track waveform cache (one per mixable track) |
| `0x2629` | 68 (= our AudioRegionNew, already known) | already decoded ✓ |
| `0x2637` | (very few) | TBD |
| `0x261e` | **2 instances** | wraps MIDI tracks (Click 1, Shake) — contains 0x261b. NOT automation. |
| `0x261f` | **0 instances** | absent on this session — track-specific automation only when present? |

So `0x261e`/`0x261f` are not the automation envelope blocks. The
mute automation must live in a content_type that's NOT in PTXBlocks
enum — likely handled by a different dispatch path in the converter
(perhaps via a separate Swift enum for automation block IDs).

## Mute automation — open question

Given the Swift model declares `PTXTrackSpec.mute: [(pos, raw)]?` as
a list of breakpoints, PT MUST store these somewhere. Candidates yet
to investigate:

1. **Inside the 281-byte `0x1029` payload itself** — bytes `+50..+170`
   are mostly zero but might hold inline automation tables when the
   track has any automation set.
2. **A separate per-track block we haven't enumerated** that exists
   only when automation is present (which would explain why our
   "tracks with no automation" don't have it).
3. **The 274 KB single `0x261d` top-level container** holds everything
   and may include automation tables in a section we haven't found.

## Useful next steps (if continuing this thread)

1. Pull Ghidra's Swift symbolicator plugin or use `swift-demangle`
   manually on `_$s` and `_TtC` strings to give functions friendly
   names.
2. Manually mark the 4 `0x1029` call sites' parent functions, study
   them as the entry points for TrackMixSettings parsing.
3. Cross-reference `mainMute` / `clipMute` string offsets to find
   which parser produces the named fields.
4. Alternative cheaper path: invoke the converter on the
   Lord-of-the-Fight session and observe what mute state it emits
   for each track. The output `.rpp` becomes ground truth.

## 2026-05-17 round: dispatcher disassembly

Imported binary into Ghidra (`ProTools_RE` project,
`pt_reaper_converter` program, x86_64 slice). Auto-analysis surfaced
only 615 named functions — every internal PTX parser remains
`FUN_xxxxxx` without function-boundary recognition, because the
auto-analyzer can't follow Swift's indirect-call-through-type-metadata
pattern. `ghidra-cli analyze` is idempotent, and inline `script
python`/`script java` are blocked in bridge mode, so forcing function
boundaries via script is non-trivial via CLI.

Direct objdump on `__TEXT,__text` disassembled the 4 sites that
reference `0x1029` (the byte-pattern hit list from earlier scan):

| Site | Instruction | Role |
|---|---|---|
| `0x1000fe41b` | `cmpl $0x1029, %eax` | Block-scanner (skipper) |
| `0x1001574a3` | `pushq $0x1029` | Caller of `find_block(ct, &out)` |
| `0x10019c302` | `pushq $0x1029` | Same caller pattern |
| `0x1002338b9` | `cmpl $0x1029, %eax` | Second block-scanner (skipper) |

Key finding from disassembling `0x1000fe40d..0x1000fe48f`:

```
cmpl  $0x260b, %eax
jg    .gt_260b_branch
cmpl  $0x1029, %eax
je    .skip_payload          ; common handler for 0x1029
cmpl  $0x260a, %eax          ; sends
jne   .skip_one_byte
cmpl  $0x1c, %edx
jb    .skip_one_byte
...
.gt_260b_branch:
cmpl  $0x260c, %eax
je    .skip_payload          ; same handler!
cmpl  $0x260e, %eax           ; output routing
jne   .skip_one_byte
.skip_payload:
testq %rdx, %rdx
addq  $0x7, %rdx
addq  %rdx, %rbx              ; advance file pointer past payload
jno   .loop                   ; continue scanning
```

`0x1029`, `0x260c`, `0x260e` (and `0x260a` after a size check) share
**a single skip handler** in this function — confirming this is a
block-boundary walker, not a field parser. The actual mute/vol/pan
field reads happen in a different code path entirely.

The only candidate-parser site is `0x1001574a3`:

```
pushq  $0x1029
leaq   -0x30(%rbp), %rax    ; output pointer
pushq  %rax
pushq  $0x10                  ; size or count
callq  0x1001b9ad0           ; find_block(...)
...
movq   -0x30(%rbp), %rdi    ; load found block
cmpq   $0x0, 0x10(%rdi)      ; check field +0x10
movq   0x38(%rdi), %r13     ; read field +0x38
...
shrq   $0x3e, %rcx           ; extract tagged-pointer tag bits
leaq   0x29d(%rip), %rdx     ; jump table base
movslq (%rdx,%rcx,4), %rcx
addq   %rdx, %rcx
jmpq   *%rcx                  ; Swift enum case dispatch
```

After locating the `0x1029` block, this code:
1. Reads fields at `+0x10` and `+0x38` of a found-block struct
2. Dispatches through a Swift enum jump table (`0x29d(%rip)`)

Tracing the enum jump table is where the real parser lives — but
that's a multi-day pursuit (every case is a separate inlined Swift
function with no boundary markers in this binary).

## Practical conclusion for the mute fix

After thorough byte-level brute force (see `find_mute_v3.rs`) AND
direct dispatcher disassembly, the mute discriminator cannot be
located in this fixture without one of:

1. **`mute-automation.ptx` fixture** (priority-1 from
   `pt-sample-data-needed.md`): differential between known-muted and
   known-unmuted gives bytes directly.
2. **Multi-day RE pursuit** into the Swift enum case dispatcher at
   `0x10015750a`, naming each case manually in Ghidra GUI, and
   recovering field offsets from the WitnessTable-mediated reads.

The CLI-only ghidra-cli tooling cannot productively drive option 2
because it can't create functions or force-disassemble at addresses
without GUI interaction. Recommend either capturing the fixture or
moving to the Ghidra desktop GUI for sustained RE work.

## 2026-05-17 round 2: Frida dynamic analysis — DEFINITIVE answer

Ran the actual PT Reaper Converter on the LotF session via Frida on
voyager (macOS 26.0.1, ARM64). The .rpp output is ground truth.

**The user's earlier `LOTF_EXPECTED_MUTED` list was wrong.** Only 8
tracks are actually muted per the converter, not 17. The real muted
set:

  ClickPrint
  02 LORD OF THE FIGHT.01
  02 LORD OF THE FIGHT_Vocals
  02 LORD OF THE FIGHT_Bass
  02 LORD OF THE FIGHT_Drums
  02 LORD OF THE FIGHT_Guitar
  02 LORD OF THE FIGHT_Other
  02 LORD OF THE FIGHT_Piano

The Inst/MIDI 1/SYZ/AC GTR/El Gtr/Bass Demo tracks our parser also
marks as muted are **NOT** muted in the converter's output.

### MUTESOLO emit point located in ARM64 binary

Scanned ARM64 immediates (MOV+MOVK chains) for "MUTESOLO" inline
literals. Found 4 sites; only **0x100061400** fires at runtime. The
containing function's prologue is at **0x100060b28** — this is the
per-track RPP emitter.

### Mute decision logic disassembled

The mute=1 path at `0x1000612b0`-`0x1000612dc`:

```
ldr  w8, [sp, #0x84]         ; global "include mute" flag (always 1)
tbz  w8, #0, .not_muted
str  wzr, [sp, #0x24]
ldr  x9, [sp, #0x88]          ; Swift Optional<...> pointer
cbz  x9, .not_muted_2         ; null → NOT MUTED
ldr  x8, [x9, #0x10]          ; must equal 1
cmp  x8, #1
b.ne .not_muted_2
ldrb w8, [x9, #0x28]
tbz  w8, #0, .not_muted_2     ; bit 0 must be set
cbz  w27, .different_path     ; (folder-inheritance path likely)
mov  w21, #1                  ; w21 := 1 (mute=true)
```

Frida hook at `0x1000612b0` over a full LotF conversion captured:

| Track index | sp+0x88 ptr | [ptr+0x10] | [ptr+0x28] |
|---|---|---|---|
| 1 | 0x0 (NULL) | — | — |
| 2 (ClickPrint) | 0xb4d8435d0 | 1 | 0x1 |
| 3 (02 LORD.01) | 0xb4d843720 | 1 | 0x0 |
| 16, 17, 18, 19, 21, 23, 25, 27 | 0x0 | — | — |

**Only 2 tracks reach the decision point with a non-null mute
object.** The other 6 muted tracks (LORD family stems) reach `MUTED=1`
via a different code path — almost certainly **folder inheritance**
from their parent track "02 LORD OF THE FIGHT.01".

### Struct shape of the mute object (Swift class at 0xb500dfc08 type)

```
+0x00: type-metadata pointer  (0xb500dfc08 — same for both tracks)
+0x08: 0x200000003             (flags or count)
+0x10: 1                       (used in cmp #1 check)
+0x18: 2                       (some state)
+0x20: 0
+0x28: byte flag (varies)
+0x30: 0xfac826af2553XXXX      (UID/token)
+0x38: 0x4XXXXX                (some value)
+0x60: 0xfac826af2553XXXX      (second UID — adjacent to +0x30)
```

The `0xfac826af2553` prefix is shared across muted tracks but NOT
present in the .ptx file (confirmed by grep). So these are **runtime
synthetic IDs**, not file offsets — they're constructed during PTX
parsing. We can't directly grep the PTX for these.

### Implications for our parser

1. **The `0x1029 +5` byte we currently use as `mute` is the wrong
   discriminator entirely.** It's set on 20 LotF tracks but only 8 are
   actually muted. The +5 byte likely encodes some other PT track
   attribute (`inactive`, `bouncedSource`, `printEnabled`, or
   similar).
2. **Mute is stored as an OBJECT, not a flag.** PT records a
   `PTXMutePoint`-style object per explicitly-muted track. We need
   to locate the block ID that carries these.
3. **Folder mute inherits.** Whatever block carries mute, only the
   FOLDER PARENT has an entry — children are muted by tree walk.
4. **For LotF: only 2 tracks have explicit mute records** (ClickPrint
   + 02 LORD OF THE FIGHT.01). Finding 2 muted records inside the
   3.8MB .ptx is the new search target.

### Action items

- **Immediate**: drop the +5 byte heuristic from
  `parse/mod.rs:181`. Default mute=false. Under-muting is much
  less harmful than over-muting (REAPER user can mute manually but
  can't easily un-mute a "muted" track they didn't expect to see
  silent).
- **Short-term**: implement folder-mute inheritance in the
  `daw-reaper` PT→RPP path. When a folder track is muted, walk
  children and mute them.
- **Medium-term**: locate the mute-record block ID. Possible
  approaches:
  - Frida stalker-trace the converter while parsing LotF, log every
    .ptx file offset read, and find the bytes corresponding to
    ClickPrint's and LORD.01's mute objects.
  - Try one of the 6 unknown PTXBlocks content_types as the
    candidate (one of them is likely `MUTEENV` / mute-automation).
  - Capture `mute-automation.ptx` fixture for a clean differential.

## Strings of interest (full list)

`/tmp/pt_reaper_converter` binary contains these PTX classes (search
`PTX[A-Z]` for the full list):

```
PTXAudioFileInfo, PTXAutomationWriter, PTXAutomationWriterError,
PTXBlocks, PTXBus, PTXBusSpec, PTXClipGain, PTXClipGainWriter,
PTXClipPlacement, PTXClipSpec, PTXClipWriter, PTXExpandError, PTXFade,
PTXFadeCurve, PTXFadeSpec, PTXFadeType, PTXFadeWriter,
PTXFadeWriterError, PTXFeatureWriterError, PTXFeatureWriters,
PTXFolderNesting, PTXInputRouting, PTXLinkingOptions,
PTXMappingAnomaly, PTXMarker, PTXMarkerSpec, PTXMeterEntry,
PTXMissingFile, PTXMissingFileKind, PTXMutePoint, PTXOutputRouting,
PTXPanPoint, PTXParser, PTXPlaylistSpec, PTXPoolError,
PTXPoolFileSpec, PTXPreflightResult, PTXRegion, PTXRegistry,
PTXRelinkCandidate, PTXSendSpec, PTXSendStateSpec, PTXSendWriter,
PTXSessionBuilderError, PTXSessionSpec, PTXStats, PTXSubPathSpec,
PTXSurroundPan, PTXSurroundPanSpec, PTXSurroundPanWriter,
PTXTempoEntry, PTXTrack, PTXTrackExpander, PTXTrackSpec,
PTXTrackStateSpec, PTXUnifiedClipPipeline, PTXVideoClip,
PTXVideoFile, PTXVideoRegion, PTXVolumeTransition
```

These names tell us EVERY feature the upstream tool can read/write.
Our parser covers ~5 of these (Tracks, Regions, Markers, Tempo,
SurroundPan partial). Big gaps for our roadmap: PlaylistSpec,
SendStateSpec, FadeWriter, AutomationWriter, VolumeTransition.
