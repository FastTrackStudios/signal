# `.signalpack` binary layout

One file per instrument. Layout (little-endian):

```
┌────────────────────────── 64-byte header ──────────────────────────┐
│ [ 0.. 8)  magic        b"SIGPACK\0"                                 │
│ [ 8..12)  version      u32  (currently 1)                          │
│ [12..16)  kind         u32  (5 = FLAC-i24 body)                    │
│ [16..24)  header_len   u64  (64)                                   │
│ [24..32)  index_offset u64  (byte offset of the text index)        │
│ [32..40)  index_len    u64                                         │
│ [40..48)  prepared     u64  (sample count actually packed)         │
│ [48..64)  reserved                                                 │
├──────────────────────── sample body ───────────────────────────────┤
│ FLAC block, FLAC block, …   (one per sample, back to back, from    │
│                              offset 64; a sample's slice is         │
│                              [row.offset .. row.offset+row.bytes))  │
├──────────────────────── text index @ index_offset ─────────────────┤
│ # signalpack-index-v1                                              │
│ # spec_path   <original library.styx path>                        │
│ # spec_format styx                                                 │
│ # spec_begin                                                      │
│ …the entire library.styx, verbatim…                               │
│ # spec_end                                                        │
│ # source  offset  bytes  channels  sample_rate  num_frames samples│
│ <rel/path.wav>\t<off>\t<bytes>\t<ch>\t<sr>\t<frames>\t<samples>   │
│ …one row per packed sample…                                       │
└─────────────────────────────────────────────────────────────────────┘
```

- `source` is stored **relative to `samples_root`** (so packs are relocatable).
- The embedded styx is read back by `PlayerPatch::from_pack` → `parse_embedded_spec`.
  In convention-mode the map is built from the row `source` filenames
  (`SampleMap::from_paths`); in zone-mode from the styx `zones`.
- Code: `features/sampler/signal-sampler/src/engine/cache.rs`
  (`create_signal_pack`, `read_pack_header`, `extract_signal_pack`); constants
  `SIGNAL_PACK_MAGIC/VERSION/HEADER_LEN/KIND_FLAC_I24`.

## Editing a pack's spec without a full rebuild

The embedded styx can be rewritten in place (audio blocks untouched) via
`pack_rewrite.rs` / the retag path — but only when the sample→key mapping is
unchanged. If samples were renamed, re-partitioned, or re-extracted, **rebuild**
with `build_pack` instead.

See also `crates/signal/docs/content/sampler-file-formats.md` (canonical).
