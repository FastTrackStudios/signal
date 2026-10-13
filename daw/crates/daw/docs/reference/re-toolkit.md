# RE toolkit — how to use the Frida + Ghidra pipeline

This doc is a recipe book for future RE sessions. Use it to convert
roadmap §16 "Block not located" items into ✅ decoded fields, without
needing to redo the binary analysis.

## Architecture overview

The PT Reaper Converter 1.5.4 (the closed-source reference converter)
ships as a universal Mach-O. The arm64 slice is in our Ghidra project
`ProTools_RE` as program `ptrc_arm64` (5,233 functions analyzed).

Key functions discovered:

| Function | Role |
|----------|------|
| `FUN_100175f6c` | Universal block-parse helper. Called 46× with `(magic, ct_lo, ct_hi)` args; scans the decrypted PTX buffer for matching blocks. |
| `FUN_1001267e0` | TrackMixSettings (`0x1029`) handler — reads mute byte. |
| `FUN_100052628` | 41 KB per-track RPP emit dispatcher (contains MUTESOLO, VOLPAN, PEAKCOL, etc.). Too big for the decompiler in one pass. |

Foundation runtime imports of interest:

| Symbol | What it is |
|--------|------------|
| `_$s10Foundation4DataV15_RepresentationOys5UInt8VSicig` | `Data.subscript(_:Int) -> UInt8` — byte-level read |
| `_$s10Foundation4DataV15_RepresentationOyACSnySiGcig` | Range subscript — Data slice |

Stub in main binary at base + `0x2690d4` (for `Data.subscript`).

## Recipe 1: Decode a new field via Frida byte-read tracing

This is the workflow that landed marker color + region UID this
session.

### Step 1: Build a feature-isolating probe

Edit `crates/daw-reaper/examples/rpp_to_ptx_probe.rs`:

```rust
"my_new_feature" => {
    b = b.track("ProbeTrack", |t| t.<feature_method>(<value>));
}
```

### Step 2: Run the probe-diff harness

```bash
scripts/probe_diff.sh my_new_feature
```

This will:
1. Build the probe PTX via REAPER builder + converter shellout.
2. Re-run convert under Frida `trace_all_reads.js` hook.
3. Diff byte-reads against `/tmp/reads_baseline.log`.
4. Map differing offsets to `(CT, within-block offset)`.

### Step 3: Identify the offset

The diff output shows lines like:

```
> {"msg":"read","off":30546,"val":1}
```

= "the probe causes a read at file offset 30546 with value 1, whereas
baseline read 0 at that position".

The block mapping shows:

```
offset 30546 (0x7752): [0x2624@0x7595+445] [0x261c@0x75a2+432] ... [0x1029@0x7744+14]
```

= "this offset is inside `0x1029` at within-block offset +14".

### Step 4: Wire it into the parser

Add a field to `Track` (or `Marker`, `AudioRegion`, ...) in
`crates/dawfile-protools/src/types.rs`. Read in
`crates/dawfile-protools/src/parse/<module>.rs`:

```rust
let value_at = block.offset.saturating_sub(7) + 14;  // block_start + 14
if value_at < data.len() {
    track.my_field = data[value_at];
}
```

Add a round-trip test in `crates/dawfile-protools/src/write/native.rs`.

## Recipe 2: Find what handler reads a given CT

In Ghidra, search for the CT's u32 LE bytes:

```bash
ghidra-cli find bytes --project ProTools_RE --program ptrc_arm64 "29 10 00 00"
```

Each hit is in a function that handles that CT. Inspect with:

```bash
ghidra-cli function get --project ProTools_RE --program ptrc_arm64 0x<addr>
ghidra-cli decompile --project ProTools_RE --program ptrc_arm64 --target "FUN_<addr>"
```

## Recipe 3: Map every byte-read of a fixture

```bash
# Trace
scripts/pt-convert.sh \
  --hook scripts/frida/trace_all_reads.js \
  --log /tmp/reads.log \
  <input.ptx> /tmp/out.rpp

# Map offsets to blocks
grep '"msg":"read"' /tmp/reads.log | \
  python3 -c "
import json, sys
for l in sys.stdin:
    d = json.loads(l.strip())
    print(d['off'], d['val'])
" | \
  cargo run --quiet -p daw-reaper --example map_offsets -- <input.ptx> > /tmp/mapped.txt

# Then aggregate by block CT, by within-block offset, etc.
```

## Recipe 4: Find which bytes vary across multiple probes

Use Python to load multiple `_mapped.txt` files and group by `(CT,
within-block-offset)`. Fields where VALUES vary across probes (vs
constant) are real data fields. Constant values are sentinels /
headers.

## Files

| File | Purpose |
|------|---------|
| `scripts/frida/trace_all_reads.js` | Logs every Data.subscript byte-read |
| `scripts/frida/trace_block_scan.js` | Logs every CT scan from `FUN_100175f6c` |
| `scripts/frida/trace_data_reads.js` | Earlier reads + emit-correlation variant |
| `scripts/frida/harness_emit.js` | Hooks 24 RPP emit sites for register dumps |
| `scripts/probe_diff.sh` | End-to-end probe → trace → diff → map |
| `scripts/pt-convert.sh` | Drives the converter remotely on voyager |
| `crates/daw-reaper/examples/find_blocks_at.rs` | Single offset → block |
| `crates/daw-reaper/examples/map_offsets.rs` | Bulk offsets → blocks |
| `crates/daw-reaper/examples/list_blocks_ct.rs` | List blocks of a CT |
| `crates/daw-reaper/examples/dump_bytes.rs` | Raw byte hex-dump |
| `crates/daw-reaper/examples/dump_decrypted.rs` | Dump decrypted PTX |
| `crates/daw-reaper/examples/dump_regions_idx.rs` | Region list + UIDs |
| `crates/daw-reaper/examples/dump_region_uids.rs` | Group regions by UID |
| `crates/daw-reaper/examples/check_state.rs` | Quick parser state check |
| `crates/daw-reaper/examples/check_auto.rs` | Automation envelope check |

## Roadmap §16 entries with KNOWN offsets (ready to wire)

These have been spotted via Frida traces but not yet codified:

| Feature | Block + offset | Verification probe |
|---------|----------------|---------------------|
| Routing entry active | `0x2602 +10` u8 | `routing-examples.ptx` fixture |
| Routing destination UID (6 bytes) | `0x2602 +47..+52` | (TBD — varies per entry) |
| Track output destination | `0x260e +49` u8 destination idx (single read in routing-examples) | TBD |
| Per-clip flag (mute or gain?) | `0x1050 +53` u8 | TBD — find a probe that toggles it |
| Per-clip sub-block | `0x104f` at start of 0x1050 payload | needs probe with real audio source |
| Extra track-color positions | `0x2015 +51..+52` and `+54..+55` | possibly icon/tint vs main color |
| File-list entry UID | `0x1003 +45..+50` (sentinels `2A` at +44 and `80` at +51) | LotF — verified for all 24 files |

Each of these takes a 30-60 min round-trip from probe to merged
commit using the recipes above.
