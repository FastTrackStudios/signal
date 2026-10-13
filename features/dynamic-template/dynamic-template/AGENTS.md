# Dynamic Template Module

This module provides automatic track organization using the `monarchy` library for hierarchical sorting based on pattern matching.

## Debugging with the CLI

The `dynamic-template` binary is a CLI tool for testing and debugging track sorting. It helps you understand:
- How input strings are parsed into `ItemMetadata`
- Which groups match and in what order
- What display name would be generated
- How the final tree structure looks

### Building the CLI

```bash
cargo build -p dynamic-template --bin dynamic-template
```

### Usage

```bash
# Single input - shows detailed parsing info
cargo run -p dynamic-template --bin dynamic-template -- "Kick In"

# Multiple inputs - shows tree structure
cargo run -p dynamic-template --bin dynamic-template -- "Kick In" "Kick Out" "Snare Top"

# Verbose mode - shows parsing details for each input before the tree
cargo run -p dynamic-template --bin dynamic-template -- -v "Acc Guitar" "Electric Guitar"

# Force tree output for single input
cargo run -p dynamic-template --bin dynamic-template -- -t "Kick In"

# JSON output for metadata
cargo run -p dynamic-template --bin dynamic-template -- -j "SNR VERB"
```

### Flags

- `-v, --verbose` - Show detailed parsing information for each input
- `-t, --tree` - Force tree output even for single input
- `-j, --json` - Output metadata as JSON

### Understanding the Output

#### Single Input Mode

```
═══════════════════════════════════════════════════════════════
INPUT: "Kick In"
═══════════════════════════════════════════════════════════════

PARSED METADATA:
────────────────────────────────────────────────────────────────
  group:       ["Drums", "Drum_Kit", "Kick"]
  multi_mic:   ["In"]

MATCHED GROUPS:
────────────────────────────────────────────────────────────────
  ├─ Drums (prefix: Some("D"))
  ├─ Drum_Kit (prefix: None)
  └─ Kick (prefix: None)

DISPLAY NAME:
────────────────────────────────────────────────────────────────
  "Kick In"

FINAL STRUCTURE (single item tree):
────────────────────────────────────────────────────────────────
-D Drums
--Kick: [Kick In]
```

- **PARSED METADATA**: Shows what metadata fields were extracted from the input
- **MATCHED GROUPS**: Shows the hierarchy of groups that matched, with their prefixes
- **DISPLAY NAME**: Shows what the track would be named in the DAW
- **FINAL STRUCTURE**: Shows the tree structure after collapse optimization

#### Multiple Input Mode

Shows a tree structure and how it would appear as DAW tracks:

```
STRUCTURE TREE:
────────────────────────────────────────────────────────────────
-D Drums
--Kick
---In: [Kick In]
---Out: [Kick Out]
--Snare: [Snare Top]

TRACKS (as they would appear in DAW):
────────────────────────────────────────────────────────────────
📁 Drums
  📁 Kick
       In [1 item(s)]
          └─ "Kick In"
       Out [1 item(s)]
          └─ "Kick Out"
     Snare [1 item(s)]
        └─ "Snare Top"
```

### Common Debugging Scenarios

#### Why isn't my track matching the right group?

Use single-input mode to see matched groups:
```bash
cargo run -p dynamic-template --bin dynamic-template -- "My Track Name"
```

Check:
1. Are the expected groups in "MATCHED GROUPS"?
2. Is the group hierarchy correct (parent -> child order)?
3. Are there any `unparsed` words that should have been recognized?

#### Why are my subgroups being collapsed?

The collapse algorithm removes unnecessary intermediate folders. A subgroup is kept when:
- Multiple different subgroups have items (e.g., both "Acoustic" and "Electric" guitars)
- The subgroup has its own children

A subgroup is collapsed when:
- All items in the parent go to the same single subgroup
- Example: If you only have acoustic guitars, `Guitars -> Acoustic: [items]` becomes `Guitars: [items]`

Test with multiple item types to see subgroup preservation:
```bash
cargo run -p dynamic-template --bin dynamic-template -- "Acc Guitar" "Electric Guitar"
```

#### Why is the display name wrong?

The display name is generated from metadata. If it falls back to the original name, it means:
- No "primary" metadata was extracted (performer, arrangement, multi_mic, track_type)
- Check `PARSED METADATA` to see what was extracted

#### Testing exclusion patterns

Some groups have exclusion patterns (e.g., Room mics exclude snare-related words):
```bash
# Should go to Snare, not Rooms
cargo run -p dynamic-template --bin dynamic-template -- "SNR VERB"

# Should go to Rooms
cargo run -p dynamic-template --bin dynamic-template -- "Room OH"
```

### Running Tests

```bash
# Run all dynamic-template tests
cargo test -p dynamic-template

# Run a specific test with output
cargo test -p dynamic-template radiohead_paranoid_android -- --nocapture

# Run tests matching a pattern
cargo test -p dynamic-template acc_guitar -- --nocapture
```

## Architecture

- `lib.rs` - Main module, defines `default_config()` and track conversion
- `item_metadata.rs` - The `ItemMetadata` struct with all track metadata fields
- `groups/` - Group definitions (Drums, Guitars, Bass, etc.)
- `metadata_patterns.rs` - Patterns for metadata extraction (performer, arrangement, etc.)

## Key Concepts

### Groups
Groups define patterns that match input strings. Groups can be nested to create hierarchies.

### Metadata Fields
Fields extracted from input strings (performer, arrangement, multi_mic, etc.) that affect grouping and display names.

### Collapse Behavior
The monarchy library collapses unnecessary hierarchy levels:
- Single-child folders with same items get merged
- Intermediate empty folders are removed
- This creates cleaner track structures
