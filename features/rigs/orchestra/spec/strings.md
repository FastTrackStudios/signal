+++
title = "Orchestral strings definition"
description = "How the strings feature loads and drives a CSS library on the shared engine."
weight = 10
+++

# Orchestral Strings

The strings feature is a definition + test layer; the sampler engine implements
the articulation behaviour. These rules cover the strings *definition*.

r[orchestra.load.css-match]
`load_strings` MUST wire the engine settings that reproduce a real CSS-in-Kontakt
render — solo the requested mic, apply the arco-attack sustain bloom, and set the
note-off release overlap.

r[orchestra.load.section-zones]
`load_strings` MUST resolve the section's `library.styx` zone map under the
library root so a section name (e.g. "1st Violins") is all a caller needs.
