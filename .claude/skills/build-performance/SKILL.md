---
name: build-performance
description: "Diagnose and fix slow builds or a full disk in this monorepo — target/ growth across worktrees, link times, debuginfo, opt-level choices, sccache, cargo-sweep, and cargo rail. Use when builds feel slow, when the dev disk fills up, when deciding whether to change a profile knob in the root Cargo.toml, or before benchmarking any build-time change."
---

# Build performance in this tree

~160 workspace members, one `target/` per worktree, a dozen worktrees.
The two failure modes are **disk** (target dirs in the hundreds of GB)
and **link time**. This is what was measured, what was changed, and how
to measure the next thing without fooling yourself.

## Measure before you change anything

Everything below was established by measurement, not by reading blog
posts. Do the same — most "obvious" Rust build wins are worth nothing
here, and one of them (sccache across worktrees) is worth literally zero.

```bash
just disk            # per-worktree target/ sizes, largest first
just timings -p foo  # cargo --timings → per-crate Gantt + link tail
just cache-stats     # sccache hit rate
du -sh target/debug/{deps,incremental,build}
size --format=sysv <binary> | awk '/^\.debug/{s+=$2} END{print s/1e9" GB"}'
```

That last one is the important one. **Check the debuginfo share of a
big artifact before blaming anything else.**

### The contention trap — read this before benchmarking

This machine has 32 cores and other agents build on it. A background
`cargo rail unify` or another agent's build will make a change look
*slower* than what it replaced. A real measurement was thrown away this
way (O1 "measured" at 316s vs O3 at 221s, purely because load was ~50).

Before an A/B: `uptime` and `pgrep -c rustc`. If load is high or rustc
is running, wait. Build each arm into its own `CARGO_TARGET_DIR` under
the scratchpad, sequentially, never in parallel.

Also: only one cargo command at a time per worktree — they share a
target-dir lock and will block each other.

## What is already configured, and why

Root `Cargo.toml`, `.cargo/config.toml`, `nix/modules/toolchain.nix`,
`nix/modules/shells/default.nix`.

### Debuginfo — the dominant cost

`profile.dev` is `debug = "line-tables-only"` + `split-debuginfo = "unpacked"`.

The finding that drove it: the `action_ids` test binary was 1.62 GB, of
which **1.59 GB (98%) was `.debug_*` sections**, and `ld.bfd` copies all
of it into every artifact on every relink. Result: **1.62 GB → 211 MB,
−87%.**

You keep backtraces with file:line, and samply/perf still symbolize.
You lose debugger variable inspection — for that use `--profile dev-dbg`
(full DWARF, otherwise identical to dev).

Platform notes:
- wasm is unaffected — `-Csplit-debuginfo` is unstable on wasm32 and
  cargo omits it automatically. Verified with `cargo build -v`.
- macOS/iOS: `unpacked` is already the platform default.

### Linker

mold, via `-C link-arg=-fuse-ld=mold` in `.cargo/config.toml`.

**Precedence trap:** `target.<triple>.rustflags` *replaces*
`build.rustflags` — cargo does not merge them. The linux table has to
repeat `force-frame-pointers`. If you add a global rustflag, add it in
both places or it silently vanishes on Linux.

mold comes from the devshell. After pulling these changes, `direnv
reload` — otherwise every link fails with `cannot find -fuse-ld=mold`.

Changing any rustflag invalidates **every** cached artifact in the
worktree. Expect one full rebuild, and prefer to `just sweep` first.

### opt-level

- `profile.dev` base: 1 (workspace members).
- `package."*"`: **1** — dependencies. Was 3, which meant full LLVM
  optimization on ~1500 registry crates for every cold build and every
  new worktree.
- An explicit allowlist back at **3** for audio-thread crates, both
  workspace (`audiocore-dsp`, `daw-audio-graph`, `signal-sampler`,
  the `*-dsp` crates, …) and third-party (`rubato`, `symphonia-*`,
  `rustfft`/`realfft` + `transpose`/`strength_reduce`/`primal-check`,
  `dasp_*`, `zstd-*`, `blake3`, `phon*`).
- `build-override`: opt-level 3, debug off — build scripts and proc
  macros. `package."*"` does **not** cover these, so syn/facet/
  architect/dioxus-rsx were building at O0 and then executing thousands
  of times per build.

**A dev run of the rig must never xrun.** That is why the allowlist
exists. If you hit xruns on a path it doesn't cover, run `--release`
(launch paths already do) or add the crate — do not raise `"*"` back
to 3 wholesale. Regenerate the list with `cargo tree` over the audio
crates; note `package."*"` matches dependencies only, never workspace
members.

## sccache — know its actual scope

Wired as `RUSTC_WRAPPER` in the devshell. Opt out: `FTS_NO_SCCACHE=1`.
Cache lives on the dev disk, not `~/.cache` (which would fill `/`).

Measured on this tree:

| Scenario | Hit rate |
|---|---|
| Rebuild the **same path** after wiping `target/` | ~100% |
| Build the **same code in a different worktree** | **0%** |

**It does not dedupe worktrees.** The target-dir path is part of
sccache's Rust cache key. `SCCACHE_BASEDIR` does not fix this — it
makes paths *relative*, not *equal*, so `FastTrackStudio/target` and
`herdr-worktrees/FastTrackStudio/target` still hash differently. This
was tested with basedir at the tree root and per-target-dir; both 0%.
Don't re-litigate it without new evidence.

What it buys: wiping `target/` becomes cheap to recover from, which is
what matters when disk is the binding constraint.

It costs nothing in incremental compilation: cargo emits `-C incremental`
for workspace members only, and sccache skips exactly those units.

Gotcha: sccache is a **daemon** and reads its config at server start.
Changing `SCCACHE_*` in your shell does nothing until
`sccache --stop-server`. Confirm with `sccache --show-stats`.

## Disk: cargo never garbage-collects

There is no cargo GC on stable. A long-lived worktree accumulates a new
copy of every artifact each time a fingerprint changes — **56 stale
copies of a single crate** were measured, plus 77 G of
`debug/incremental` in one worktree.

```bash
just sweep              # this worktree, keep artifacts touched in 7d
just sweep-all          # every worktree from `git worktree list`
just sweep-incremental  # drop incremental caches (pure cache, always safe)
```

**Never delete another agent's worktree target dir** without asking —
most worktrees here belong to other agents mid-task. `just sweep-all`
is a time-based sweep, not a wipe, which is why it's safe.

## cargo rail

`.config/rail.toml` is configured for unused-dep detection, dead-feature
pruning, and version unification.

`cargo rail unify --check` takes **well over an hour** on a cold
`compiler_diag_cache` and prints nothing while it works — it is
compiling the tree. Run it with `run_in_background`, give it an isolated
`CARGO_TARGET_DIR` so it doesn't take the worktree's target lock, and
don't benchmark anything while it runs (see the contention trap).

For a fast dedup signal instead: `cargo tree --duplicates --workspace`.
Baseline was **469 duplicated packages**, mostly transitive through the
Blitz/servo stack.

## Ideas already evaluated — don't redo these

- **Shared `CARGO_TARGET_DIR` across worktrees** — causes fingerprint
  clobbering with concurrent agent builds. Use worktree-local targets.
- **sccache for cross-worktree dedup** — 0%, see above.
- **`-Z threads=N` (parallel frontend), cranelift** — need nightly; the
  workspace is pinned to stable 1.94.
