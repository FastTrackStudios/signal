# EQ display — range, zoom, and band editing

The EQ graph (`features/fx/eq/eq-ui`) is the primary editing surface for the
parametric model and the "what is actually happening" view for every other
model and for a stack (`fx.stack.*`). This spec covers the display's scale
and the band gestures. Band-gesture rules defer to [controls.md](controls.md)
for modifier meanings.

## Vertical (dB) range

r[fx.eq.display.range]
The graph's gain axis is symmetric about 0 dB with a selectable range of
±3, ±6, ±12, ±18, ±24, ±30 dB, persisted as a plugin parameter (`db_range`)
so it survives a session reload and can be a per-preset choice. The
selector is always reachable from the graph surface itself (a compact
control at the top of the dB scale), not only from parked chrome. The grid
step follows the range (2 dB ≤ 6, 3 dB ≤ 12, 6 dB above).

r[fx.eq.display.auto-range]
When a band's gain (or the summed response, or a stacked stage's response)
is dragged **outside** the current range, the range expands to the next step
automatically, so the curve is never clipped by the display while editing.
Expansion is immediate; contraction is never automatic (the user chose the
range; a shrinking graph under a drag is disorienting). Auto-range is a
per-instance toggle (`auto_range`, default on) and double-click on the dB
scale returns to the default range for the model (±3 dB for the
parametric baseline, ±12 dB for hardware faces with broad curves).

r[fx.eq.display.defaults-agree]
There is ONE source of truth for the default range: the `db_range`
parameter default. The graph component, the SVG fallback painter and the
model carry no independent default that disagrees with it.

## Horizontal (frequency) zoom

r[fx.eq.display.freq-zoom]
Click-dragging vertically on the frequency scale zooms in/out about the
frequency under the cursor; dragging horizontally while zoomed pans;
double-click on the scale returns to the full 10 Hz–30 kHz range. Zoom is
view state (not a parameter) and resets with the editor.

## Band gestures

r[fx.eq.display.band-add]
Double-click on empty graph space adds a band at the pointer, with the
shape inferred from the position (notch near the floor, cut at the
extremes, shelf at the edges, bell elsewhere) and immediately enters a
drag. Right-click on empty space offers "Add <shape>". A band can always
be added even when the graph is showing a stack's total response — it is
added to the focused parametric stage.

r[fx.eq.display.band-drag]
Dragging a band dot edits frequency (x) and gain (y), gain clamped by
shape. Fine modifier (Ctrl/Cmd or Shift, per `fx.control.fine`) reduces the
drag ratio. Multi-select (Shift-click dots, rubber-band on empty space)
drags proportionally. Dragging a dot out of the graph and releasing deletes
it.

r[fx.eq.display.band-wheel]
Wheel over a band adjusts Q (×1.15 per notch); Ctrl/Cmd+wheel adjusts
slope on cut bands; **Alt+wheel adjusts dynamic range** on a dynamic band
(Pro-Q convention, currently unimplemented); fine modifier halves the step.

r[fx.eq.display.band-reset]
Double-click on a band dot resets its gain to 0 dB (its default) but keeps
frequency, Q and shape; Alt-click toggles the band's bypass.

r[fx.eq.display.band-text]
Double-click on a band's readout (in the band popup or the panel) opens text
entry for that field, with the parsing conventions of
`fx.control.text-entry.parse` (so `1k`, `A4`, `2x`, `50%` all work). Tab
steps Frequency → Gain → Q.

## What the curve shows

r[fx.eq.display.curves]
The graph always draws: the parametric sum (solid), each band's own curve
on hover/selection (faint), the hardware model's computed response when a
hardware model is active (dashed), and — when the instance is a stack —
the **total** response of the stack in the accent colour plus each stage's
contribution in a muted per-stage colour (see `fx.stack.visualize`).

r[fx.eq.display.scale-is-not-zoom]
`gain_scale` (the curve-amount macro, −100..200%) multiplies band gains in
DSP and in the drawn curve; it is never used as a view zoom, and the view
range is never used as a gain.
