+++
title = "Live amp engine + NAM backend"
description = "Requirements for the standalone neural-amp modeler path — the NAM DSP backend and the single-amp live engine that fronts it."
weight = 10
+++

# Live Amp Engine + NAM Backend

The neural-amp modeler path turns a live guitar input into a modeled amp tone.
It has two layers: the `NamProcessor` DSP backend (one loaded `.nam` model) and
the `AmpEngine` handle that opens live duplex audio and runs exactly one amp.

## NAM DSP backend

r[sampler.nam.mono-fold]
NAM models are mono in / mono out. The FX chain processes interleaved-stereo
`f32`, so the backend MUST collapse input to mono by summing L+R (halved so a
centered signal lands at unity), run the model once, and broadcast the mono
output back to both channels.

r[sampler.nam.no-hot-alloc]
The per-block process path MUST NOT allocate under normal operation. Mono
scratch buffers MUST be pre-sized at `reset(sample_rate, max_block)` time; a
larger-than-expected block MAY resize once but the steady state MUST be
allocation-free.

r[sampler.nam.expected-rate]
The backend MUST expose the model's declared training sample rate (when the
`.nam` file provides one) so the rig can detect a host-rate mismatch that would
shift the model's voicing.

r[sampler.nam.planar-interleaved-parity]
The planar `process_block` path (daw plugin instance) and the interleaved
`process_interleaved` path MUST produce identical output for identical input —
the rig hears the same tone regardless of which host drives it.

r[sampler.nam.prepared-flag]
A freshly loaded model MUST report `is_prepared() == true`; `deactivate()` MUST
clear the flag and `prepare()` MUST restore it, so the plugin host can gate
processing on readiness.

## Single-amp live engine

r[sampler.amp.clean-passthrough]
`AmpEngine::open` MUST open the live duplex audio with no model loaded — the
signal passes through clean until `load_model` installs one.

r[sampler.amp.full-path]
`load_model` MUST carry the full model path through to the installed chain
(not a derived block id), so model files with spaces or quotes in their path
load correctly.

r[sampler.amp.single-patch]
Loading a model MUST replace any current amp with a fresh one-patch profile
whose only block is that NAM amp — the amp is the product, not a profile tree.

r[sampler.amp.live-meters]
The engine MUST expose live input and output peak levels and the per-block DSP
render time, so a front-end can meter the signal without touching the audio
thread.

r[sampler.amp.chain-blocks]
The engine MUST expose the active patch's chain as blocks (with names filled
from the model when empty) so a signal-flow grid can render the live amp.
