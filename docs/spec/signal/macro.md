# Macros

A **macro** is a single control that drives many [Parameters](parameter.md) at
once — one knob that reshapes a whole patch. Macros are the player-facing top of
the [modulator](modulator.md) system: a macro is itself a modulator source
(`signal.modulator.param-source`), and each of its targets is an ordinary route.
Reference: `features/fx/macromod/` (`MacroKnob`, `MacroBank`, `Binding`, `Curve`).

## Bank & knobs

r[signal.macro.bank]
An instrument has a **macro bank**: a named, ordered set of macro knobs saved
with the patch. The bank is the assignable control surface a rig exposes (e.g. the
eight Live globals of a synth Part) and persists by stable id.

r[signal.macro.knob]
A macro knob has an `id`, a display `name`, a normalized `value` in 0..1, a
**bipolar** flag (bipolar knobs are neutral at center, unipolar at zero), and a
set of **bindings**. Setting the knob re-evaluates every binding.

## Bindings

r[signal.macro.binding]
A binding maps the knob to one target `{ target: ParamTarget, amount ∈ [-1,1]
(signed), curve, enabled }`. One knob may bind many parameters, each with its own
amount, direction, and curve, so a single gesture moves a set of parameters by
different, individually-shaped amounts. A binding is exactly a modulator route
whose source is the macro.

r[signal.macro.curve]
Each binding carries a response curve (linear, exp/log, or multi-point custom)
mapping the knob position to that target's contribution, so targets can respond
non-linearly and reach their extremes at different knob positions.

r[signal.macro.sum]
A parameter driven by a macro obeys the Parameter modulation contract
(`signal.parameter.modulatable`): the macro's contribution sums with the
parameter's base and any other routes, then clamps. Multiple macros may target the
same parameter; their contributions add.

## Composition

r[signal.macro.sub]
A macro binding MAY target another macro knob (a **sub-macro**), so macros nest:
one master control drives several macros, each driving its own parameters. Nesting
is acyclic; the engine evaluates in dependency order.

r[signal.macro.learn]
A macro knob is MIDI-learnable (`signal.parameter.learn`): arm it, move a hardware
control, and that CC/MPE dimension drives the knob. This lets a hardware control
map to many parameters through one macro.

r[signal.macro.persist]
The macro bank — knob values, bipolar flags, every binding's target/amount/curve,
and sub-macro links — persists with the patch by stable id, restoring the full
control surface on load.
