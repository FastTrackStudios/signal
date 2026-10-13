# Automated converter harness

The PT Reaper Converter v1.5.4 has a **headless CLI mode** that wasn't
documented in the GUI. Discovered 2026-05-17 by scanning the ARM64
binary for Swift small-string immediates matching `--*`. Combined with
voyager (macOS Tailscale host) + Frida, this gives us a fully
scriptable reverse-engineering loop.

## CLI flags discovered

```
PTReaperConverter --convert <input> <output> [flags] [--options '{"k":bool,...}']

Directions:
  input.ptx output.rpp    PT → Reaper conversion
  input.rpp output.ptx    Reaper → PT conversion (reverse — item 9!)

Flags (PTX → RPP only):
  --mediaDir <path>      Output dir for merged/poly audio
  --copyAllAudio         Copy all session audio files to Media/
  --copyVideo            Copy video files into <out>/Video Files/
  --routingAudio         (presumed; reads as --routinlAudio from
                          fragmented MOV/MOVK, decode unclear)
```

Other recognized flags (purposes not yet probed):
  `--info`, `--preflight`, `--gen-test(s)`, `--debug-dump-spec`,
  `--regen-clean-template`, `--no-standalone`, `--audioDir`,
  `--options <json>`

## Stage progress output

Headless mode prints structured progress for every conversion. This is
a **gift** for RE — each line names a pipeline stage and (in
parentheses) the count of items processed. Use it to map our parser's
stages onto the converter's:

```
Converting: Color Testing.ptx
  Loading PTX file...
  Finding tracks...
  Finding regions (72 tracks)...
  Resolving source files (0 regions)...
  Detecting channel formats...
  Finding playlists...
  Finding clip gain...
  Finding clips (0 gain entries)...
  Finding fades (0 clips)...
  Finding automation (0 points)...
  Finding routing...
  Converting clips (0)...
  Building RPP (72 auto tracks)...
Merge: found 0 unique file pairs to merge
Fade matching: 0 fade-ins, 0 fade-outs (clips: 0, fades: 0)
OK: <output>.rpp
```

This is the canonical pipeline structure. Our parser should mirror
each stage's counts when given the same input.

## Voyager runner: `~/pt-re/run.sh`

Wraps the converter with optional Frida injection. On voyager:

```bash
~/pt-re/run.sh [--hook script.js] [--log path] <input> <output>
```

If `--hook` is given, Frida spawns the converter under instrumentation
and writes the JS log to `--log`. The conversion still completes
headlessly.

## Linux driver: `scripts/pt-convert.sh`

End-to-end remote driver. Uploads input + optional hook to voyager,
runs the converter, pulls output + log back.

```bash
scripts/pt-convert.sh \
  --hook scripts/frida/harness_emit.js \
  --log /tmp/lotf.frida.log \
  /home/cody/Downloads/LotF.ptx \
  /tmp/lotf.rpp
```

~5 seconds per fixture (network + Frida startup dominates; the
conversion itself is <500 ms).

## Frida harness

`scripts/frida/harness_emit.js` — 24 hooks armed on RPP emit sites
(MUTESOLO, PEAKCOL, MARKER, FADEIN, AUXRECV, etc.). Each hook captures
`x0..x28` at the emit instruction and tags with the last-seen track
name. Output as JSON lines: `EMIT {"f":<feature>,"t":<track>,"x...":...}`

See `pt-reaper-converter-emit-sites.md` for the address catalog.

## Iteration workflow

For each roadmap feature:

1. Pick the relevant fixture (LotF for sends, Color Testing for
   palette, future `mute-automation.ptx` for envelopes, etc.).
2. Run: `pt-convert.sh --hook harness_emit.js --log f.log fixture.ptx out.rpp`
3. Compare emit captures against output `.rpp` content to identify
   which register holds each feature's value.
4. If a feature needs more hooks (e.g. envelope emit, item emit),
   extend `harness_emit.js` with the address — discoverable via
   `pt-reaper-converter-emit-sites.md` scan.
5. Trace backward to the read site — once we know which register
   holds the value at emit, follow the dataflow in
   `voyager:/tmp/arm64_disasm.txt` to find which `ldr` from the PTX
   buffer fed it.
6. Update our Rust parser to read the corresponding PTX bytes.
7. Run `pt-convert.sh` on the same fixture again with both directions
   (we now have `--convert .rpp .ptx` for the reverse). Use to
   validate the writer.

## Reverse-direction writer validation

When item 9 (RPP→PTX writer) is built, every output we produce can be
re-converted through `--convert generated.rpp roundtrip.ptx` and
compared against the original `.ptx`. The converter's reverse path
becomes our writer's correctness oracle.
