# Parameter

The universal parameter contract. **Every** controllable value in the system —
every FX/plugin parameter, every soundsource/filter/amp control, every macro
target — is a `Parameter`. Parameters are addressable, linkable, and
modulatable by a single mechanism, so modulation, macros, MIDI learn, and
automation work identically everywhere. Reference implementation:
`features/fx/macromod/` (`BlockParameter`, `ParamTarget`, `MacroBank`).

This is a cross-cutting contract: it is NOT synth-specific. A guitar-rig reverb
knob, a sampler layer level, and a synth filter cutoff are all Parameters and
all participate in the same modulation/macro graph.

## Identity & addressing

r[signal.parameter]
A Parameter has a stable `id`, a display `name`, a normalized `value` in 0..1,
and metadata (unit, default, bipolar flag, and — when imported from a plugin —
the original DAW name). Its normalized value maps to the real (engineering)
value through the parameter's response curve.

r[signal.parameter.address]
A Parameter is addressed globally by a `ParamTarget { block_id, param_id }` — the
owning node's id plus the parameter's id. Any part of the system (a modulation
route, a macro binding, automation, the UI) refers to a parameter by this
address; addresses are stable across save/load.

r[signal.parameter.enumerable]
Every parameter-bearing node exposes its parameters (`params()`), each readable
and settable by id (`param_value`/`set_param`). A generic client (modulation
engine, macro system, UI) drives any parameter without knowing the node's type.

r[signal.parameter.response]
A Parameter carries a response/taper mapping normalized 0..1 → real value
(linear, exponential, logarithmic, or a multi-point custom curve). Modulation and
macro amounts act in the normalized domain unless a route specifies otherwise.

## Linking & modulation surface

r[signal.parameter.modulatable]
Any Parameter can be a modulation **target**: the modulation engine writes a
signed offset to it per render block (see [modulator.md](modulator.md)). A
parameter's effective value is `base + Σ(route offsets)`, clamped to range. The
base value (what the user set) is preserved independently of modulation.

r[signal.parameter.link]
Any Parameter can drive another: a parameter used as a modulation **source**
("param-modulates-param") links two controls, so moving one moves the other
through a route with its own amount + curve. Links compose (a linked parameter
may itself be modulated).

r[signal.parameter.mod-display]
The UI shows a parameter's modulation state: its base value, the live modulated
value, and a per-route modulation range indicator (the arc/ring a knob draws for
`mod_min..mod_max`). Editing the base and editing a route's amount are distinct
gestures.

## Automation, learn, save

r[signal.parameter.learn]
A Parameter can be MIDI-learned: arm learn, move a hardware control, and the
next incoming CC/note/MPE dimension binds to that parameter (source → target)
with a default amount + curve. (Reference: macromod `learn`.)

r[signal.parameter.automation]
A Parameter is automatable: its normalized value can be driven by an automation
lane / document track over time. Automation, modulation, and macros stack
predictably (base ← automation, then modulation offsets, then clamp).

r[signal.parameter.persist]
A Parameter's base value, response curve, and the routes/links/macro bindings
that reference it persist by stable `ParamTarget` address, so a saved patch
restores the full modulation graph.
