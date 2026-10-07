---
name: Signal Touch
description: The floor setlist world for managing the Signal rig by touch. Paper sheets on a hairline grid, marker type, fluoro gaffer tape for the stacks.
colors:
  desk: "#e9ece9"
  sheet: "#ffffff"
  sheet-2: "#f6f7f5"
  up-now-wash: "#fffbe0"
  rule: "#dcdfda"
  rule-strong: "#b9bdb6"
  ink: "#121212"
  ink-2: "#3d3f43"
  ink-3: "#64666c"
  tape-gaffer: "#2a2b2f"
  tape-clean: "#19c3ff"
  tape-crunch: "#9b7bff"
  tape-drive: "#ff8a00"
  tape-lead: "#ff3b5c"
  tape-ambient: "#1ed98d"
  tape-special: "#ffd60a"
  path: "#2433ff"
  void: "#d7263d"
typography:
  display:
    fontFamily: "Archivo Variable, Archivo, system-ui, sans-serif"
    fontSize: "44px"
    fontWeight: 860
    lineHeight: 1.02
    letterSpacing: "-0.015em"
    fontVariation: "'wdth' 112"
  headline:
    fontFamily: "Archivo Variable, Archivo, system-ui, sans-serif"
    fontSize: "28px"
    fontWeight: 860
    lineHeight: 1.02
    letterSpacing: "-0.015em"
    fontVariation: "'wdth' 112"
  title:
    fontFamily: "Archivo Variable, Archivo, system-ui, sans-serif"
    fontSize: "20px"
    fontWeight: 720
    fontVariation: "'wdth' 104"
  body:
    fontFamily: "Archivo Variable, Archivo, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 560
    fontFeature: "'tnum' 1"
  meta:
    fontFamily: "Archivo Variable, Archivo, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 560
  label:
    fontFamily: "Archivo Variable, Archivo, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 760
    letterSpacing: "0.03em"
    fontVariation: "'wdth' 78"
rounded:
  tape: "2px"
  sheet: "4px"
  dot: "999px"
spacing:
  tight: "8px"
  gap: "14px"
  gutter: "22px"
  grid: "24px"
  hit: "48px"
  bar: "60px"
  row: "64px"
  song-row: "76px"
  strip: "104px"
components:
  button-primary:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.sheet}"
    rounded: "{rounded.sheet}"
    padding: "0 18px"
    height: "{spacing.hit}"
  button-outline:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.ink}"
    rounded: "{rounded.sheet}"
    padding: "0 18px"
    height: "{spacing.hit}"
  button-outline-pressed:
    backgroundColor: "{colors.sheet-2}"
    textColor: "{colors.ink}"
  button-disabled:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.ink-3}"
  tab:
    backgroundColor: "transparent"
    textColor: "{colors.ink-2}"
    rounded: "{rounded.tape}"
    padding: "0 18px"
    height: "{spacing.hit}"
  tab-active:
    backgroundColor: "{colors.tape-gaffer}"
    textColor: "{colors.sheet}"
    rounded: "{rounded.tape}"
  tape-stack:
    backgroundColor: "{colors.tape-clean}"
    textColor: "{colors.ink}"
    typography: "{typography.label}"
    padding: "4px 9px"
  tape-gaffer:
    backgroundColor: "{colors.tape-gaffer}"
    textColor: "{colors.sheet}"
    typography: "{typography.label}"
    padding: "4px 9px"
  key-box:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.ink}"
    rounded: "{rounded.tape}"
    size: "28px"
  sheet:
    backgroundColor: "{colors.sheet}"
    rounded: "{rounded.sheet}"
  row-up-now:
    backgroundColor: "{colors.up-now-wash}"
    textColor: "{colors.ink}"
    height: "{spacing.song-row}"
  library-cell:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.ink}"
    padding: "12px 14px"
    height: "96px"
---

# Design System: Signal Touch

This is the world of the touch management prototype (`prototypes/touch`). It is meant to be ported back to Dioxus/Blitz. Until that port happens, the incumbent desktop theme tokens (`crates/signal/widgets/src/theme.rs`, `features/rigs/guitar/ui`) are a separate system, and this file does not govern them.

## Overview

