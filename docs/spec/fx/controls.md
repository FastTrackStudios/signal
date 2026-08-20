# Audio control interaction

How every parameter-editing control in an FTS plugin or rig GUI behaves under
the pointer and keyboard: knobs (flat and hardware), sliders, range knobs, XY
pads, levers, EQ band dots, and any future drag-valued widget. This is the
**binding** contract for production plugins; [signal.knob](../signal/knob.md)
is the older, thinner synth-era version and defers to this document where
the two differ (fine modifier, reset gesture).

The conventions below are the intersection of what working engineers expect
from REAPER, FabFilter, iZotope, Logic and Pro Tools: a plugin that behaves
differently from all of them costs the user a mis-drag on every first use.
Reference implementation: `libs/ui/fts-audio-ui` (`drag.rs`, `gesture.rs`,
`param.rs`, `controls/`, `hardware/`).

## One gesture layer

r[fx.control.shared-gesture]
All pointer and keyboard gestures on a value control go through ONE shared
gesture layer in `fts-audio-ui` (drag capture, modifier handling, reset,
wheel, text entry, keyboard). A control widget declares *what* it edits (a
`ParamHandle`, an axis, a sensitivity) and never re-implements *how* a
gesture maps to a value. Two widgets that disagree on a modifier or a
reset gesture are a defect (`primitives.drift-is-a-bug`), fixed by routing
both through the layer, never by hand-syncing the copies.

r[fx.control.capture]
On pointer-down a control captures the drag until pointer-up anywhere on
screen. Leaving the widget's bounds, the window, or the plugin editor does
not end the gesture; pointer-up/cancel does. The gesture is bracketed by
`begin_edit` / `end_edit` on the handle so the host records one undo step
and one automation gesture.

## Rendering

