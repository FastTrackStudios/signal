# Pro-R 2 reference — Decay Rate EQ + Post EQ

Extracted from `ProR2_Manual.pdf` / `pror2-manual.txt` (FabFilter, behavior
reference only). What FTS Reverb borrows and where ours differs is specified
in `docs/spec/fx/reverb-eq.md` (`fx.reverb.*`).

## Decay Rate EQ (manual p.9–11)

- Replaces the classic low/high crossover decay system: a free **6-band EQ
  over decay time** — Bell, Low/High Shelf, Notch shapes — drawn as the
  blue curve in the same display as the Post EQ.
- Vertical axis = decay-time multiplier per frequency. The global Decay
  Rate knob spans **25 %–400 %** of the space's natural decay; the curve
  shapes that per frequency (waterfall picture: lows 200 %, highs 50 % is
  the classic crossover sound).
- Band creation: click the blue curve and drag; dragging at the far
  left/right creates a shelf instead of a bell; double-click / Ctrl-click
  in the *bottom* of the display creates a notch (kill that frequency's
  tail).
- The blue scale at the display's left lights up while a decay band is
  touched, to separate it from the yellow Post EQ scale at the right.
- Gestures are the Pro-Q set: wheel = Q, Ctrl+drag vertical = Q, Shift =
  fine, Alt-click dot = bypass band, Ctrl+Alt-click = change shape,
  double-click dot = text entry (Tab steps Frequency → Decay Rate → Q,
  freq accepts `2k`, `A4`, `C#2+13`), right-click = band menu.

## Post EQ (manual p.12–13)

- 6 bands over the **final reverb sound** (wet only): Bell, Low/High
  Shelf, Low/High Cut up to 96 dB/oct.
- **The reverb gain is automatically adjusted to compensate for EQ
  changes** — tweaking the Post EQ does not require riding the Mix/send
  (this is `fx.gain-comp.eq` applied to the wet path).
- Yellow curve/scale at the right side; display range ±30/18/9 dB via a
  button at the top of the Post EQ scale.
- Same creation/selection/editing gestures as the Decay Rate EQ; a
  rubber-band selection takes both kinds of bands.

## Both

- Up to 6 bands each; selections may span both curves; per-band bypass;
  per-band speaker assignment in surround (out of scope for us today).
- The two curves live in ONE display with two scales (decay left/blue,
  gain right/yellow), over the realtime spectrum analyzer.
