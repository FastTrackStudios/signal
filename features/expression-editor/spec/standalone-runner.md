# The standalone runner

The editor's development loop. Before this existed the editor ran in
exactly one place — a REAPER dockable panel — and `reaper-testing.md`
records that headless REAPER cannot open a Dioxus panel at all, so
seeing a change meant a private Xvfb, a window manager and a REAPER
launch. `expression-editor-standalone` puts the same component in a
desktop window over `daw-standalone` instead.

## Running it

```sh
# a demo scene (default: phrase)
cargo run -p expression-editor-standalone --example editor
cargo run -p expression-editor-standalone --example editor -- guitar
cargo run -p expression-editor-standalone --example editor -- --list

# a project on disk — its first editable item, or pick one
cargo run -p expression-editor-standalone --example editor -- song.rpp
cargo run -p expression-editor-standalone --example editor -- song.rpp --track Vox
cargo run -p expression-editor-standalone --example editor -- song.rpp --item 3

# a standard MIDI file
cargo run -p expression-editor-standalone --example editor -- part.mid --mode mpe
```

`--mode` overrides whatever the source implies, which is how one
document reaches all seven surfaces. `--size WxH` sets the window.

## Screenshots

`--example shot` takes the same arguments and rasterizes instead of
opening:

```sh
cargo run -p expression-editor-standalone --example shot -- \
    guitar --mode guitar --out target/gui-shots/guitar.png
```

It paints `App` — the *same* root the window mounts — on a headless
Blitz DOM through `dioxus-test`'s CPU rasterizer. CPU rather than the
wgpu offscreen path because a screenshot that needs a GPU cannot be
regenerated on a CI box, and a committed picture that nobody can
regenerate rots.

This is where a mode's committed screenshot (see the map's definition of
done) comes from.

## Shape

- **`src/`** is the loading: a command line in, an `Editor` out,
  through the same `daw` facade the REAPER module uses. No window in it,
  which is what makes it testable — `tests/load.rs` runs the whole thing
  headless, including analysing a real audio take through the standalone
  accessor.
- **`examples/`** is the window and the rasterizer. `nice-plug-dioxus`
  and `dioxus-test` are *dev*-dependencies so a consumer of the loading
  half does not link a GPU stack.

The document reaches the component through `stage()` rather than props,
because `open_standalone_with_state` takes a bare `fn() -> Element` —
and the REAPER panel mounts a propless root for the same reason. A root
that needed props would be a root only one host could build.

## What it is not

No arrangement view, no mixer, no transport. The editor is the product;
surrounding it with chrome that does not exist yet would make this a
second app to maintain. Write-back is wired as far as the session — the
`Standalone` backend is held for the runner's lifetime so the location a
session remembers stays meaningful — but no key saves yet.

## Known limits

- **A bare audio file is refused.** Analysing a take needs its length,
  and the standalone accessor returns silence past the end of a source
  rather than a short read, so a guessed length would analyse minutes of
  silence. Put the file on a track in a `.rpp`.
- **The window cannot be screenshotted through X11.** It opens at the
  right geometry with the right title under Xvfb + openbox, but `import`
  reads the X drawable and a wgpu surface is not in it, so the capture
  is black whether or not the surface painted. Pixel verification goes
  through `--example shot`, which walks the identical component tree.
- **`--example editor` needs a working GPU surface.** Under Xvfb's
  software stack the window maps but does not paint. `nice-plug-dioxus`
  has `cpu-rendering`/`softbuffer-blit` features for that case; enabling
  one here would flip the whole workspace's plugin rendering, so it has
  not been wired.
