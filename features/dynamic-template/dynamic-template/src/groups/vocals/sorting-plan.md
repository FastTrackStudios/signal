# Vocals Group Sorting Plan

## Overview

The vocals group organizes tracks by vocal type (Lead vs Background), then by performer, section, layers, and channels.

## Hierarchy

The main configuration hierarchy is:

```
Vocals (transparent) → Lead Vocals / BGVs → Performer → Section → Arrangement (harmonies for BGVs) → Layers → Channels
```

**Note:** 
- Background Vocals group is called "BGVs"
- Harmonies (Soprano, Alto, Tenor, etc.) are the **Arrangement** metadata field within BGVs
- This allows easy addition of new harmony types as arrangement patterns

### Configuration Details

- **Vocal Types**: 
  - `Lead Vocals` - Patterns: `lead`, `main`, `solo`
  - `BGVs` (Background Vocals) - Patterns: `bgv`, `background`, `backing`, `harmony`, `choir`
- **Sections**: From the main config. Set to: `Verse`, `Chorus`, `Bridge`, `Intro`, `Outro`
- **Performers**: List of names from the main config. Set to: `John`, `Cody`, `JT`, `Bri`, etc.
- **Layers**: From the main config metadata patterns. Set to: `DBL`, `TPL`, `Double`, `Triple`, `Main`
- **Channels**: From the main config metadata patterns. Set to: `L`, `C`, `R`, `Left`, `Center`, `Right`

## Full Example Structure

Here's an example of a complete vocals group structure:

```
Vocals
-Lead Vocals
--Cody
---Verse
----Main [Vocal Verse Cody]
----DBL [Vocal Verse Cody DBL]
---Chorus
----Main [Vocal Chorus Cody]
----DBL [Vocal Chorus Cody DBL]
--John
---Bridge
----Main
-----L [Vocal Bridge John L]
-----C [Vocal Bridge John C]
-----R [Vocal Bridge John R]
-BGVs
--Cody
---Chorus
----Soprano [BGV Chorus Cody Soprano]
----Alto [BGV Chorus Cody Alto]
----Tenor [BGV Chorus Cody Tenor]
--JT
---Chorus
----High [BGV Chorus JT High]
----Low [BGV Chorus JT Low]
----Mid [BGV Chorus JT Mid]
```

## Special Cases

### Layers

Layers allow multiple takes of the same vocal. The track without a layer keyword becomes "Main" (default value).

**Input:**
```
Vocal Chorus Cody
Vocal Chorus Cody DBL
```

**Output:**
```
Lead Vocals
-Chorus
--Cody
---Main [Vocal Chorus Cody]
---DBL [Vocal Chorus Cody DBL]
```

### Channels

When channels are present, they organize stereo/multi-channel vocal recordings.

**Input:**
```
Vocal Chorus Cody L
Vocal Chorus Cody C
Vocal Chorus Cody R
```

**Output:**
```
Lead Vocals
-Chorus
--Cody
---L [Vocal Chorus Cody L]
---C [Vocal Chorus Cody C]
---R [Vocal Chorus Cody R]
```

## Implementation Examples

### 1. Single Track

**Input:**
```
Vocal Chorus Cody DBL L
```

**Output:**
```
Lead Vocals: Vocal Chorus Cody DBL L
```

A single track with no grouping needed (all intermediate levels collapsed).

---

### 2. Multiple Sections

**Input:**
```
Vocal Verse Cody
Vocal Chorus Cody
```

**Output:**
```
Lead Vocals
-Cody
--Verse [Vocal Verse Cody]
--Chorus [Vocal Chorus Cody]
```

Performers are grouped first, then sections under each performer.

---

### 3. Multiple Performers

**Input:**
```
Vocal Chorus Cody
Vocal Chorus John
```

**Output:**
```
Lead Vocals
-Cody
--Chorus [Vocal Chorus Cody]
-John
--Chorus [Vocal Chorus John]
```

Performers are grouped first, then sections under each performer.

---

### 4. Adding Layers

**Input:**
```
Vocal Chorus Cody
Vocal Chorus Cody DBL
```

**Output:**
```
Lead Vocals
-Cody
--Chorus
---Main [Vocal Chorus Cody]
---DBL [Vocal Chorus Cody DBL]
```

The track without the layer keyword is assumed to be "Main" and goes on the folder track.

---

### 5. Adding Channels

**Input:**
```
Vocal Chorus Cody L
Vocal Chorus Cody C
Vocal Chorus Cody R
```

