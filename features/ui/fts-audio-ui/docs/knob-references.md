# The knob kit: what the parts are, and what we have not built yet

Every style in `hardware/knob_parts.rs` is a real part somebody moulded. This
is the shelf it came off — what the archetype is, what makes it recognisable
before you read a word on the panel, and which of them the kit can draw.

Look at what exists before adding to it:

```sh
cargo test -p fts-audio-ui --test knob_sheet   # the kit, plain and tinted
cargo test -p kit-sheet                        # every knob on the panel it sits on
```

## In the kit

| style | the part | the tell |
|---|---|---|
| `Bakelite` | LA-2A / 1176 body knob | matte black truncated cone, painted line |
| `Metal` | turned aluminium, dark index | concentric turning marks, two bright lobes |
| `Skirted` | Davies 1910/1913 phenolic | wide skirt, ribbed body standing on it |
| `Daka` | Daka-Ware, Pultec EQP-1A | scalloped skirt, ridged body, engraved line |
| `Marconi` | Neve 1073 gain / HPF | a coloured wing laid *across* the knob, overhanging |
| `Collet` | SSL 4000 channel | flat coloured cap, fluted rim, one bar across |
| `SilverTop` | UREI 1176 in/out | dark annulus around a bright knurled disc |
| `MetalFluted` | dbx 160 and its generation | brushed rim flutes, dark centre cap |
| `Pointer` | Davies 1900H, LA-2A | plain black body with a moulded *nose* |
| `Neve` | 1073 concentric EQ | light collar, **geared** dark cap — teeth inside |
| `SoftPointer` | Rogan PT — Make Noise, Mutable | soft-touch rubber, pointer *widening* to the rim |
| `ChickenHead` | tweed Fender, Ampeg, EMS VCS3 | the pointer IS the knob: a beak on a small hub |
| `Dial` | Empirical Labs Distressor | numerals printed on the skirt, turning with it |

The kit's own vocabulary — skirt, collet, flutes, knurl, index, hub — is the
trade's, not ours: a skirt flares at the base and may carry the scale; a
collet clamps a tapered stud instead of a set screw; knurling is cut on a
lathe, which is why it goes *round* and not out.

## Not in the kit

Roughly in order of how much they would add — a new silhouette is worth far
more than a new colourway, because silhouette is what you recognise across a
room and what a screenshot has to carry at 44 px.

**Top-hat / witch-hat.** Fender amp and API 550-family: a cylinder flaring to
a numbered skirt, often with a coloured insert in the cap. Close to `Skirted`
in anatomy, so the value is the coloured cap and the printed numbers, not the
outline.

**Bar / T-handle.** EMI REDD, Chandler: a bar handle spanning the panel, no
disc at all. `Marconi`'s wing is the near neighbour; this one has no body.

**Vernier with a window.** A precision dial reading through a slot rather than
against a printed ring — the Fairchild's time constant, tuning gear. Would
need a new `Index` variant and a window cut in a tier.

**Concentric collet pair.** SSL and API stack two collets on one bushing. The
kit's only concentric knob is `Neve`; the mechanism generalises, but nothing
else declares it.

**Rubber-over-metal encoder.** The modern controller knob: a soft grip ring
with an LED collar for position. No printed scale at all — the light *is* the
readout, which is a different contract from everything above.

## The other idiom

`controls::knob::Knob` is not hardware. It is the flat dial with a value arc,
and its references are plugins rather than parts:

- **FabFilter Pro-Q** — a thin arc on a dark ring, the value as a numeral
  under it, a fine ring appearing on hover. The band's colour is the only
  chrome.
- **oeksound soothe** — no cap at all: a ring, a mark, and a large numeral.
  Proof that the dial can be almost nothing.
- **Valhalla** — flat coloured disc, single line, no gradient anywhere.
- **u-he Diva** — the opposite pole: fully rendered metal, closer to our
  hardware faces than to this one.
- **Kilohearts** — minimal ring, no body, everything carried by the arc.

Where the hardware kit is judged against a photograph, this one is judged
against a *readout*: what it has to get right is that the number under the
dial and the label under that stay legible at 44 px and do not shift when the
value changes width. `kit-sheet`'s `knob_readouts` test is that check.
