---
title: The rigs
order: 2
summary: Guitar, keys, drums — and what each one actually does
---

# The rigs

A *rig* is a complete instrument: its sound sources, its effects chain, its
routing, and the control surface that drives it live.

## Guitar

Neural amp models and impulse responses, with a pedalboard in front of them.
The whole board is switchable on a footswitch without a click, because the
switch happens in the graph rather than by rebuilding it.

This is the rig with the least to download — the DSP *is* the product, and
the only thing streamed is the amp model itself.

## Keys

Multi-mic sampled instruments with velocity layers, round-robins and real
legato transitions. Layers and splits behave like a mixer: move a fader and
the balance changes live, rather than editing a patch and reloading it.

## Drums

One kit, every microphone, one fader each. Trigger detection runs per pad,
round-robins go deep enough that repeated hits do not sound like a loop, and
the bleed between microphones is kept rather than gated away — because the
bleed is most of what makes a kit sound like a room.

## Vocal

Not built yet. The tracking chain, tuning and a harmony stack, driven from
the same engine.
