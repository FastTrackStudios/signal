# Rig pages: sections you swipe between

Decided with the user on 2026-09-29, for signal's Control view.

A rig is shown as **sections**. Each section is a page built from frame surfaces: a block can
have any face it wants, and a section arranges its modules. A **section bar** runs across the top
of every page: one small icon per section, with a dot under the current one. Tap an icon or swipe
to move between sections.

How much fits depends on the screen:
- **Phone:** one section per screen.
- **Tablet or desktop:** sections sit side by side or in rows. For example, Drives next to
  Amp/Cab.
- **Any module:** zooms to fill the screen (for example, the Time module's two delays and two
  reverbs).

## Sections, in signal order

| # | Section | Holds |
|---|---|---|
| 1 | Input | pitch, envelope filter, compressor, pre-EQ |
| 2 | Drives | the drive pedals, on a pedalboard |
| 3 | Pre-FX | the spring-and-tremolo tank, a slapback delay |
| 4 | Amp/Cab | the amp standing on its cab (the cab is optional), plus the gate |
| 5 | Post EQ/Comp | EQ and compressor after the amp |
| 6 | Post-FX | the Modulation and Motion modules |
| 7 | Time | delays and reverbs |

## Modules: saved views

A **module** is a saved view of one or more sections, laid out left to right in signal order,
that the player configures and saves presets for as one unit. A module ignores the section
boundaries it spans: Drives and Amp/Cab have Pre-FX between them in the chain, but a player tunes
drives against the amp, so they save together.

Examples:
- **Drive → Amp:** the drives in a row, then the Amp/Cab/Gate section. Built as
  `05-module-drive-amp`.
- **Time:** the pre-amp time effects (the Pre-FX section: tank and slapback) on top, the Time
  section (post delays and reverbs) under them.

A module's preset stores every param of every unit in it. The section bar shows a dot under each
section the module spans.

## First rig: blues

This rig follows the John Mayer X chain. Its drives and effects are generic recreations of the
real pedals the plugin models: no brand or model names, but their looks.

| Plugin unit | Real pedal | Section |
|---|---|---|
| Justa Boost | Keeley Katana (clean boost) | Drives |
| Halfman OD | Klon Centaur | Drives |
| Tealbreaker | Marshall Bluesbreaker + Ibanez TS-10 (two-in-one) | Drives, as two pedals |
| Antelope Filter | EHX Q-Tron | Input |
| Millipede Delay | Way Huge Aqua-Puss | Pre-FX (slapback) |
| Gravity Tank | Neural's own spring + harmonic tremolo | Pre-FX |
| Amps | 1964 Fender Vibroverb, Dumble Steel String Singer #002, Two-Rock prototype | Amp/Cab |
| Dream Delay | Providence Chrono Delay | Time |
| Studio Verb | Strymon Flint (reverb) | Time |

Sources:
- Guitar.com review: https://guitar.com/reviews/effects-pedal/hands-on-neural-dsp-archetype-john-mayer-review/
- Guitar World: https://www.guitarworld.com/gear/plugins-apps/neural-dsp-archetype-john-mayer-x
- Neural DSP manual: https://neuraldsp.com/manual/archetype-john-mayer-x

Reference photos are in `~/Downloads/Reference Images/real-pedals/`.