**Creative North Star: "The Floor Setlist"**

Every set, song and sound reads like the setlist gaffer-taped at a player's feet. Copy-paper sheets lie on a cool off-white desk, and a faint 24px hairline grid runs under every page. One heavy grotesk, Archivo with its width axis pushed wide for the marker voice and narrow for labels, does all the talking. Colour comes from fluoro gaffer tape, one roll per stack. The live signal path is drawn in a single ballpoint ultramarine and nothing else uses it. A marker strike and a marker circle are the only hand-drawn marks: the strike goes through what is done or out, the circle goes round what is up now.

Density is arm's-length. Every target is at least 48px. Song rows are 76px and list rows 64px. Titles go up to 44px in the marker weight. Nothing disappears: played songs, bypassed blocks and cancelled presets stay in place and get struck through, so a list always reads whole. The world rejects the dark modeller tile grid and the SaaS sidebar-and-cards.

**Key Characteristics:**
- Bright paper sheets with hairline edges on a cool desk with a ruled grid.
- One variable family: wide and heavy for marker headings, narrow uppercase for labels, tabular numerals everywhere.
- Fluoro tape carries stack identity. Black gaffer tape marks "this one" (the active tab, the shown variation, the profile, the open set).
- Strike and circle are the only hand-drawn marks. Both draw themselves in once over 260ms.
- Ultramarine belongs only to the live signal path.

## Colors

The neutrals are paper and ink, and the colour all comes from tape. Six fluoro stack colours each carry black ink, and one reserved blue is kept for the signal.

### Primary
- **Marker Ink** (ink): every heading, outline button border, key box, strike and circle, and the 3px focus ring. The solid primary button is filled with it. The marker colour is the same value as ink.

### Secondary
- **Fluoro Gaffer Tape** (tape-clean cyan, tape-crunch violet, tape-drive orange, tape-lead pink-red, tape-ambient green, tape-special yellow): one colour per stack, mapped by name. Used on stack headers in pickers, the colour bar along the top of a section tab, and the "starts …" tape under a song. Ink on tape is always black. Tape is laid down with multiply blending so the sheet's rules show through.
- **Gaffer Black** (tape-gaffer): selection tape. Used for the active tab, the shown variation, the profile badge, the "Open" set, and any stack the map doesn't know. Text on it is white.

### Tertiary
- **Ballpoint Ultramarine** (path): the routing line, its IN and OUT nodes, and the legend swatch, and nowhere else.
- **Void Red** (void): cancellation. Used for the strike through a cancelled preset or variation, the Cancel button outline, and the "differs" note.

### Neutral
- **Desk** (desk): the app background behind the sheets.
- **Sheet** (sheet): sheets, the top bar, the chain strip, buttons, and library cells.
- **Sheet Underlay** (sheet-2): pressed and hover state, the scrolling body of the preset sheet, group headers in the library, and a row being dragged.
- **Up-Now Wash** (up-now-wash): a pale highlighter behind the current song row, the picked library cell, and the selected sound. It always appears together with the marker circle.
- **Rule / Rule Strong** (rule, rule-strong): hairlines between rows and cells (rule), and module boundaries, bar and strip edges, inactive section-tab outlines, and disabled borders (rule-strong).
- **Ink 2 / Ink 3** (ink-2, ink-3): secondary values such as BPM and inactive tabs (ink-2). Meta lines, played or struck text, and the "out" state (ink-3).

### Named Rules
**The Tape Is the Stack Rule.** A fluoro tape colour always means a stack (Clean, Crunch, Drive, Lead, Ambient, Special). Never use one for decoration, emphasis or status. "Selected" is gaffer black.

**The One Pen Rule.** Ultramarine is the live signal path. If a surface has no signal path, it has no blue.

