# Automation: lanes under the tracks, moves made by section

A mix is finished by moving things through the song: the kick's top end
up where the band is loud and nobody sings, a vocal's reverb up in a
sparse verse, an instrument pulled back under a line. Session shows a
track's automation the way REAPER does, an envelope to a lane under the
track, and makes the section-by-section moves from what is in each
section.

## Why this shape

- **The lane is the envelope's own.** A lane is shown when its envelope
  is visible, and its height is the envelope's lane height. Both are the
  song's, saved with it, and REAPER reads them the same way. The window
  keeps no list of its own that could disagree.
- **What is drawn is what plays.** The engine and the window evaluate an
  envelope with one definition: each segment bent by its point's shape,
  automation items played over their windows. A curve drawn
  straight that plays as an S would be a curve that lies.
- **Sections are picked by what is in them.** "Loud" and "sung" are read
  from the recordings: the peaks of everything playing at its fader, and
  how much of the section the vocal tracks are above their own floor.
  On Unbreakable the two solos and the first breakdown come out loud
  instrumentals, the intro and first verses quiet, everything else loud
  and sung. A name (`CH`) picks every section of a kind, so a move is
  made once for all the choruses.

## Rules

r[automation.lanes]
A track's visible envelopes are drawn in lanes of their own under it,
each as its curve plays (shapes and automation items included), with
its points, its items' windows and curves, and a header with its name,
its value under the cursor, its mode and a button that hides it. In a
lane a press on the ground adds a point and drags it, a press on a point
drags it, Alt and a press deletes it, an item's name bar moves the
item, and the lane's bottom edge resizes it. Each gesture is one undo
step. The track panel's automation button lists what the track can
automate (its volume, pan, width and mute, each plugin's parameters, and
a leveled vocal's leveler lanes) and shows or hides each.

r[automation.write]
A fader or pan knob moved while the song plays writes its automation,
as the envelope's mode — or, for an envelope that has none of its own
yet, its track's — says: Touch while it is held, Latch from the first
touch until the song stops, Write always. A pass replaces the points it
rides over. In Read and the writing modes the envelope is the volume
(or pan) itself; in Trim/Read the fader trims it.

r[automation.items]
Automation items are made by hand (Shift and a drag in a lane, over the
span, their curve what played there), resized by their edges, moved by
their name bar, copied into their pool with Ctrl (onto another track's
lane too) and deleted with Alt. A pool is the project's: every instance
of it, on any envelope, plays one curve, and an edit to one is an edit
to all. An item's take volume curve can be shown on it and its points
dragged there.

r[automation.by-section]
A move by section sets an envelope section by section. A section is
picked by its name, its kind (its name up to its number), or by what is
in it: `loud` (above halfway between the song's quietest and loudest
sections), `quiet`, `vocal` (sung through a quarter of it or more),
`instrumental`, or several of those words together. The first pick that
matches wins, and the rest of the song keeps a value of its own. Each
change ramps in over a beat (by default) ending as its section starts.
Pooled, each picked kind of section becomes one pooled automation item,
so changing it in one chorus changes it in all. The envelope is shown in
its lane, and the move is one undo step.