r[fx.control.painted]
A control's graphic (dial, arc, pointer, faceplate art, meter face, graph)
is painted as an `anyrender::Scene` through a Blitz custom widget
(`<object data=widget>` + `SceneSlot`, the pattern of the expression
editor's roll and the EQ graph painter), **never as an inline `<svg>`
subtree**. Blitz paints inline svg as a replaced element with a hardcoded
`object-fit: contain`, so the drawing is rescaled by box/declared-size and
the pointer mapping with it, and every value change re-parses markup into a
usvg tree. The scene is built in render (where signals are readable) and
left in a slot the widget replays; the painter itself is portable
(`fts_audio_ui::paint`, no DOM, compiles on wasm). Text (readouts, labels)
stays DOM. The gesture overlay covers exactly the painted box, so readouts
and labels beside it stay clickable.

## Drag

r[fx.control.drag.axis]
A knob, vertical slider, and lever drag **vertically**: up increases, down
decreases; horizontal motion is ignored. A horizontal slider drags
horizontally: right increases. An XY pad drags both. A range control (Q,
width, mod-range) drags along its own axis per its widget. Value change is
proportional to accumulated delta since pointer-down (relative), never to
the cursor's absolute position over the widget.

r[fx.control.drag.sensitivity]
A full 0→1 sweep takes a fixed pixel distance (≈150–200 px) that is the
same for every control of a kind across every FTS plugin. Sensitivity is in
the **normalized** domain; the parameter's taper shapes the value, not the
drag.

r[fx.control.fine]
Holding **Ctrl** (Cmd on macOS) while dragging or wheeling is the fine
modifier (drag distance ×8 per unit; REAPER/Pro Tools convention). **Shift**
is also accepted as fine (FabFilter/Logic convention) at the same ratio.
Ctrl+Shift is ultra-fine (×32). Pressing or releasing the modifier
*mid-drag* changes the ratio from that point without jumping the value:
the drag is re-anchored at the current cursor position and value.

r[fx.control.drag.readout]
While dragging, the control shows its live value in its readout (or a
tooltip for controls without one), formatted by the parameter's display
function (unit included).

## Reset

r[fx.control.reset]
**Double-click** on a control's body resets it to the parameter's default
value, as one edit gesture. **Alt-click** does the same (Pro Tools/Logic
compatibility). A control without a meaningful default (an inert faceplate
control) ignores the gesture. A double-click MUST NOT also start a drag or
open text entry — the first press's drag is cancelled when the second click
arrives within the double-click window, and the value the first press wrote
is reverted to the pre-press value before the reset applies.

## Text entry

r[fx.control.text-entry]
Every value control offers keyboard entry of an exact value. The gesture is
a **single click on the value readout** (the number next to / under the
control) — or, where there is no readout, right-click on the control
opens a value prompt. The readout becomes an input pre-filled with the
current displayed value, fully selected, focused, and the control's drag
overlay is disabled while it is open. **Enter** commits, **Escape**
cancels, **Tab** commits and moves to the next control's readout in the
same group, focus-out commits (FabFilter/iZotope behaviour). An
unparseable string leaves the value unchanged and the field shakes/flashes
rather than silently closing.

r[fx.control.text-entry.parse]
Typed values are parsed by the parameter's parser with these shared
conventions (implemented once, in `fts-audio-ui`, and used by every plugin):
the unit suffix is optional and case-insensitive (`-6`, `-6dB`, `-6 db`);
frequency accepts `k` (`1k` = 1000 Hz, `2.5k`), `hz`/`khz`, and note names
(`A4` = 440 Hz, `C#3`, `C#3+13` cents); dB fields accept a ratio suffix
`x` (`2x` = +6.02 dB, `0.5x` = −6.02 dB); time accepts `ms`, `s`, and note
divisions (`1/8`, `1/8.`, `1/8T`) where the parameter is tempo-syncable;
percent fields accept `%`; **any** field accepts `N%` meaning *N% of the
normalized range* only when the parameter itself is not a percent
(FabFilter's `50%` = centre). Out-of-range input clamps.

## Wheel

r[fx.control.wheel]
The scroll wheel over a control nudges it by a coarse step (≈2% of range)
without a prior click; fine modifier gives a fine step (≈0.5%); ultra-fine
≈0.125%. Each wheel notch is its own edit gesture. Wheel over a stepped
(enum) parameter moves exactly one step per notch. Wheel over a readout or
label behaves like wheel over the control.

## Keyboard

r[fx.control.keyboard]
A focused control (Tab order follows visual order) responds to arrow keys
(↑/→ increase, ↓/← decrease, by the wheel coarse step; fine modifier gives
the fine step), Page Up/Down (10%), Home/End (min/max), Backspace/Delete
(reset to default), and Enter (open text entry).

## Stepped & bipolar parameters

r[fx.control.stepped]
A stepped parameter (enum, integer, on/off) snaps on every gesture: drag
moves one step per `sensitivity / steps` pixels, wheel one step per notch,
text entry accepts the step's label or its index. The readout always shows
the step label.

r[fx.control.bipolar]
A bipolar parameter (gain, pan, tilt) draws its value from the centre and
has a soft **detent** at the default: while dragging through the default
the value sticks for a few pixels (≈6 px) so landing on 0 dB / centre by
hand is reliable. Fine modifier disables the detent.

## Context menu

r[fx.control.context-menu]
Right-click on a control (where it does not open text entry) shows a menu
with: the parameter name and current value, *Reset to default*, *Enter
value…*, and host-side items when available (MIDI learn, automation, lock).
Right-click never starts a drag.

## Gesture table (normative summary)

| Gesture | Action |
|---|---|
| drag | edit (relative, axis per widget) |
| Ctrl/Cmd + drag, Shift + drag | fine ×8 |
| Ctrl+Shift + drag | ultra-fine ×32 |
| double-click body, Alt+click | reset to default |
| click readout | text entry (Enter commits, Esc cancels, Tab next) |
| right-click | context menu (or text entry where no readout) |
| wheel / + fine mod | coarse / fine step |
| arrows, PgUp/PgDn, Home/End, Backspace, Enter | keyboard edit / reset / entry |

Shift-click on a **style/profile selector** is reserved for stacking
(`fx.stack.add`), not a control gesture.
