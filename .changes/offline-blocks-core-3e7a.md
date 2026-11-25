---
"signal-core" = "minor"
---

Add offline blocks: `level` (the vocal leveler: gate, de-breath, ride and de-ess lanes, analysed once over the whole vocal, played by a cheap apply stage) and `align` (delay either way, polarity, phase rotation), each block's analysis kept in its spec with its state (none, fresh, stale, running), and its lanes played by the host's automation or by Signal itself against the transport (signal)
