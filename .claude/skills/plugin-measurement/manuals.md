# Manuals as measurement context

Measure first, then read the manual — but always read it. The manual is the
manufacturer's own claim about the unit, and it is the cheapest way to find
out that a measurement missed something or measured the wrong control.

Two things it reliably catches:

- **Controls that are not what they look like.** An 1176's Attack and
  Release knobs run *backwards* — 7 is fastest, 1 is slowest — so a sweep
  labelled "attack rising" is measuring the opposite of what its name says.
  The all-buttons-in mode is not on the ratio control at all.
- **Values worth checking a measured curve against.** Published attack and
  release times, ratio steps, and the Fairchild's six time-constant
  positions are all numbers a capture can be validated against. Where the
  measurement and the manual disagree, one of them is interesting; where
  they agree, the units are confirmed.

## Where they go

Alongside the measurements, not in the repo:

```
/run/media/AudioHaven/Plugin Analysis/<plugin>/manual/
    manual.pdf          as published
    manual.txt          extracted text, for grepping and for context
    notes.md            what it says that bears on the model
```

`features/fx/eq/spec/` already follows this shape for the FabFilter units
(`proq4-manual.pdf` beside `proq4-manual.txt` and `manual-notes.md`) — keep
the extracted text next to the PDF, because a PDF is not greppable and not
usable as context.

Extract with `pdftotext -layout manual.pdf manual.txt`; `-layout` keeps the
specification tables readable, which is the part that matters.

## What to pull out into `notes.md`

Only what bears on modelling — not a summary of the manual:

- every control, its range, and its units as the manufacturer states them
- published time constants, ratios, and any stated program-dependence
- signal path order (where the saturation sits relative to the gain
  element) — this is what says whether a measured decomposition is
  physically plausible
- anything the manual says is *modelled* versus *emulated behaviour*, which
  tells you what the plugin is even trying to do

Cite the page. A number in `notes.md` with no source is one nobody can
check later, which is the same standard the measured constants are held to.
