#!/usr/bin/env python3
"""Run a probe through KONTRA (the library's own script) and compare its
voices with ours, event by event.

usage: shreddage_kontra.py <probe.mid> <instrument.nki> <ours.trace> <outdir> [--seed N]

KONTRA is the reference for what the script commands: it runs the NKI's
real KSP on its own engine. This renders the probe with
`kontra-inspect play` (env KONTRA_INSPECT, default the inspect-vars build)
into <outdir>/kontra.wav and kontra.json, then pairs each KONTRA voice
start (frame, sample file, source position) with ours from
`render_fretted --trace` and prints every difference. Also reports what
KONTRA could not answer. `--seed` sets KONTRA_KSP_SEED (KSP random()'s
state at transport start; default 1): render ours with the same
`render_fretted --ksp-seed`, and Kontakt with the KSP Seed CLAP at it.
KONTRA could not answer (`unhandled_actions`, `unanswered_pars`): a script
branch that reads one of those may not be Kontakt's.

The NKI must sit in its installed library folder with Samples/*.nkx beside
it, or KONTRA renders silence (the trace is still complete).
"""
import json, os, re, struct, subprocess, sys

KONTRA = os.environ.get(
    "KONTRA_INSPECT", "/run/media/Development/KONTRA-inspect-vars/target/release/kontra-inspect")
RATE = 48_000


def scenario(mid):
    """A Standard MIDI File's note/cc events at RATE frames (format 0/1, tempo map)."""
    b = open(mid, "rb").read()
    ntrk, div = struct.unpack(">HH", b[10:14])
    p, evs = 14, []
    for _ in range(ntrk):
        ln = struct.unpack(">I", b[p + 4:p + 8])[0]
        q, end, t, run = p + 8, p + 8 + ln, 0, 0
        while q < end:
            v = 0
            while True:
                c = b[q]; q += 1; v = (v << 7) | (c & 0x7F)
                if c < 0x80:
                    break
            t += v
            st = b[q]
            if st == 0xFF:
                ty = b[q + 1]; q += 2; n = 0
                while True:
                    c = b[q]; q += 1; n = (n << 7) | (c & 0x7F)
                    if c < 0x80:
                        break
                if ty == 0x51:
                    evs.append((t, "tempo", int.from_bytes(b[q:q + 3], "big")))
                q += n
                continue
            if st & 0x80:
                run = st; q += 1
            s, d1 = run, b[q]
            d2 = b[q + 1] if s & 0xF0 not in (0xC0, 0xD0) else 0
            q += 2 if s & 0xF0 not in (0xC0, 0xD0) else 1
            evs.append((t, "ev", (s, d1, d2)))
        p = end
    evs.sort(key=lambda e: (e[0], e[1] != "tempo"))
    out, us, last_t, sec = [], 500_000, 0, 0.0
    for t, kind, x in evs:
        sec += (t - last_t) * us / 1e6 / div
        last_t = t
        if kind == "tempo":
            us = x
            continue
        s, d1, d2 = x
        f = round(sec * RATE)
        if s & 0xF0 == 0x90 and d2 > 0:
            out.append({"frame": f, "type": "note_on", "key": d1, "velocity": d2})
        elif s & 0xF0 in (0x80, 0x90):
            out.append({"frame": f, "type": "note_off", "key": d1})
        elif s & 0xF0 == 0xB0:
            out.append({"frame": f, "type": "cc", "cc": d1, "value": d2})
    return {"events": out}


def ours_starts(trace):
    out = []
    for line in open(trace):
        m = re.match(r"\s*([\d.]+)s \[v\d+\] \S+\s+(\S+)\s+note\s+\d+\s+root\s+\d+.*start (\d+)", line)
        if m:
            out.append((round(float(m.group(1)) * RATE), os.path.basename(m.group(2)), int(m.group(3))))
    return out


def main():
    mid, nki, trace, outdir = sys.argv[1:5]
    os.makedirs(outdir, exist_ok=True)
    scn = os.path.join(outdir, "scenario.json")
    json.dump(scenario(mid), open(scn, "w"))
    wav, js = os.path.join(outdir, "kontra.wav"), os.path.join(outdir, "kontra.json")
    seed = sys.argv[sys.argv.index("--seed") + 1] if "--seed" in sys.argv else "1"
    subprocess.run([KONTRA, "play", nki, "--scenario", scn, "--json", js], check=True, timeout=3600,
                   env=dict(os.environ, KONTRA_INSPECT_WAV=wav, KONTRA_KSP_SEED=seed),
                   stdout=subprocess.DEVNULL)
    r = json.load(open(js))
    r = r.get("result", r)
    print("unhandled_actions:", r.get("unhandled_actions") or "none",
          "| unanswered_pars:", r.get("unanswered_pars") or "none")
    faults = [{k: v for k, v in f.items() if v and k in ("divide_by_zero", "out_of_range", "first_out_of_range")}
              for f in r.get("script_faults", [])]
    print("script faults:", faults)
    k = [(e["frame"], os.path.basename(e["sample"]), int(e.get("position", 0)))
         for e in r["stage_events"] if e.get("type") == "voice" and e.get("event") == "Start"]
    o = ours_starts(trace)
    print(f"voice starts: KONTRA {len(k)}, ours {len(o)}")
    diffs = 0
    for i, (a, b) in enumerate(zip(k, o)):
        if a[1] != b[1] or a[2] != b[2]:
            diffs += 1
            print(f"  #{i}: KONTRA {a}  ours {b}")
    offs = sorted({a[0] - b[0] for a, b in zip(k, o)})
    print(f"file/offset differences: {diffs}; frame offsets KONTRA − ours: {offs}")


if __name__ == "__main__":
    main()
