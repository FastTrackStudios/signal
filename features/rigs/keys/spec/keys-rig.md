+++
title = "Keys rig definition"
description = "The Nord-style keys live rig over the shared engine."
weight = 10
+++

# Keys Rig

`KeysRig` plays a Nord-style composition tree (splits, layers, velocity
crossfades) from a central MIDI input, on the shared sampler engine.

r[keys.rig.composition-tree]
`KeysRig` MUST drive a composition tree whose zones map keyboard splits +
velocity layers onto engine instruments, so one keyboard plays a multi-part
patch.

r[keys.rig.output-only]
The keys rig MUST run as an output-only engine (MIDI in → audio out); it needs
no audio input, unlike the guitar rig's duplex path.
