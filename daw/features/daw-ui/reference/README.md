# Reference screenshots

The ground truth the panel-convergence loop measures against. Everything in
`daw-ui`'s measured geometry — band heights, knob proportions, the 0.70
panel tint, the TCP columns — traces to these images, so they are primary
sources and live in the tree. They used to live in `target/theme-shots/`,
where one `cargo clean` deleted the evidence behind every constant.

`tests/compare.sh` diffs the painted panels against crops of these; see
issue #237 for pointing it here instead of at `target/`.

## The images

### `mcp-zoom.png` — the mixer strip, 3× zoom

A 3× enlargement of REAPER wearing the FTS theme, mixer visible, tracks
Kick/Snare/OH/Bass at 86px-wide strips. The strip is 232 rows at 1×; its
index plate is 20 rows (60 in the zoom), which is how the zoom factor was
fixed. The Kick crop the loop uses is `-crop 254x694+18+6` of this image.

Every mixer measurement at 1× came from here: coloured band 29 rows
(pan 33 has collapsed to unlabelled at this height... it is 6+22 minimal —
measure, don't trust prose), fader cap at 0.744 of travel at unity, scale
text 17×8 for `-18-`.

### `tcp-ref.png` — the track panel and a squeezed mixer, 1×

Full REAPER window, 1024×768, TCP visible with Kick/Snare/OH rows. A row
is 70 rows tall plus a 1px divider; the tint ends at x=296, the meter
section runs to 343. The Kick row crop is `-crop 344x70+0+107`.

The mixer at the bottom of this shot is *squeezed* — its strips are ~43
wide — so it is valid for vertical facts only, never horizontal ones.

The TCP colour here is `#9D3C55` painted from a raw track colour of
`#E0567A`, which is the pair the 0.70 panel tint was derived from
(`dress::panel_tint`).

## Known gaps

- **No tall mixer at normal width.** The tall-strip layout (adaptive scale
  ladder, filled input section, spread button column) was measured off a
  user-supplied screenshot that was never saved into the repo, plus
  `rtconfig.txt`. An `fts-themer shot` of a tall mixer with 86-wide strips
  would let the loop verify those numbers; the one attempt captured a
  blank screen (the mixer window did not open in time).
- The two references disagree about the pan section because REAPER has
  `narrowMode` — `rtconfig` branches `mcp.pan`, `mcp.width` and
  `mcp.recinput` on it, and the squeezed mixer at the bottom of
  `tcp-ref.png` is in it. **Decided (#245): width variants are not
  modelled until the wide strip has converged in REAPER itself (#212).**
  The strip follows the tall, 86-wide reference; `Collapse` models height
  only. When a measurement from one reference contradicts the other about
  the pan area, that is the mode, not an error in either image — do not
  "fix" the strip toward the squeezed shot. The seam a future width model
  goes through is `Collapse` growing a width input beside its height, not
  a second strip component.

## Reproducing

```sh
cargo run -p fts-themer --bin fts-themer -- \
  --theme features/reaper/fts-theme shot --out <png> [--action 40078] --settle 25
```

`40078` toggles the mixer (numeric REAPER command ids work as of the same
change that added this directory). Run under the `#reaper-test` dev shell;
see the reaper-testing skill for the Xvfb/FHS gotchas.
