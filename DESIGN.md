---
name: Signal Touch
description: The Signal app's own dark zinc world, made for touch. Hairline panes on a near-black desk, stack colours as tinted chips, green for what plays, blue for what is picked, the pick pressed into the bar.
colors:
  desk: "#0a0a0c"
  sheet: "#0e0e11"
  sheet-2: "#141418"
  field: "#0a0a0d"
  rule: "#222228"
  rule-strong: "#2b2b31"
  ink: "#e4e4e7"
  ink-2: "#a1a1aa"
  ink-3: "#80808a"
  dim: "#3f3f46"
  primary: "#2563eb"
  live: "#22c55e"
  live-bg: "rgba(34, 197, 94, 0.12)"
  up: "#1b2331"
  focus-fg: "#bfdbfe"
  void: "#f87171"
  pressed-bg: "rgba(0, 0, 0, 0.5)"
  tape-clean: "#38bdf8"
  tape-crunch: "#2563eb"
  tape-drive: "#f97316"
  tape-lead: "#ef4444"
  tape-ambient: "#06b6d4"
  tape-special: "#71717a"
typography:
  display:
    fontFamily: "-apple-system, SF Pro Text, system-ui, Segoe UI, Roboto, Helvetica Neue, sans-serif"
    fontSize: "44px"
    fontWeight: 750
    lineHeight: 1.08
    letterSpacing: "-0.02em"
  headline:
    fontFamily: "-apple-system, SF Pro Text, system-ui, Segoe UI, Roboto, Helvetica Neue, sans-serif"
    fontSize: "28px"
    fontWeight: 750
    lineHeight: 1.08
    letterSpacing: "-0.02em"
  title:
    fontFamily: "-apple-system, SF Pro Text, system-ui, Segoe UI, Roboto, Helvetica Neue, sans-serif"
    fontSize: "22px"
    fontWeight: 800
  body:
    fontFamily: "-apple-system, SF Pro Text, system-ui, Segoe UI, Roboto, Helvetica Neue, sans-serif"
    fontSize: "17px"
    fontWeight: 560
    fontFeature: "'tnum' 1"
  meta:
    fontFamily: "-apple-system, SF Pro Text, system-ui, Segoe UI, Roboto, Helvetica Neue, sans-serif"
    fontSize: "14px"
    fontWeight: 500
  label:
    fontFamily: "-apple-system, SF Pro Text, system-ui, Segoe UI, Roboto, Helvetica Neue, sans-serif"
    fontSize: "12px"
    fontWeight: 700
    letterSpacing: "0.1em"
rounded:
  chip: "4px"
  r: "6px"
  r-md: "8px"
  dot: "999px"
spacing:
  page: "14px"
  gutter: "22px"
  hit: "48px"
  bar: "56px"
  row: "64px"
  tile: "84px"
  strip: "104px"
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "#ffffff"
    rounded: "{rounded.r}"
    padding: "0 18px"
    height: "{spacing.hit}"
  button-outline:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    rounded: "{rounded.r}"
    padding: "0 18px"
    height: "{spacing.hit}"
  tab:
    backgroundColor: "transparent"
    textColor: "{colors.ink-2}"
    rounded: "{rounded.r}"
    padding: "0 18px"
    height: "{spacing.hit}"
  tab-pressed:
    backgroundColor: "{colors.pressed-bg}"
    textColor: "#fafafa"
    rounded: "{rounded.r}"
    padding: "0 18px"
    height: "{spacing.hit}"
  stack-chip:
    textColor: "{colors.ink}"
    typography: "{typography.label}"
    rounded: "{rounded.chip}"
    padding: "4px 9px"
  pane:
    backgroundColor: "{colors.sheet}"
    rounded: "{rounded.r-md}"
  row-up:
    backgroundColor: "{colors.up}"
    height: "{spacing.row}"
  input:
    backgroundColor: "{colors.field}"
    textColor: "{colors.ink}"
    rounded: "{rounded.r}"
    padding: "0 14px"
    height: "{spacing.hit}"
---

# Design System: Signal Touch

These tokens mirror `crates/signal/widgets/src/theme.rs` (greys, states, PRESSED) and `features/rigs/guitar/ui/src/perform.rs` `folder_color` (stack colours), so the Dioxus port and the prototype share one system.

## Overview

