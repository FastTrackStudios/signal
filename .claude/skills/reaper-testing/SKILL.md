---
name: reaper-testing
description: "Use when writing or debugging automated tests that must run inside a real REAPER — `#[reaper_test]` integration tests, verifying a Dioxus panel actually opens and docks, or checking that an edit reaches a real take. Covers the daw-test harness, running REAPER on a private screenshottable Xvfb with a window manager, capturing panel windows, dioxus-test for headless UI logic, and the split between what belongs in a REAPER test versus the standalone backend."
---

# Testing inside REAPER

Three layers, and using the wrong one is the most common mistake:

| layer | harness | what it is for |
|---|---|---|
| UI logic | `dioxus-test` | components, events, rendering — headless Blitz DOM, no DAW |
| domain logic | `daw::standalone::Standalone` | anything you could implement yourself — real service calls, no REAPER |
| REAPER-only | `#[reaper_test]` | docking, selection, REAPER's own writes — needs REAPER running |

## Put the test in the right layer

**Default to standalone.** `Standalone` implements the same service traits
REAPER does, so a test against it exercises the real `Midi` / `Tracks` /
`Items` code paths without a DAW. A test that boots REAPER to check
arithmetic is a slow test for no reason.

Write a REAPER test only for what cannot exist elsewhere:

- a panel registering and docking through `reaper-dioxus`
- finding REAPER's *actual* item/track selection
- verifying REAPER itself reports a change (its writers are its own)

Design your code so this split is possible: make session/adapter types
**generic over the service traits**, not over a concrete backend. Then
the whole load → edit → write-back loop is standalone-testable and the
REAPER test only has to cover the seam.

### Standalone needs a project seeded

`Standalone::new()` has no projects, so `ProjectContext::Current`
resolves to nothing and every call fails with `NotFound`:

```rust
let daw = Standalone::new();
daw.seed_project(ProjectInfo {
    guid: "test-proj".into(), name: "Test".into(), path: String::new(),
});
```

## The trap: the test binary is a different process

A `#[reaper_test]` runs in its own process and talks to REAPER over a
socket. **It cannot see the extension's memory.** Asserting on a
`static` inside your module — a counter, a loaded document, a global
session — always reads the process-local value (usually zero) and proves
nothing, while *looking* like a real assertion.

Every assertion must go through the DAW RPC and read state REAPER owns.

If you need to observe something that only exists in extension memory,
add a **test-only action** that makes an observable change, then assert
on the change:

```rust
ActionDef::new("FTS_MYPANEL_TEST_TRANSPOSE", "…(test)", || {
    with_editor(|ed| ed.apply(&Edit::Transpose { .. }));
})
```

```rust
action(ctx, "FTS_MYPANEL_OPEN").await?;
action(ctx, "FTS_MYPANEL_TEST_TRANSPOSE").await?;
action(ctx, "FTS_MYPANEL_WRITE").await?;
assert_eq!(pitches(&item).await?, vec![72, 76, 79]); // ask REAPER
```

That single assertion covers load, edit and write: if the load silently
failed the transpose is a no-op and the take is unchanged.

## Assert on more than identity

Comparing only pitches will pass while positions and lengths are wrong.
A unit mismatch between what a backend reports and what your code
assumes sails straight through a pitch-only test — and is instantly
obvious in a screenshot. Assert starts and lengths too.

## Where the test file lives

In the crate whose **extension registers the panel** (normally
`fts-extensions`), not in the module's own crate. A test in the module
crate would be testing a panel nothing had loaded.

Register the module and add its test binary to the xtask's `packages`:

```rust
TestPackage {
    package: "fts-extensions".into(),
    test_binary: Some("my_feature".into()),
    ..
}
```

## Running it

```sh
cargo run -p fts-extensions-xtask                  # headless
cargo run -p fts-extensions-xtask -- --gui         # inherits $DISPLAY
cargo run -p fts-extensions-xtask -- <name-filter> # filters TEST NAMES
```

The filter matches test *names*, not files — passing a file stem
silently runs nothing and still reports success.

### Headless cannot open a GUI panel

`DISPLAY=""` aborts REAPER inside GDK the moment a Dioxus window is
created:

```
gdk_cursor_new_from_pixbuf: assertion 'GDK_IS_DISPLAY (display)' failed
```

It takes the daw socket down with it, so every *later* test in the run
fails with `Timed out waiting for REAPER socket` — a cascade that looks
like many broken tests but is one missing display.

`--virtual` is not a fix by itself: it expects an `fts-test` Xvfb
launcher that is not installed everywhere and **silently falls back to
headless** when missing.

## Running visibly, with screenshots

Use `scripts/reaper-test-visual.sh` in
`apps/extensions/reaper-fts-extensions`:

```sh
nix shell nixpkgs#openbox nixpkgs#bc -c \
  ./scripts/reaper-test-visual.sh [name-filter]
```

It brings up a private Xvfb, runs **openbox** on it, drives the suite
with `--gui`, captures the root window on an interval, and drops blank
frames. Output lands in `target/reaper-shots/`.

**Run a window manager.** This is the part that is easy to skip and
shouldn't be. A bare Xvfb has no WM, so windows are unmanaged: nothing
positions or stacks them, they have no decorations, and REAPER's Actions
List ends up covering the screen with your panel buried under it. With
openbox the root capture actually represents what a user would see.

### Capturing one window instead of the screen

When another window is in the way, grab the panel directly by its
geometry:

```sh
xwininfo -display :99 -root -tree \
  | grep -oE '0x[0-9a-f]+ .*1180x640' | grep -oE '^0x[0-9a-f]+' \
  | head -1 | xargs -I{} import -display :99 -window {} panel.png
```

Poll it in a loop — a panel opened by a test is only up for a second or
two.

## Read the extension log first

Faster than any screenshot for "did the panel come up":

```sh
tail -f ~/.local/state/fasttrackstudio/reaper-fts-extensions.log.$(date +%F)
```

```
Panel 'FTS_EXPRESSION_EDITOR' client rect w=1180 h=640
Created EmbeddedView for panel 'FTS_EXPRESSION_EDITOR'
Applied floating X11 window hints panel="…" xid=2097626
```

If the panel registered but a visibility assertion fails, trust the log
and the screenshot — the assertion is likely asking the wrong question
(e.g. docked-state for a floating panel).

## Headless UI logic: dioxus-test

For component behaviour that needs no DAW, mount on the vendored
`dioxus-test` (`libs/vendor/dioxus-test`) — a headless Blitz DOM with
real event dispatch, plus `render_png` for CPU-rasterized screenshots
with no GPU. See any `*-ui` crate's `tests/`.

Blitz quirks that bite:

- `<select>` renders as an empty box → use cycling buttons
- `<input type="range">` has no thumb or track fill → draw sliders in SVG
- inline `<svg>` works (serialized to usvg) but is sized as a **replaced
  element**; a flex child holding one needs `flex: 1 1 auto` plus a
  min-height, or it collapses or eats its siblings
- an absolutely-positioned child is **not** clipped by an
  `overflow: hidden` parent — clamp coordinates yourself
- `.focus()` before typing

## Housekeeping

REAPER resources live at `~/fts-dev` (override with
`FTS_REAPER_RESOURCES`). The runner spawns and kills its own REAPER; do
not `pkill reaper` broadly on a machine that may be running a live rig.

Worktree `target/` dirs are enormous and a full disk shows up as
`No space left on device` mid-build. `target/debug/incremental` is pure
cache and safe to delete **in your own worktree**; never delete another
worktree's build state.
