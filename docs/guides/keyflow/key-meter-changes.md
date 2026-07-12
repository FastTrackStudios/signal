---
title: Key & Meter Changes
kind: concept
type: concept
order: 8
---

# Key & Meter Changes

The [[structure|header]] sets the song's starting key and time signature.
When the music moves to a new key or meter partway through, you mark the change
right where it happens on a chord line — the same tokens you used in the header,
dropped inline.

## Changing key

Put a key token — the same `#Key` from the header — on the chord line at the
point the key changes:

```kf-src
4/4 #C

VS 8
1  4  5  1   #G   1  4  5  1
```

A key change mid-chart (to G at bar 3), engraved:

```kf+
4/4 #C
1 | 5 | #G 1 | 5
```

From `#G` onward the chart is in G, so the `1 4 5` after it are G–C–D, not
C–F–G. The token isn't a chord; it just moves the key — and the key signature —
from that spot on. It works in a letter-name chart too: the change still updates
the key signature, even though letter chords don't lean on the key.

A whole section can also *start* in a new key by putting the token on its header
(`BR 8 #Ab`) — see [[sections|Sections]].

## Changing time signature

A meter change is written with a **`T`** in front of the new signature:

```kf-src
VS
G  D/F#  Em  C   T6/8 Am   T4/4   G  D
```

`T6/8` switches to 6/8 from that point; `T4/4` switches back. A change holds until
the next `T`.

The `T` is required. A bare `6/8` on a chord line would read as a *chord* — a 6
over an 8 — so the `T` tells Keyflow "this is the meter." (It's the meter's
version of the `#` that marks a key.)

## One bar, then back: `!T`

Often a meter only wobbles for a single bar — one bar of 2/4 in a stream of 4/4.
Prefix the change with `!` and it lasts exactly **one measure**, then snaps back
to the prevailing meter on its own, with no closing `T` needed:

```kf-src
VS
G  D/F#  Em  G   !T2/4 Am   G  D
```

`Am` is that one bar of 2/4; the `G D` after it are already back in 4/4. The `!`
is the same "just this once" mark you can put on a
[[rhythm|chord duration]] — here it scopes the meter change to a single
measure instead of letting it stick.

## What's next

- **Annotations & Expression** — the markings that sit on top of the notes:
  staff text, instrument cues, dynamics, and hairpins.

---

Previous: [[lyrics|Lyrics]] · Next: [[annotations|Annotations & Expression]] · Up: [[keyflow|Keyflow Guide]]
