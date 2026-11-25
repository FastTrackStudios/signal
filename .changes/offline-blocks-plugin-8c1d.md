---
"signal-plugin" = "minor"
---

Add offline blocks to the Signal plugin: `SIGNAL_ANALYZE` reads the audio arriving at a `level` or `align` block from a file the host renders offline (or captures it while the song plays through once), analyses on a worker, and writes the lanes as the host's own automation of the FX's parameters through host-automation/1 in one undo step, keeping the user's redrawn points, or plays them itself where the host has no such extension; `SIGNAL_ANALYSIS`, `SIGNAL_ANALYSIS_CLEAR`, `SIGNAL_ANALYSIS_STALE`; each block's `mode` and `analysis` in signal.chain/1 (signal)
