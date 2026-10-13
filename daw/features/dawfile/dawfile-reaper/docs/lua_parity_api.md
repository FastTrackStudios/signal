# Lua Parity API Matrix

This crate now exposes a compatibility facade for the core ReaTeam parser API.

| Lua function | Rust compatibility symbol |
|---|---|
| `ReadRPP` | `ReadRPP` |
| `ReadRPPChunk` | `ReadRPPChunk` |
| `CreateRPP` | `CreateRPP` |
| `CreateRTokens` | `CreateRTokens` |
| `CreateRChunk` | `CreateRChunk` |
| `CreateRNode` | `CreateRNode` + `CreateNodeInput` |
| `AddRChunk` | `AddRChunk` |
| `AddRNode` | `AddRNode` |
| `AddRToken` | `AddRToken` |
| `StringifyRPPNode` | `StringifyRPPNode` |
| `WriteRPP` | `WriteRPP` |

Notes:
- Function names intentionally keep Lua-style casing for straightforward porting.
- Behavioral parity for parent-linked mutation, filter/range APIs, and copy/remove semantics is delivered by follow-up parity beads.

## Migration Notes

### 1) Reading RPP from file/string/lines

```rust
use dawfile_reaper::{ReadRPP, ReadRPPChunk, ReadRPPChunkLines};

let root = ReadRPP("session.rpp")?;
let root2 = ReadRPPChunk(r#"<REAPER_PROJECT 0.1 "7.0/x64" 123
  RIPPLE 0 0
>"#)?;
let lines = vec![
  r#"<REAPER_PROJECT 0.1 "7.0/x64" 123"#.to_string(),
  "RIPPLE 0 0".to_string(),
  ">".to_string(),
];
let root3 = ReadRPPChunkLines(&lines)?;
```

### 2) Notes text (pipe-prefixed storage)

`setTextNotes("a\nb")` stores child lines as `|a` / `|b`; `getTextNotes()` returns plain text.

### 3) GUID stripping modes

```rust
use dawfile_reaper::{GuidStripPolicy, RChunk};

let mut chunk: RChunk = /* ... */;
chunk.strip_guid_with_policy(GuidStripPolicy::LuaCompat); // GUID/IGUID/TRACKID
chunk.strip_guid_with_policy(GuidStripPolicy::Extended);  // +FXID/EGUID
```

`StripGUID()` (Lua-style method) uses `LuaCompat`.

### 4) FXCHAIN bridge workflow

```rust
use dawfile_reaper::{parse_fxchain_tree, fx_tree_to_rfxchain_text};

let mut tree = parse_fxchain_tree(fxchain_text)?;
// mutate tree...
let out_fxchain = fx_tree_to_rfxchain_text(&tree);
```

### 5) Common gotchas

- Lua indices are 1-based; range-aware methods in parity APIs use Lua-style boundaries.
- `RNode::remove` needs explicit parent chunk in Rust (`node.remove(&mut parent)`).
- Programmatic FX nodes without raw plugin blocks are serialized using synthetic minimal plugin headers.
