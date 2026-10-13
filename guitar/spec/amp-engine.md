+++
title = "Live amp engine"
description = "The single-amp live handle — opens duplex audio and fronts one NAM amp on the shared sampler core."
weight = 10
+++

# Live Amp Engine

`AmpEngine` is a thin live-handle over the shared sampler rig: it opens the
native duplex audio and runs exactly one NAM amp as the active patch, exposing
just what an amp front-end needs.

r[guitar.amp.clean-passthrough]
`AmpEngine::open` MUST open the live duplex audio with no model loaded — the
signal passes through clean until `load_model` installs one.

r[guitar.amp.full-path]
`load_model` MUST carry the full model path through to the installed chain
(not a derived block id), so model files with spaces or quotes in their path
load correctly.

r[guitar.amp.single-patch]
Loading a model MUST replace any current amp with a fresh one-patch profile
whose only block is that NAM amp — the amp is the product, not a profile tree.

r[guitar.amp.live-meters]
The engine MUST expose live input and output peak levels and the per-block DSP
render time, so a front-end can meter the signal without touching the audio
thread.

r[guitar.amp.chain-blocks]
The engine MUST expose the active patch's chain as blocks (with names filled
from the model when empty) so a signal-flow grid can render the live amp.