**Output:**
```
Lead Vocals
-Cody
--Chorus
---L [Vocal Chorus Cody L]
---C [Vocal Chorus Cody C]
---R [Vocal Chorus Cody R]
```

Channels are organized under the section folder within each performer.

---

### 6. Layers and Channels Together

**Input:**
```
Vocal Chorus Cody Main L
Vocal Chorus Cody Main C
Vocal Chorus Cody Main R
Vocal Chorus Cody DBL L
Vocal Chorus Cody DBL C
Vocal Chorus Cody DBL R
```

**Output:**
```
Lead Vocals
-Cody
--Chorus
---Main
----L [Vocal Chorus Cody Main L]
----C [Vocal Chorus Cody Main C]
----R [Vocal Chorus Cody Main R]
---DBL
----L [Vocal Chorus Cody DBL L]
----C [Vocal Chorus Cody DBL C]
----R [Vocal Chorus Cody DBL R]
```

When both layers and channels are present, **layers take priority**.

---

### 7. BGVs with Harmony Arrangements

**Input:**
```
BGV Chorus Cody Soprano
BGV Chorus Cody Alto
BGV Chorus JT High
BGV Chorus JT Low
```

**Output:**
```
BGVs
-Cody
--Chorus
---Soprano [BGV Chorus Cody Soprano]
---Alto [BGV Chorus Cody Alto]
-JT
--Chorus
---High [BGV Chorus JT High]
---Low [BGV Chorus JT Low]
```

Harmonies are organized as Arrangement values under the BGV group.

---

### 8. Background Vocals with Multiple Performers

**Input:**
```
BGV Chorus Cody
BGV Chorus JT
BGV Chorus Bri
```

**Output:**
```
BGVs
-Cody
--Chorus [BGV Chorus Cody]
-JT
--Chorus [BGV Chorus JT]
-Bri
--Chorus [BGV Chorus Bri]
```

BGVs without explicit harmony arrangements are organized the same way as lead vocals (performer first, then section).

---

### 9. Complex Example: All Dimensions

**Input:**
```
Vocal Verse Cody Main
Vocal Verse Cody DBL
Vocal Chorus Cody Main L
Vocal Chorus Cody Main C
Vocal Chorus Cody Main R
Vocal Chorus Cody DBL L
Vocal Chorus Cody DBL C
Vocal Chorus Cody DBL R
BGV Chorus Cody Soprano
BGV Chorus JT High
```

**Output:**
```
Vocals
-Lead Vocals
--Cody
---Verse
----Main [Vocal Verse Cody Main]
----DBL [Vocal Verse Cody DBL]
---Chorus
----Main
-----L [Vocal Chorus Cody Main L]
-----C [Vocal Chorus Cody Main C]
-----R [Vocal Chorus Cody Main R]
----DBL
-----L [Vocal Chorus Cody DBL L]
-----C [Vocal Chorus Cody DBL C]
-----R [Vocal Chorus Cody DBL R]
-BGVs
--Cody
---Chorus
----Soprano [BGV Chorus Cody Soprano]
--JT
---Chorus
----High [BGV Chorus JT High]
```

## Sorting Priority

The sorting priority is:

1. **Vocal Type** (Lead Vocals, BGVs) - handled by transparent parent group
2. **Performer** (Cody, John, JT, etc.)
3. **Section** (Verse, Chorus, Bridge, etc.)
4. **Arrangement** (harmonies for BGVs: Soprano, Alto, Tenor, Baritone, Bass, High, Low, Mid, Drone, etc.) - only for BGVs
5. **Layer** (Main, DBL, TPL, etc.) - with "Main" as default
6. **Channel** (L, C, R) - only when present

This ensures that all vocals from the same performer are grouped together, then organized by section. Harmonies act as arrangements within BGVs.

## Harmony System

### Overview

Harmonies are managed as the **Arrangement** metadata field within BGVs. This approach allows:
- Easy addition of new harmony types (just add to arrangement patterns)
- Flexible pattern matching per harmony
- Controlled sorting order (order of patterns in arrangement group)
- Natural hierarchy (harmonies organize sections/performers like arrangements do for guitars)

### Harmony Arrangements

Harmonies are defined as patterns in the Arrangement metadata field group within BGVs. Each harmony can have multiple patterns that match it.

