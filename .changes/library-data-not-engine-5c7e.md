---
"sampler" = "minor"
"signal-orchestra" = "minor"
---

The sampler engine holds no library numbers: note-off/declick timings, CC lags, the con sordino filter and the transition shape (swell, low-latency fade, start skip, portamento, AB dip, old-body fade) are spec fields, and CC58 values are keyswitch tokens ("Nonvib+@legato-low", "Marcato+@overlay-on") instead of labels the engine matched. Orchestra loads any Cinematic Studio library (`LIBRARIES`, `load_instrument`, `render_css_live --library`); the null suite runs woodwinds (signal)
