---
title: FAQ
kind: concept
type: concept
---

# FAQ

Short answers to the things that trip people up, each pointing at the page with the full story. (More will land here as they come up.)

## Monitoring & latency

**I can hear myself, but there's a delay / latency.**
Your buffer size is too high for live monitoring. Drop it to 64–128 samples while tracking, or switch to your interface's direct hardware monitoring and turn REAPER's input monitoring off. Full explanation in [[audio-setup|Audio Setup]].

**I can't hear my input at all while recording.**
Input monitoring is off, or the track isn't armed. Arm the track (`kbd:@9`) and toggle monitoring with `kbd:@_FTS_SESSION_MONITOR_TOGGLE_ON_OFF`. If there's still nothing, check the device and input in [[audio-setup|Audio Setup]] and see [[Recording]].

**Playback crackles or drops out on a big mix.**
Opposite problem — buffer too *small* for the plugin load. Raise it to 512–1024 while mixing. See [[audio-setup|Audio Setup]].

## Recording

**My take-ranking number keys do nothing.**
Take ranking is a Record-mode layer, not a base binding. Activate Record mode first (`kbd:@_FTS_SESSION_MODE_RECORD`), then the number row ranks takes. See [[modes|Modes]] and [[Recording]].

**The player rushes the first bar.**
Give them a lead-in: turn on pre-roll (`kbd:@41819`) or drop a count-in region. See [[Recording]].

## Input layer

**I pressed a prefix and no menu appeared.**
The which-key overlay is part of the FTS input layer — make sure the extension is installed and the status panel shows a profile is active. See [[installation-setup|Installation & Setup]] and [[Input System|the input layer]].

**A key does something different than the guide says.**
You're probably in a mode that re-layers it. Check the active mode in the status panel; leave the mode to fall back to the base profile. See [[modes|Modes]].

Still stuck? The full searchable binding reference is at /input.
