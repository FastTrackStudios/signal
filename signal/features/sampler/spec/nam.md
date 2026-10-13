+++
title = "NAM DSP backend"
description = "The neural-amp-modeler FX backend — one loaded .nam model, mono-folded and metered, used by the sampler mixer and the guitar amp."
weight = 10
+++

# NAM DSP Backend

`NamProcessor` wraps one loaded `.nam` model as a mixer FX backend. It's part of
the shared sampler core (the mixer's `FxBackend::Nam`); the guitar feature runs
it as an amp on top.

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
