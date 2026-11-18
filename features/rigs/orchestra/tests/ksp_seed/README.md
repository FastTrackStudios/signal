# KSP Seed — reproducible Kontakt renders

Kontakt's KSP `random(a, b)` (decoded from Kontakt 8.7.1, macOS arm64,
`BScriptParser::EvalExpression`) is

```
v = ((rand() << 30) | (rand() << 15) | rand()) & 0x7fffffff   // 32-bit shifts
random(a, b) = v % (max − min + 1) + min
```

on the C library's `rand()`: on macOS, Park–Miller (`x = 16807·x mod 2^31−1`,
unseeded state 1). Kontakt seeds it with `srand(time(NULL))`, so two renders
of one project draw differently.

`ksp_seed.c` is a CLAP plugin. Placed in a track's FX chain right before
Kontakt and run with `KSP_SEED=<n>`, it rebinds Kontakt's imports of `rand()`
/ `srand()` (once per process) to a Park–Miller state, pinned to `n` at
transport start, so every KSP draw depends only on the seed. KONTRA replays
the same draws with `KONTRA_KSP_SEED=<n>` (local KONTRA branch
`local/inspect-vars`). `KSP_SEED_LOG=<file>` logs each call with its caller
(offset in the Kontakt 8 image), the track and the thread. No seed, the
plugin does nothing.

**One state per track (1.2).** Each instance holds its own state; its
`process()` points a thread-local at it, and the Kontakt after it in the chain
runs on that thread in that block (REAPER runs a chain serially on one
thread), so it draws from that track's state only. Several seeded Kontakt
tracks in one project — a batch render — each draw exactly what they draw
rendered alone, however REAPER's worker threads interleave them (1.1 had one
global state: their draws interleaved, even inside one `random()`'s three
calls). A track named `<anything>@<n>` seeds with `n` (CLAP track-info),
else `KSP_SEED`. A `rand()` on a thread no instance ran on would draw from a
shared fallback state; Kontakt 8.7.1 makes none during a render (every KSP
draw is on the processing thread; consecutive blocks of one track may run on
different worker threads — the state follows the instance, not the thread).
One instance per track behaves as 1.1 did: the CSSS Violin I probe renders
PCM-identical to its 1.1 reference.

Build on voyager (CLAP headers from free-audio/clap 1.2.2, MIT):

```sh
clang -O2 -arch arm64 -arch x86_64 -bundle -I clap/include \
  -o ~/Library/Audio/Plug-Ins/CLAP/KspSeed.clap/Contents/MacOS/KspSeed ksp_seed.c
codesign -f -s - ~/Library/Audio/Plug-Ins/CLAP/KspSeed.clap
```

plus an `Info.plist` (`CFBundleExecutable KspSeed`, package type `BNDL`), and a
`[KspSeed.clap]` entry in the render config's `reaper-clap-macos-aarch64.ini`
(a rebuilt binary is rescanned by the next headless run; its `_=` stamp line
changes). The sources and the 1.1 bundle live in `~/orchestra-ab/re/ksp-seed/`
on voyager; install by building into a staged copy of the bundle and moving
it into place, so a render already running keeps its loaded copy.

`seed_renders.sh <sec dir> <mode> <name:seed>...` renders seeded copies of
`<mode>_r1.rpp`, one REAPER each. `../seed_batch.py` renders many seeded jobs
(tag, mode, chain, MIDI, seed) as the stems of one REAPER and lands each at
`target/orchestra-ab/sec/<tag>/<mode>/<mode>_seed_<name>.wav`, cut to its own
length; see its docstring. Checked on the four CSSS solo strings × ten
probe modes (40 tracks, seed 777): every stem has its single render's
length and alignment; 24 are PCM-identical to it, the 16 marcato / marc2 /
trem / rebow stems differ only by Kontakt's ±1 LSB jitter (below; worst
unit −125.7 dB, the same as two plain single renders of those modes). Two
batches of the same 20 jobs agree the same way. Render time: 20 tracks in
29 s against 142 s for 20 single renders; 40 tracks in 44 s.

Kontakt itself is not bit-exact from run to run in some modes: two unseeded
renders of a CSSS tremolo or marcato probe (no KSP draws in tremolo) differ
by ±1 LSB (24-bit) in a few hundred frames, from the same frame on, −130 to
−140 dB overall (Kontakt's plugin multiprocessing, `numCPUs_VST = 10` in
`com.native-instruments.Kontakt 8.plist`, is the likely source; untested).
A batch stem matches its single render to that same jitter.

Verified: two `KSP_SEED=777` renders of the CSSS Violin I probe null to
−300 dB on every unit; the first draw is 16807 × 777.
