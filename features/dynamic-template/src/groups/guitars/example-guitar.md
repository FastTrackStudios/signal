# Electric Guitar Group Structure

## Overview

The electric guitar group has a complex hierarchical structure that organizes tracks by multiple dimensions.

## Hierarchy

The main configuration hierarchy is:

```
Group → Nested Groups → Section → Performer → Arrangement → Layers → Channels -> MultiMic
```

### Configuration Details

- **Sections**: Taken from the main config. Set to: `Verse`, `Chorus`, `Bridge`
- **Performers**: List of names from the main config. Set to: `John`, `Cody`, `JT`, `Bri`
- **Arrangements**: Guitar-specific purposes. Options:
  - `Clean`
  - `Crunch`
  - `Drive`
  - `Lead`
  - `Pick`
  - `Chug`
- **Layers**: From the main config metadata patterns. Set to: `DBL`, `TPL`, `Double`, `Triple`, `Main`
- **Channels**: From the main config metadata patterns. Set to: `L`, `C`, `R`, `Left`, `Center`, `Right`

## Full Example Structure

Here's an example of a complete guitar group structure:

```
Guitar
-Verse
--Cody
---Clean [Guitar Verse Cody Clean]
---Drive [Guitar Verse Cody Drive]
---Lead [Guitar Verse Cody Lead]
-Chorus
--Cody
---Clean
----Main [Guitar Chorus Cody Clean]
----DBL [Guitar Chorus Cody Clean DBL]
---Drive
----Main [Guitar Chorus Cody Drive]
----DBL [Guitar Chorus Cody Drive DBL]
---Lead
----Main [Guitar Chorus Cody Lead]
----DBL [Guitar Chorus Cody Lead DBL]
-Bridge
--John
---Clean
----Main [Guitar Bridge John Clean]
----DBL [Guitar Bridge John Clean DBL]
---Drive
----Main [Guitar Bridge John Drive]
----DBL [Guitar Bridge John Drive DBL]
---Lead
----Main
-----L [Guitar Bridge John Lead L]
-----C [Guitar Bridge John Lead C]
-----R [Guitar Bridge John Lead R]
----DBL
-----L [Guitar Bridge John Lead DBL L]
-----C [Guitar Bridge John Lead DBL C]
-----R [Guitar Bridge John Lead DBL R]
```

## Special Cases

### Multi-Mic Positions

When multi-mic positions are present, the track without the multi-mic position goes on the folder track.

**Input:**
```
Guitar Clean
Guitar Clean Amp
Guitar Clean DI
Guitar Drive
Guitar Drive Amp
Guitar Drive DI
```

**Output:**
```
Guitar
-Clean [Guitar Clean]
--Amp [Guitar Clean Amp]
--DI [Guitar Clean DI]
-Drive [Guitar Drive]
--Amp [Guitar Drive Amp]
--DI [Guitar Drive DI]
```

> **Note:** Arrangement is not needed as a separate level because there's only one arrangement under the Guitar group.

## Implementation Examples

### 1. Single Track

**Input:**
```
Guitar Clean DBL L
```

**Output:**
```
Guitar: Guitar Clean DBL L
```

A single track with no grouping needed.

---

### 2. Multiple Arrangements

**Input:**
```
Guitar Clean
Guitar Drive
```

**Output:**
```
Guitar
-Clean
-Drive
```

Arrangements are grouped under the Guitar folder.

---

### 3. Guitars with Multi-Mics

**Input:**
```
Guitar Clean
Guitar Clean Amp
Guitar Clean DI
```

**Output:**
```
Guitar: [Guitar Clean]
-Amp: [Guitar Clean Amp]
-DI: [Guitar Clean DI]
```

The base track (without multi-mic) goes on the folder track.

**With Multiple Arrangements:**

**Input:**
```
Guitar Clean
Guitar Clean Amp
Guitar Clean DI
Guitar Drive
Guitar Drive Amp
Guitar Drive DI
```

**Output:**
```
Guitar
-Clean
--Amp
--DI
-Drive
--Amp
--DI
```

---

### 4. Adding Layers

Layers allow multiple takes of the same multi-mic setup. The track without a layer keyword becomes "Main".

**Input:**
```
Guitar Clean
Guitar Clean Amp
Guitar Clean DI
Guitar Clean DBL
Guitar Clean Amp DBL
Guitar Clean DI DBL
```

**Output:**
```
Guitar
-Main [Guitar Clean]
--Amp
--DI
-DBL [Guitar Clean DBL]
--Amp
--DI
```

> **Note:** The track without the layer keyword is assumed to be "Main" and goes on the folder track. Arrangement isn't needed because there's only one arrangement underneath the Guitar group (similar to the Drums → Drum Kit example).

---

### 5. Adding Channels

When channels are present, they are organized similarly to layers.

**Input:**
```
Guitar Clean L
Guitar Clean Amp L
Guitar Clean DI L
Guitar Clean C
Guitar Clean Amp C
Guitar Clean DI C
Guitar Clean R
Guitar Clean Amp R
Guitar Clean DI R
```

**Output:**
```
Guitar
-L
--Amp
--DI
-C
--Amp
--DI
-R
--Amp
--DI
```

---

### 6. Layers and Channels Together

When both layers and channels are present, **layers take priority**.

**Input:**
```
Guitar Clean Main L
Guitar Clean Amp Main L
Guitar Clean DI Main L
Guitar Clean Main C
Guitar Clean Amp Main C
Guitar Clean DI Main C
Guitar Clean Main R
Guitar Clean Amp Main R
Guitar Clean DI Main R
Guitar Clean DBL L
Guitar Clean Amp DBL L
Guitar Clean DI DBL L
Guitar Clean DBL C
Guitar Clean Amp DBL C
Guitar Clean DI DBL C
Guitar Clean DBL R
Guitar Clean Amp DBL R
Guitar Clean DI DBL R
```

**Output:**
```
Guitar
-Main
--L
---Amp
---DI
--C
---Amp
---DI
--R
---Amp
---DI
-DBL
--L
---Amp
---DI
--C
---Amp
---DI
--R
---Amp
---DI
```

## Sorting Priority

The sorting priority is:

1. **Arrangement** (Clean, Drive, Lead, etc.)
2. **Layer** (Main, DBL, TPL, etc.)
3. **Multi-Mic** (base track first, then Amp, DI)
4. **Channel** (L, C, R)

This ensures that related tracks are grouped together logically, with the most important organizational dimension (arrangement) at the top level.
