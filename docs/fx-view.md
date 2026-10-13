# The FX view

Status: design, 2026-10-03. Not built yet.

A view for editing a track's effects, in the shape of Ableton's and Bitwig's
device chains, but built on the mixer.

## Layout

The FX view has the mixer's layout with a different job:

- **Left:** the selected track's lineage as mixer strips. Its parent folders go
  out to the left, up to the top-level folder, and the selected track's own
  strip sits rightmost of them.
- **Right:** from the selected track's strip onwards, everything to the right is
  that track's effects as embedded editors (EQ, compression, the signal chain).
  These are the same processors the mixer's Tone rack shows, laid out
  horizontally as devices rather than stacked over a strip.
- **Scrolling:** the device row scrolls horizontally.
- **Collapsing:** devices collapse, as in Ableton and Bitwig. A collapsed device
  is a narrow column showing its name and bypass, and the rest of the chain
  slides over.

## Where it shows

- **Desktop:** docked under the arrangement, like the docked mixer, and as a
  view of its own.
- **iPad:** full screen.
- **iPhone:** about the docked version's height, scrolling horizontally.

## Devices are frames

Every device's editor is a runtime frame:

- **Saved frames:** an effect that has a saved frame (its own `.frame`
  document) loads that, so a device's face is a theme file. It can be swapped,
  edited live and hot-reloaded, the same as the mixer strip, the track panel
  and the rack.
- **Fallback:** an effect without one gets the rack's current panel.

This rests on what the mixer work already built:

- **Slots:** a device's curve or spectrum goes in a `%slot` that the app draws
  into.
- **Layers:** a theme can override one device's frame without touching the
  rest.
- **Components:** knobs, buttons and so on are defined once and reused across
  devices.

## Later: signal

Full signal integration comes later. A track whose instrument is a signal rig
(guitar rigs, keys rigs) shows its signal chain right in the device row, with
the rig's own blocks as devices.
