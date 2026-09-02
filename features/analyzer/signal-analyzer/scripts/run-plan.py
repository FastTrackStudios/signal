#!/usr/bin/env python3
"""Execute a measurement plan: drive axes, then every mode, one at a time.

Jobs, and what each is for:

  drive        The main capture. The primary drive control swept across
               THD-spaced settings, at several frequencies and levels. This
               is the saturation family a 2-D waveshaper is fitted from.
  second       The runner-up axis, where one exists — the Distressor
               saturates at Output as well as Input, the LA-3A at HF
               Emphasis as well as Peak Reduction.
  mode-*       One capture per state of every selector, with the drive held
               at a fixed mid setting. Cheap, because what is wanted from a
               mode is how it *changes* the curve, not a fresh drive sweep
               inside each one.

Mode jobs deliberately do not re-probe the drive. A full cartesian product
of modes against drive against level is not affordable — the Distressor
alone is 8 x 8 x 6 states, some 55,000 renders — and it answers a question
nobody asked.

Resumable: a job whose output already exists is skipped, so an interrupted
run picks up where it stopped. Progress and ETA come from the job count,
weighted by how many renders each job actually costs.

    run-plan.py <plan.json> --out <dir> [--bin ./target/release/examples/saturation_capture]
"""
import json, os, subprocess, sys, time

CORE_FREQS = "100,1000,5000"
CORE_LEVELS = "-30,-24,-18,-12,-6,0"
MODE_LEVELS = "-24,-12,-6,0"
DRIVE_STEPS = 8
SECOND_STEPS = 6


def arg(name, default=None):
    return sys.argv[sys.argv.index(name) + 1] if name in sys.argv else default


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    plan = json.load(open(sys.argv[1]))
    out_root = arg("--out") or "plan-out"
    binary = arg("--bin", "./target/release/examples/saturation_capture")
    path = plan["plugin_path"]
    pins = ";".join(f"{p['name']}={p['value']}" for p in plan.get("pins", []))

    jobs = []
    engage = plan.get("engage") or []
    engage_set = ";".join(f"{e['name']}={e['value']}" for e in engage)

    if engage:
        # Under load, swept by level: how the unit saturates as it is hit
        # harder while actually being asked to do something.
        jobs.append(("engaged-level", [
            "--set", engage_set, "--freqs", CORE_FREQS,
            "--levels", "-36,-30,-24,-18,-12,-9,-6,-3,-1,0",
        ], 10 * 3))
        # And the flat baseline beside it, so the difference the engage state
        # makes is visible rather than assumed.
        jobs.append(("flat-level", [
            "--freqs", CORE_FREQS, "--levels", "-24,-12,-6,-3,0",
        ], 5 * 3))
        # Each band swept on its own: how far a band is pushed *is* the drive
        # on a passive EQ, so this is that unit's drive axis.
        for axis in plan.get("engage_axes") or []:
            safe = "".join(c if c.isalnum() or c in "-_" else "_" for c in axis)
            jobs.append((f"engage-{safe}", [
                "--set", engage_set, "--drive-param", axis, "--drive-steps", "6",
                "--freqs", "1000", "--levels", "-12,-6,-3,0",
            ], 6 * 4 + 17))

    if not engage and (plan.get("kind") == "level" or not plan.get("drive")):
        # No drive control: sweep how hard the unit is hit instead, finely and
        # over a wide range, since that is the only axis left.
        jobs.append(("level", [
            "--freqs", CORE_FREQS,
            "--levels", "-48,-42,-36,-30,-24,-18,-12,-9,-6,-3,-1,0",
        ], 12 * 3))
    if plan.get("drive"):
        jobs.append(("drive", [
            "--drive-param", plan["drive"], "--drive-steps", str(DRIVE_STEPS),
            "--freqs", CORE_FREQS, "--levels", CORE_LEVELS,
        ], DRIVE_STEPS * 3 * 6 + 17))
    if plan.get("second_axis"):
        jobs.append(("second", [
            "--drive-param", plan["second_axis"], "--drive-steps", str(SECOND_STEPS),
            "--freqs", "1000", "--levels", CORE_LEVELS,
        ], SECOND_STEPS * 6 + 17))
    for mode in plan.get("modes", []):
        for state in mode["states"]:
            # A filesystem-safe label that still says which state it was.
            label = "".join(c if c.isalnum() or c in "-_" else "_" for c in state["text"])[:24]
            name = "mode-" + "".join(
                c if c.isalnum() or c in "-_" else "_" for c in mode["name"]
            ) + "-" + label
            setting = f"{mode['name']}={state['value']}"
            jobs.append((name, [
                "--set", setting, "--freqs", "1000", "--levels", MODE_LEVELS,
            ], 4))

    todo = [(n, a, w) for (n, a, w) in jobs
            if not os.path.exists(os.path.join(out_root, n, "saturation.json"))]
    done_already = len(jobs) - len(todo)
    total_weight = sum(w for _, _, w in todo) or 1
    print(f"{plan['plugin_name']}: {len(jobs)} jobs, {done_already} already done, "
          f"{len(todo)} to run (~{total_weight} renders)")
    if plan.get("note"):
        print(f"  note: {plan['note']}")

    started = time.time()
    weight_done = 0
    failures = []
    for i, (name, extra, weight) in enumerate(todo, 1):
        dest = os.path.join(out_root, name)
        os.makedirs(dest, exist_ok=True)
        cmd = [binary, "--plugin", path, "--out", dest, "--no-sweep"] + extra
        if pins:
            cmd += ["--set", pins]
        elapsed = time.time() - started
        rate = elapsed / weight_done if weight_done else 1.5
        eta = rate * (total_weight - weight_done)
        print(f"  [{i}/{len(todo)}] {name:<34} ETA {int(eta//60)}m{eta%60:02.0f}s", flush=True)
        with open(os.path.join(dest, "capture.log"), "w") as log:
            r = subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT)
        if r.returncode != 0 or not os.path.exists(os.path.join(dest, "saturation.json")):
            failures.append(name)
            print(f"      FAILED (exit {r.returncode}) — see {dest}/capture.log")
        weight_done += weight

    took = time.time() - started
    print(f"── {plan['plugin_name']} done in {int(took//60)}m{took%60:02.0f}s"
          + (f", {len(failures)} failed" if failures else ""))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