**Creative North Star: "The Rig's Own Room"**

Signal Touch is the Signal app's existing dark world, kept on purpose and polished for a hand instead of a cursor. Near-black zinc panes sit on a slightly darker desk, separated by hairlines rather than shadows. Colour is rationed to meaning: the stack colours say what kind of sound a thing is, green says what is playing, blue says what is picked, red says what is out. Everything else is grey.

Density is touch density, not DAW density: every target is at least 48px, list rows are 64px, section tiles 84px, and the chain strip is a fixed 104px band along the bottom of every page so the sound is always in view. State is drawn, not boxed: a hand-drawn strike through what is bypassed, played or cancelled, a short bar under what is up, and the current pick of a tab group pressed into the bar's surface.

**Key Characteristics:**
- Dark zinc greys from the incumbent app; flat panes with 1px hairlines.
- Stack colours appear as tinted chips with a solid swatch, never as large fills.
- Green (playing) and blue (picked) are the only state colours; red only for out/cancelled/destructive.
- The selected tab is pressed in (inset shadow), not raised or outlined.
- System sans throughout with tabular numerals.

## Colors

A grey-dominant palette with one blue action colour, one green live colour, and six stack colours used only as identity marks.

### Primary
- **Rig Blue** (primary): the one solid button per surface (the thing you came to do) and text selection. Shares its value with the crunch stack colour, as in the app.

### Secondary
- **Playing Green** (live): what sounds right now: the bar under the song up and the section playing, the engaged dot on a chain block, the live path in Routing. Its 12% wash (live-bg) backs live rows.
- **Picked Blue** (up / focus-fg): the dark blue wash (up) behind the row that is picked, and the pale blue (focus-fg) for the 2px border of a picked tile, the bar under a merely-picked row, an edited title's underline, and the focus outline.

### Tertiary
- **Stack colours** (tape-clean, tape-crunch, tape-drive, tape-lead, tape-ambient, tape-special): a sound's family. Shown as a 20% tint chip with an 8px solid swatch, or as a 10px band across the top of a section tile.
- **Out Red** (void): strikes through cancelled presets, "differs" notes, and the outline of a destructive button.

### Neutral
- **Desk** (desk): the page background behind all panes.
- **Pane** (sheet): every pane, the bar, the chain strip, side sheets.
- **Pane Recess** (sheet-2): a pane's lower working area, group headers, a lifted (dragged) row.
- **Field** (field): text input wells.
- **Hairline** (rule) and **Strong Hairline** (rule-strong): row dividers and pane edges; strong for control outlines, the bar and strip borders, module boundaries in the chain.
- **Ink** (ink), **Muted Ink** (ink-2), **Faint Ink** (ink-3): primary text, secondary text and unpicked tabs, meta and placeholders. Strike marks draw in muted ink.
- **Dim** (dim): a bypassed block's empty dot, scrollbar thumbs.

### Named Rules
**The Meaning-Only Colour Rule.** Every hue on screen means something: stack identity, playing, picked, or out. Surfaces and structure stay grey.

**The Green Plays, Blue Picks Rule.** Green marks only what is sounding; blue marks only what is selected. A picked thing that is not playing never goes green.

## Typography

**Display Font:** system sans (-apple-system / SF Pro Text, falling back to system-ui, Segoe UI, Roboto)
**Body Font:** the same
**Label Font:** the same, uppercase and tracked

**Character:** the app's native system voice, made heavy for titles (750 to 860) and tight (-0.02em), with tabular numerals everywhere so tempos, counts and keys align.

### Hierarchy
- **Display** (750, 44px, 1.08): the title of the thing in the right-hand pane (the song up, the preset).
- **Headline** (750, 28 to 32px, 1.08): left-pane titles, variation names, side-sheet titles (26px).
- **Title** (800, 22px): sub-names inside a pane, stepper values, empty-state lines.
- **Body** (560, 17px): list rows and picker items; the base is 500 at 16px. A picked item in a list goes to 820 to 840 instead of changing colour.
- **Meta** (500, 14px, faint ink): counts, block types, the second line of a row.
- **Label** (700, 12px, 0.1em, uppercase): status words on a row ("Playing", "Next", "In this variation"), stack chips (0.06em), the chain's module names (13px).

### Named Rules
**The Weight-Marks-The-Pick Rule.** In a list, the current item is set bolder; it does not get a new colour or a box.

