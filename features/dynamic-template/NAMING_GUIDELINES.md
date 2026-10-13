# Track Naming Guidelines

Rules for generating clean, meaningful track names in the hierarchy.

## Core Principle

**Every track name must have at least one piece of meaningful context.**

A user looking at a track name should immediately understand what it is without needing to look at the parent folder.

## Rules

### 1. Never Use Number-Only Names

Names like "1", "2", "3" tell you nothing on their own.

```
BAD:
Outro/
  1
  2
  3

GOOD:
Outro/
  Outro 1
  Outro 2
  Outro 3
```

### 2. Use Last Context as Prefix

When a track would otherwise be just a number, prefix it with the immediate parent's name (the last piece of context).

```
BAD:
Middle Bridge/
  1
  2
  3

GOOD:
Middle Bridge/
  Middle Bridge 1
  Middle Bridge 2
  Middle Bridge 3
```

```
BAD:
Ed/
  Crunch/
    1
    2

GOOD:
Ed/
  Crunch/
    Crunch 1
    Crunch 2
```

### 3. Avoid Generic Placeholder Names

Names like "Main" should be replaced with meaningful context when they don't add information.

```
BAD:
Guiro/
  Main      <- tells you nothing
  Shaker

GOOD:
Guiro/
  Guiro     <- now you know it's the main guiro track
  Shaker
```

**Exception:** "Main" is acceptable when it's a meaningful layer distinction alongside other layers like "DBL", "Quad", etc.

```
ACCEPTABLE:
Lead/
  Main/       <- meaningful: distinguishes from DBL, Quad layers
    Lead 1
    Lead 2
  DBL/
    DBL 1
    DBL 2
  Quad
```

### 4. Preserve Meaningful Descriptors

Don't strip context that adds meaning, even if it seems redundant.

```
BAD:
Snare/
  SUM/
    1         <- what is this?
    2
    Bottom

GOOD:
Snare/
  SUM/
    Snare 1   <- clearly a snare mic
    Snare 2
    Bottom
```

### 5. Hierarchy Should Add Context, Not Repeat It

The folder structure provides context. Track names should complement it, not duplicate entire paths.

```
BAD:
Drums/Snare/SUM/Drums Snare SUM Bottom

GOOD:
Drums/Snare/SUM/Bottom
```

## Summary

| Scenario | Bad | Good |
|----------|-----|------|
| Number-only name | `1` | `Outro 1` |
| Generic placeholder | `Main` (under Guiro) | `Guiro` |
| Missing context | `Crunch/1` | `Crunch/Crunch 1` |
| Over-qualified | `Drums Snare Bottom` | `Bottom` |