**Voice Parts:**
- `Soprano` - Patterns: `soprano`, `sop`, `s`
- `Alto` - Patterns: `alto`, `a`
- `Tenor` - Patterns: `tenor`, `t`
- `Baritone` - Patterns: `baritone`, `bar`, `bari`, `b`
- `Bass` - Patterns: `bass`, `low`

**Harmony Descriptors:**
- `High` - Patterns: `high`, `high harmony`, `high harm`, `upper`
- `Low` - Patterns: `low`, `low harmony`, `low harm`, `lower`
- `Mid` - Patterns: `mid`, `middle`, `mid harmony`, `mid harm`
- `Drone` - Patterns: `drone`, `drone harmony`, `sustained`

**Additional Common Harmonies (extensible):**
- `Harmony 1`, `Harmony 2`, `Harmony 3`, etc. - Patterns: `harmony 1`, `harm 1`, `h1`, `harmony1`, `harm1`
- `Oohs` - Patterns: `ooh`, `oohs`, `ooh harmony`
- `Aahs` - Patterns: `aah`, `aahs`, `aah harmony`
- `Ad Libs` - Patterns: `ad lib`, `adlib`, `ad libs`, `adlibs`

### Adding New Harmonies

To add a new harmony, simply add it to the arrangement patterns in the BGVs group:

```rust
let bgv_arrangement = Group::builder("Arrangement")
    .patterns([
        "Soprano", "soprano", "sop", "s",
        "Alto", "alto", "a",
        "Tenor", "tenor", "t",
        "Baritone", "baritone", "bar", "bari", "b",
        "Bass", "bass", "low",
        "High", "high", "high harmony", "high harm", "upper",
        "Low", "low", "low harmony", "low harm", "lower",
        "Mid", "mid", "middle", "mid harmony", "mid harm",
        "Drone", "drone", "drone harmony", "sustained",
        "New Harmony", "new harmony", "newharm", "nh", // Add new harmony here
    ])
    .build();
```

The arrangement will automatically:
- Match any of the patterns
- Sort according to its position in the patterns list
- Display using the matched pattern value
- Organize matching items hierarchically

### Harmony Examples

**Input:**
```
BGV Chorus Soprano
BGV Chorus Alto
BGV Chorus Tenor
BGV Chorus Bass
```

**Output:**
```
BGVs
-Cody (or performer name)
--Chorus
---Soprano [BGV Chorus Cody Soprano]
---Alto [BGV Chorus Cody Alto]
---Tenor [BGV Chorus Cody Tenor]
---Bass [BGV Chorus Cody Bass]
```

**Input:**
```
BGV Chorus High
BGV Chorus Low
BGV Chorus Mid
```

**Output:**
```
BGVs
-Cody (or performer name)
--Chorus
---High [BGV Chorus Cody High]
---Low [BGV Chorus Cody Low]
---Mid [BGV Chorus Cody Mid]
```

**Input:**
```
BGV Chorus Harmony 1
BGV Chorus Harmony 2
BGV Chorus Harmony 3
```

**Output:**
```
BGVs
-Cody (or performer name)
--Chorus
---Harmony 1 [BGV Chorus Cody Harmony 1]
---Harmony 2 [BGV Chorus Cody Harmony 2]
---Harmony 3 [BGV Chorus Cody Harmony 3]
```

## Field Configuration

### Lead Vocals
- **Performer**: Uses global metadata patterns (priority 1)
- **Section**: Uses global metadata patterns (priority 2)
- **Layers**: Uses global metadata patterns, default value: "Main" (priority 3)
- **Channel**: Uses global metadata patterns, order: L, C, R (priority 4)

### BGVs (Background Vocals)
- **Performer**: Uses global metadata patterns (priority 1)
- **Section**: Uses global metadata patterns (priority 2)
- **Arrangement**: Harmony-specific patterns (Soprano, Alto, Tenor, Baritone, Bass, High, Low, Mid, Drone, Harmony 1, Harmony 2, etc.) (priority 3)
- **Layers**: Uses global metadata patterns, default value: "Main" (priority 4)
- **Channel**: Uses global metadata patterns, order: L, C, R (priority 5)

## Collapse Behavior

- **Vocals** (top level): Transparent, so Lead Vocals and BGVs appear at top level
- **Performer**: Collapsed when it's the only performer under a vocal type
- **Section**: Collapsed when it's the only section under a performer
- **Arrangement** (harmonies for BGVs): Collapsed when it's the only arrangement under a section
- **Layer**: Collapsed when it's the only layer (e.g., just "Main")
- **Channel**: Collapsed when it's the only channel