**The Tabular Rule.** Numerals are always tabular.

## Layout

A fixed three-band shell: the bar (56px) on top with the wordmark, the four mode tabs, the profile chip, Undo and the demo status; the page; the chain strip (104px) on the bottom of every page. Pages are a grid of panes with 14px gap and 14px padding: the set or preset list on the left (minmax(320px, 38%) in Set, 34% in Sounds), the selected item on the right. Pane content is inset 22px. Pickers open in a side sheet from the right (460px, max 92%) over a 55% black scrim, never in a centred modal. Lists are ruled (hairline between rows) rather than gapped.

## Elevation & Depth

Flat. Depth is tonal (desk under pane, pane over recess) and drawn with hairlines; nothing casts a shadow. The only shadow in the system is inward: the pressed pick.

### Shadow Vocabulary
- **Pressed** (`box-shadow: inset 0 1px 2px rgba(0,0,0,0.75), inset 0 -1px 0 rgba(255,255,255,0.05)` on `rgba(0,0,0,0.5)`): the current tab in a tab group and the profile chip in the bar. Mirrors theme::PRESSED.

### Named Rules
**The Pressed-In Rule.** A selected choice among a few sinks into its surface; it is never lifted or boxed.

## Shapes

Gently rounded and small: 6px on controls, tiles and inputs; 8px on panes (side sheets square their corners against the screen edge); 4px on chips and key boxes; full round for status dots. Edges are 1px hairlines; a picked tile takes a 2px pale-blue border. Strikes are hand-drawn SVG lines that draw in once (260ms).

## Components

### Buttons
- **Shape:** gently rounded (6px), at least 48px tall, 18px side padding, 15px at 650.
- **Primary:** solid rig blue with white text, one per surface.
- **Outline:** transparent with a strong-hairline border and ink text; destructive takes the out-red border and text.
- **Press / Hover:** 5% white wash on hover (pointer devices only), 7% and a 0.985 scale on press, 140ms.
- **Disabled:** faint ink, no fill.

### Tabs
- **Style:** bare text in muted ink at 560; 48px tall in the bar (17px), 40px elsewhere (15px), optional tabular count.
- **Selected:** pressed in, near-white text at 700.

### Stack Chips
- **Style:** uppercase label on a 20% tint of the stack colour with an 8px solid swatch, 4px radius.
- **Variants:** pressed (the profile in the bar) and plain (hairline outline, no swatch).

### Panes
- **Corner Style:** 8px. **Background:** pane grey. **Border:** 1px hairline. **Shadow:** none.
- **Internal Padding:** 22px horizontal; rows divided by hairlines.

### Inputs / Fields
- **Style:** 48px, field background, strong-hairline border, 6px, 16 to 17px text, faint placeholder.
- **Focus:** 2px pale-blue outline offset 2px. An inline title edit uses a 2px pale-blue underline instead of a box.

### Navigation
- **Bar:** pane-grey band, strong hairline under it, wordmark "signal" at 24px/750, a hairline divider, the mode tabs.

### Chain Strip
One horizontally scrolled band of blocks in signal order, each at least 138px wide. Module names sit above in faint label type; a stronger hairline marks a new module. Each block shows its name (15px/780) and what it plays (meta), with a 40px bypass switch holding a 14px dot: solid green when in, an empty dim ring when out. A bypassed block is struck and dimmed to 55%, never hidden.

### Section Tiles
84px tiles with a 10px stack-colour band across the top, a 1px strong hairline (2px pale blue when picked), and a green bar under the one playing.

## Do's and Don'ts

### Do:
- **Do** take colours from the tokens only; they are the same values the Dioxus app reads from theme.rs and folder_color.
- **Do** keep every touch target at least 48px (the hit token).
- **Do** mark the current pick of a tab group pressed in (theme::PRESSED), never with a border or a raised fill.
- **Do** strike what is out, played or cancelled instead of hiding it.
- **Do** keep the chain strip visible on every page.

### Don't:
- **Don't** use green for anything that is not sounding, or blue for anything that is not picked.
- **Don't** fill large areas with stack colours; they are chips, swatches and tile bands.
- **Don't** add drop shadows to panes or sheets; depth is tonal and hairline.
- **Don't** open pickers as centred modals; use the right-hand side sheet.