**The Black Ink on Tape Rule.** Text on fluoro tape is ink (#121212). Only gaffer-black tape takes white.

## Typography

**Display Font:** Archivo Variable (with Archivo, system-ui)
**Body Font:** Archivo Variable
**Label Font:** Archivo Variable at a narrow width

**Character:** One grotesk spread across its width axis. Wide and very heavy, it is a Sharpie on paper. Narrow, bold and uppercase, it is the label on a strip of tape. Body text sits at weight 500 or more, so nothing reads thin from a standing distance.

### Hierarchy
- **Display** (860, 44px, 1.02, width 112%): the item in hand, such as the song up, the preset open, or the block type open.
- **Headline** (860, 26–30px, width 112%): sheet titles. The set name is 30, Core/Time panels and the Routing page are 28, and side-sheet titles are 26. The wordmark is 24 with -0.03em tracking.
- **Title** (720–860, 18–20px, width 104%): song names in the set (20, rising to 860 for the song up), section tab names (18/840), and block names in the chain (15/780).
- **Body** (560–760, 15–17px): list rows (17), buttons (15/760), and tabs (15–17, 650 off and 800 on).
- **Meta** (560, 13–14px, ink-3): one line of facts under a title, joined by " · ".
- **Label** (760, 13px, width 78%, uppercase, +0.03em): module names over chain blocks, stack names on tape, and state words ("Next", "Playing").

### Named Rules
**The Tabular Rule.** The whole body has `tabular-nums` set. Every BPM, count, index and key lines up in columns.

**The Width Is the Voice Rule.** Hierarchy comes from the width axis plus weight, never from a second family.

## Layout

The shell has three bands. At the top is a 60px bar on the sheet with a rule-strong underline, holding the wordmark, the mode tabs (Set · Sounds · Library · Routing), the profile tape, Undo, and the rig status. In the middle is the mode page on the desk, with a 24px hairline grid (rgba(0,0,0,0.045) lines). At the bottom is a 104px chain strip that scrolls horizontally and appears on every page, so the sound is always in view.

Pages are two-column grids with a 14px gap and 14px padding. The left column is `minmax(320px, 38%)` in Set and `minmax(320px, 34%)` in Sounds. In Library, the right column becomes a fixed 360px detail panel when something is picked. Content inside a sheet sits on a 22px horizontal gutter. Headers are padded 18px 22px 14px. Repeating choices use `auto-fill` grids: section tabs at minmax(150px), sound choices at minmax(170–200px), and library cells at minmax(176px) with hairline seams. Tight groups use a gap of 8px and panel stacks 14px. Targets are at least 48px everywhere (the 40px medium tab is the only exception), with 64px list rows, 76px song rows, and 84px section tabs.

Pickers open in a side sheet that slides in from the right (460px, max 92%, 220ms) over a 22% ink scrim. They are never centred modals.

## Elevation & Depth

Depth means paper lying on a desk. A sheet has one soft lift, and everything inside it is flat and separated by rules, not by shadow.

### Shadow Vocabulary
- **Paper lift** (`box-shadow: 0 1px 0 rgba(0,0,0,0.04), 0 6px 16px -10px rgba(0,0,0,0.18)`): every `.sheet`, which means page sheets, Core/Time panels, and the side sheet.
- **Held** (`box-shadow: 0 10px 24px -8px rgba(0,0,0,0.35)`): a routing block while it is being dragged, and only then.
- **Tape edge** (`box-shadow: inset 0 -1px 0 rgba(0,0,0,0.12)`): the bottom edge of a strip of tape.
- **Drop line** (`box-shadow: inset 0 3px 0 #121212`): the insertion point while reordering a set.

### Named Rules
**The Ruled, Not Raised Rule.** Rows, cells and blocks inside a sheet are divided by 1px hairlines. Only the sheet itself lifts, plus whatever a finger is holding.

## Shapes

Corners are nearly square. Sheets, buttons, section tabs and routing blocks use 4px. Tape-like and boxed items (tabs, the key box, the BPM stepper) use 2px. The side sheet is square-edged against the screen. Fully round shapes are reserved for dots: the bypass switch (14px ring, filled when the block is in), the status ring, and the IN and OUT nodes on the signal path. Borders carry the weight. Interactive outlines are 2px ink, a selected section tab is 3px ink, and an open slot ("+ Section") is a 2px dashed rule-strong. Tape may tilt by ±0.7–1°. Nothing else rotates.

The two marks are SVG paths with round caps and non-scaling strokes. The **strike** is a slightly wavering line through the middle of the text, 3px by default, 2–2.5px in dense rows, and 4px through a cancelled preset title. The **circle** is an open marker loop drawn outside its subject (inset -6 to -9px, 2.5px stroke). Both draw on once over 260ms with the shared ease (`cubic-bezier(0.2, 0.8, 0.2, 1)`) and stay static under reduced motion.

## Components

### Buttons
Tactile and unambiguous: an ink outline, or solid ink for the one thing you came to do.
- **Shape:** 4px corners, 2px ink border, at least 48px tall, padding 0 18px, 15px/760.
- **Primary:** an ink fill with white text, at most one per area ("Add songs").
- **Outline:** sheet with an ink border and ink text ("Done", "Remove", "Clear its sound").
- **Pressed / Hover:** scales to 0.985 and fills with sheet-2 over 140ms. Hover applies only on hover-capable pointers. Focus is a 3px ink outline offset 2px.
- **Disabled:** the border goes rule-strong and the text ink-3.
- **Destructive:** an outline button with a void border and void text.

### Tabs
- Selection works like a strip of tape. Inactive tabs are transparent with ink-2 text at 650. The active tab is gaffer black with white text at 800. Corners are 2px. Large (48px, 17px) is for mode tabs and medium (40px, 15px) for in-page filters. An optional count follows in 12px tabular numerals at 70% opacity.

### Tape
- A label-voice strip in a stack colour with black ink and multiply blending, padded 4px 9px, with a tape-edge inset at the bottom. A gaffer-black strip means "this one" and takes white text. A sheet-coloured strip with a rule-strong border is an unselected variation or an attached-use chip.

### Key Box
- A song's key in a 2px ink box with 2px corners, 800 weight. It is 28px tall in rows and 44px next to the song title.

### Section Tab (signature)
- A 4px-cornered sheet tile at least 84px tall, topped with a 10px bar of its sound's stack tape (rule-strong if it has no sound yet). Below the bar are the section name (18/840) and its sound in meta. Selected: a 3px ink border. Playing now: the marker circle round the name.

### Set Row (signature)
- 76px. The index sits in 18px tabular numerals, then the song name (20px), a "starts …" tape, a "Next" label, the key box, the BPM in ink-2, and a 44px drag handle. A played row has its index and name in ink-3 with a marker strike through the name. The current row gets the up-now wash and a circle round its index.

### Library Cell
- Whole-cell targets in a hairline grid: at least 96px, padding 12px 14px, a title then meta. The picked cell gets the up-now wash and a circle. Group headers are sheet-2 with a 2px ink underline and a tabular count.

### Chain Strip (signature)
- A horizontal strip of blocks in signal order, each at least 138px. The module name appears as a label only over the first block of each module, and module boundaries are rule-strong. Each block shows its name (15/780) and current pick in meta, with a 40px bypass switch on the right. A bypassed block drops to 55% opacity, gets a marker strike, and reads "out". It stays in place.

### Side Sheet
- Right-anchored. A marker title (26px) and a meta subline, with a "Done" outline button in the header. The body is a ruled list of 56px rows, and the chosen row is in heavier weight with a "Playing" label.

## Do's and Don'ts

### Do:
- **Do** put every value on the sheet in tabular numerals, and keep counts and BPM in ink-2 or ink-3 beside their subject.
- **Do** strike what is done, out or cancelled, and leave it in place. Ink strikes mean played or bypassed, and void strikes mean cancelled.
- **Do** circle exactly what is up now, and pair the circle with the up-now wash on rows and cells.
- **Do** colour a stack with its own tape and black ink, and use gaffer black for "this one".
- **Do** keep every target at least 48px (the `--hit` token) and divide rows with 1px rules.
- **Do** open pickers as a right-hand side sheet over the page, never as a centred modal.

### Don't:
- **Don't** use a fluoro tape colour for anything but its stack, or ultramarine for anything but the live signal path.
- **Don't** hide a played song, a bypassed block or a cancelled preset. Nothing vanishes; it cancels.
- **Don't** add hand-drawn marks beyond the strike and the circle (no scribbles, arrows or underlines drawn as marks).
- **Don't** lift rows, cells or blocks with shadows. Only the sheet and a held block lift.
- **Don't** introduce a second typeface. Hierarchy comes from Archivo's width and weight.
- **Don't** set label text below 12px.
