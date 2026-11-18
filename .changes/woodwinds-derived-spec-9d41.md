---
"sampler" = "minor"
"signal-orchestra" = "minor"
---

Cinematic Studio Woodwinds play on the Kontakt-matched strings engine model: their spec is derived from the strings spec (articulations renamed to the woodwinds' slots), each instrument gets a generated section.styx from its decoded KSP values, and short notes can draw random round robins that avoid the key's last N (`rr_random_avoid_last`, with `--rrs` to replay a reference's draws) (signal)
