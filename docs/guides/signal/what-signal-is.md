---
title: What Signal is
order: 1
summary: One engine, many remotes
---

# What Signal is

Signal is a rig that runs headless. The engine holds the audio graph — amps,
sample libraries, effects, routing — and every interface you touch is a
*remote* onto that engine rather than the thing itself.

That split is the whole architecture, and it is strict:

- The rig core has no window. It runs as `signal-desktop --engine` and serves
  a vox router on `ws://:4040/vox`.
- Desktop, tablet, browser and the plugin editor are all clients of that
  router. They render the same state because they are looking at the same
  running rig.
- Closing a window does not stop a note. The engine keeps playing.

## Why it is built that way

A live rig cannot afford to have its audio thread coupled to its user
interface. If drawing a meter can stall the render callback, you get a
dropout on stage — and the fix is not "draw fewer meters", it is to make the
two things unable to touch each other.

Running the interface as a network client makes that separation impossible to
violate by accident. It also gets you the tablet on your mic stand for free:
it is the same client, on a different screen.
