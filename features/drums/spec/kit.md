+++
title = "Drum kit definition"
description = "How the drums feature loads a kit onto the shared engine."
weight = 10
+++

# Drum Kits

Drums are plain sample zones on the shared sampler engine — no drum-specific DSP.

r[drums.kit.gm-channel]
`load_kit` MUST route the loaded kit to the General-MIDI percussion channel so a
standard drum sequence triggers the right zones.

r[drums.kit.sample-zones]
A kit MUST be realized as ordinary engine zones (round-robins + velocity layers)
through the shared sampler loader — no drum-only signal path.
