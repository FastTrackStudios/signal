# Knob interaction

How a knob (and any drag-valued control — slider, XY pad, envelope node) behaves
under the pointer. This is a UI contract, binding on every knob in every signal
GUI (synth, keys, sampler, guitar rig, FX). A knob edits one
[Parameter](parameter.md); this spec covers only the *interaction*, not the value
model. Reference the drag pattern in `features/rigs/synth/ui` (`MiniKnob`).

## Drag capture (the core rule)

r[signal.knob.capture]
On pointer-down a knob **captures the pointer** and stays the active control
until pointer-up, regardless of where the cursor moves. It MUST keep receiving
motion when the cursor leaves the knob's bounds — moving fast, or dragging far
above/below the knob, must not drop the drag. (Concretely: capture the pointer
(`setPointerCapture`) or bind move/up listeners on the window/document, never on
the knob element — an element-scoped `mousemove` stops firing the instant the
cursor exits, which is the current broken behavior.)

r[signal.knob.release]
The drag ends only on pointer-up (or pointer-cancel). At that point the knob
releases capture and returns to idle. A pointer-up anywhere on screen ends the
drag; the value at release is the committed value.

r[signal.knob.leave-noop]
Pointer-leave / mouse-out MUST NOT end a drag or stop value updates. A knob only
stops tracking on release. (Hover styling MAY change on leave; the *drag* does
not.)

## Direction & mapping

r[signal.knob.vertical]
Dragging **up increases** the value, dragging **down decreases** it — vertical
motion only. Horizontal motion is ignored by default (a knob is not a horizontal
slider). Value change is proportional to accumulated vertical delta since
pointer-down, not to the cursor's absolute position over the knob.

r[signal.knob.sensitivity]
A full-range sweep takes a fixed drag distance (a sensitivity constant, e.g.
~200 px for 0→1). Holding a **fine** modifier (Shift) scales the delta down for
precise adjustment. Sensitivity is constant across the range; the parameter's
[response curve](parameter.md) (`signal.parameter.response`) shapes value, not the
drag.

## Shortcuts

r[signal.knob.wheel]
The scroll wheel over a knob nudges its value by a small step (fine step with the
modifier). Wheel adjustment does not require a prior click.

r[signal.knob.reset]
Double-click (or a modifier-click) resets the knob to the parameter's **default**
value.

r[signal.knob.keyboard]
A focused knob responds to arrow keys (up/right = increase, down/left = decrease),
Page Up/Down for coarse steps, and Home/End for min/max, for accessibility.

## Value & modulation display

r[signal.knob.value-display]
While dragging, the knob shows its live value (a readout/tooltip). Editing the
base value is a distinct gesture from editing a modulation route's depth
(`signal.parameter.mod-display`).

r[signal.knob.mod-ring]
A knob renders its modulation state: the base position, the live modulated
position, and an arc/ring showing each route's `mod_min..mod_max` extent. Dragging
the ring edits modulation depth; dragging the knob body edits the base value.
