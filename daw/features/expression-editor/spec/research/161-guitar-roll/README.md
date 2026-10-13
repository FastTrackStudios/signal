# #161 — six-string roll with bend flow (prototype)

Committed output of the PNG screenshot harness, so the pictures the
resolution comment argues from survive `target/` being wiped:

```sh
cargo test -p expression-editor-ui --test screenshots
```

| file | what it shows |
|---|---|
| `26-guitar.png` | bends **on the string row** — the line lifts off its string and comes back |
| `26b-guitar-bend-lane.png` | the same riff with every bend in a **lane below**, roll left as a clean grid of fret numbers |
| `26c-guitar-bend-both.png` | both at once |
| `40-guitar-zoom-close.png` | one bend filling the screen |
| `41-guitar-zoom-out-2x.png` | ~19 px per string — the last readable step |
| `42-guitar-zoom-out-4x.png` | ~10 px per string — fret numbers overprint |
| `44-guitar-zoom-out-16x.png` | the camera's own floor (3.5 px/semitone); six strings are one band |

The scene is `demo::Scene::{Guitar, GuitarLane, GuitarBoth}` and the
rendering is `expression_editor_ui::guitar`. Both are prototype code and
are meant to be thrown away once the questions in #161 are answered.
