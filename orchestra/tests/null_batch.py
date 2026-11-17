#!/usr/bin/env python3
"""Null-test many instruments against their Kontakt renders, fast:

    python3 null_batch.py [--modes "probe legato …"] [--refit] [--force] [--jobs N] <Section|library>...

Sections are patch folder names ("Solo Flute") or a whole library
(`strings`, `woodwinds`, `brass`, `all`); see sections.tsv. Prints one line
per (section, mode) and a per-library summary.

What makes it fast, against null_all.sh / null_suite.sh:

- **One instrument load per section.** Every mode of a section renders in ONE
  `render_css_live --batch` process (the samples decode once; the renders
  run side by side).
- **Unchanged results are reused.** Each (section, mode) records the hash of
  everything its result depends on — the renderer binary, the library spec,
  the instrument's patch folder, the probe MIDI, the Kontakt renders and the
  fitted draws. A matching hash prints the stored result without rendering.
- **Fitted draws are replayed, not refitted.** Kontakt's random draws are
  facts of its render; a fit (fit_draws.py, minutes) runs only when there is
  no stored fit, when `--refit` asks, or when replaying the stored draws
  scores more than REFIT_DB worse than the fit did (something the draws
  depend on changed).
- **Scoring in-process** (null_test.score), no python per call.

Kontakt renders must already exist (null_suite.sh makes them on voyager); a
missing one runs `null_suite.sh <section> <mode>` for that pair, which renders
Kontakt and fits as before.

Env: BIN another render_css_live, RENDER_THREADS per render process.
"""
import struct
import argparse, concurrent.futures as cf, csv, hashlib, json, os, subprocess, sys, tempfile, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import null_test  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "../../../.."))
SPECS = os.path.join(HERE, "../specs")
ROOTS = {
    "strings": "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings",
    "woodwinds": "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Woodwinds",
    "brass": "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Brass",
    "solo-strings": "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Solo Strings",
}
MODES = {
    "strings": "probe legato leg2 marcato marc2 shorts pizz trem rebow attack".split(),
    # pizz / trem / rebow are string articulations: the winds and brass
    # test their own (ext)
    "woodwinds": "probe legato leg2 marcato marc2 shorts attack ext".split(),
    "brass": "probe legato leg2 marcato marc2 shorts attack ext".split(),
    "solo-strings": "probe legato leg2 marcato marc2 shorts pizz trem rebow attack".split(),
}
FLAGS = {
    "probe": [], "legato": ["--legato"], "leg2": ["--legato", "--zone2"], "marcato": ["--marcato"],
    "marc2": ["--marcato", "--overlay"], "shorts": ["--shorts"], "pizz": ["--shorts", "--pizz"],
    "trem": ["--tremolo"], "rebow": ["--tremolo", "--rebow"], "attack": ["--attack"],
    "ext": ["--winds"],
}


def probe_flags(lib, mode):
    return FLAGS[mode] + (["--brass"] if mode == "ext" and lib == "brass" else [])
# replaying stored draws this much worse than their fit scored: refit
REFIT_DB = 1.0
CACHE_VERSION = "1"


def sections():
    rows = {}
    for line in open(os.path.join(HERE, "sections.tsv")):
        if line.startswith("#") or not line.strip():
            continue
        s, lib, tag, tr = line.rstrip("\n").split("\t")
        rows[s] = dict(section=s, lib=lib, tag=tag, tr=int(tr))
    return rows


def draw_files(lib, mode):
    """The replay flags of a fitted mode, as (flag, file) pairs (as null_suite.sh fits them)."""
    if lib == "woodwinds" and mode == "marcato":
        return [("--pitch-envs", "draws_penv.txt")]
    if lib == "woodwinds" and mode == "marc2":
        return [("--rrs", "draws_rr.txt"), ("--tunes", "draws.txt"), ("--pitch-envs", "draws_penv.txt")]
    # brass bodies detune when struck with another key down (held_random_tune_cents)
    if lib == "brass" and mode == "marcato":
        return [("--tunes", "draws.txt")]
    if lib == "brass" and mode == "marc2":
        return [("--rrs", "draws_rr.txt"), ("--tunes", "draws.txt")]
    # brass ext: Mutereps / Rips draw their RRs (and detune), a Taka phrase its hits
    if lib == "brass" and mode == "ext":
        return [("--rrs", "draws_rr.txt"), ("--tunes", "draws.txt")]
    # CSSS draws a loud first note's body variant at random (random(2,4)),
    # where CSS walks a sequence: every mode with loud first notes fits it
    if lib == "solo-strings" and mode in ("probe", "legato"):
        return [("--variants", "draws.txt")]
    if lib == "solo-strings" and mode == "marcato":
        return [("--offsets", "draws.txt")]
    if lib == "solo-strings" and mode == "marc2":
        return [("--rrs", "draws_rr.txt"), ("--offsets", "draws.txt")]
    if lib == "solo-strings" and mode == "attack":
        return [("--attacks", "draws.txt"), ("--variants", "draws_var.txt")]
    fit_mode = "shorts" if lib != "strings" and mode == "marc2" else mode
    if fit_mode not in ("shorts", "pizz", "leg2", "attack"):
        return None
    if mode == "leg2":
        return [("--variants", "draws.txt")]
    if mode == "attack":
        return [("--attacks", "draws.txt")]
    if lib == "strings":
        return [("--tunes", "draws.txt")]
    if lib == "woodwinds":
        return [("--rrs", "draws_rr.txt"), ("--tunes", "draws.txt")]
    return [("--rrs", "draws.txt")]


def wav_complete(path):
    """The WAV's data is all there: its header's data size (a Kontakt render
    or copy cut short keeps the full size in the header)."""
    try:
        with open(path, "rb") as f:
            b = f.read(4096)
        j = b.find(b"data")
        n = struct.unpack("<I", b[j + 4 : j + 8])[0] if j > 0 else 0
        return n > 0 and os.path.getsize(path) - (j + 8) >= n
    except OSError:
        return False


def sha_file(path, h):
    with open(path, "rb") as f:
        while chunk := f.read(1 << 20):
            h.update(chunk)


def stamp(path, h):
    st = os.stat(path)
    h.update(f"{path}:{st.st_size}:{st.st_mtime_ns}".encode())


def first_line(path):
    try:
        with open(path) as f:
            return f.readline().strip()
    except FileNotFoundError:
        return None


class Pair:
    def __init__(self, row, mode):
        self.row, self.mode = row, mode
        self.d = os.path.join(REPO, "target/orchestra-ab/sec", row["tag"], mode)
        self.mid = os.path.join(self.d, f"{mode}.mid")
        self.manifest = os.path.join(self.d, f"{mode}_manifest.tsv")
        self.ours = os.path.join(self.d, f"{mode}_ours.wav")
        self.k1 = os.path.join(self.d, f"{mode}_kontakt_r1.wav")
        self.k2 = os.path.join(self.d, f"{mode}_kontakt_r2.wav")
        self.draws = draw_files(row["lib"], mode)
        self.cache = os.path.join(self.d, "batch_cache.json")
        self._key = None

    def replay_args(self):
        """Replay flags from the stored fit, or None when a fit is missing."""
        args = []
        for flag, name in self.draws or []:
            v = first_line(os.path.join(self.d, name))
            if not v:
                return None
            args += [flag, v]
        return args

    def key(self, bin_hash, lib_hash):
        h = hashlib.sha256(CACHE_VERSION.encode())
        h.update(bin_hash.encode())
        h.update(lib_hash.encode())
        for p in (self.mid, self.k1, self.k2):
            if os.path.exists(p):
                sha_file(p, h) if p == self.mid else stamp(p, h)
        for _, name in self.draws or []:
            h.update((first_line(os.path.join(self.d, name)) or "").encode())
        return h.hexdigest()

    def load_cache(self):
        try:
            return json.load(open(self.cache))
        except (FileNotFoundError, ValueError):
            return {}


def run_suite(section, mode, env):
    """The slow path: null_suite.sh (Kontakt render and/or a fit)."""
    r = subprocess.run([os.path.join(HERE, "null_suite.sh"), section, mode], env=env,
                       capture_output=True, text=True, timeout=3600)
    return r.returncode == 0, r.stdout + r.stderr


def lib_hash():
    h = hashlib.sha256()
    for name in sorted(os.listdir(SPECS)):
        if name.endswith(".styx"):
            sha_file(os.path.join(SPECS, name), h)
    return h.hexdigest()


def patch_hash(row):
    h = hashlib.sha256()
    d = os.path.join(ROOTS[row["lib"]], "_patches", row["section"])
    for name in sorted(os.listdir(d)):
        p = os.path.join(d, name)
        if os.path.isfile(p) and name.endswith(".styx"):
            sha_file(p, h)
    return h.hexdigest()


def gen_probe(pair):
    os.makedirs(pair.d, exist_ok=True)
    subprocess.run([sys.executable, "-I", os.path.join(HERE, "gen_probe.py"), pair.mid, pair.manifest,
                    *probe_flags(pair.row["lib"], pair.mode), f"--transpose={pair.row['tr']}"], check=True, capture_output=True)


def score(pair, fitted):
    rows = list(csv.DictReader(open(pair.manifest), delimiter="\t"))
    ours, k1 = null_test.load(pair.ours), null_test.load(pair.k1)
    # fitted modes are scored against the run whose draws were fitted
    k2 = None if fitted else (null_test.load(pair.k2) if os.path.exists(pair.k2) else None)
    out, summary = null_test.score(rows, ours, k1, k2, show=lambda *_: None)
    json.dump(out, open(os.path.join(pair.d, "null.json"), "w"), indent=1)
    all_line = next((l for l in summary if l.startswith("all")), "")
    med = float(all_line.split("strict median")[1].split("dB")[0]) if all_line else 0.0
    return all_line, med


def section_job(row, modes, args, bin_path, bin_hash, lhash):
    """Every mode of one section. Returns [(mode, line, how)]."""
    env = dict(os.environ, CS_LIBRARY=row["lib"], BIN=bin_path, SKIP_KONTAKT="1")
    phash = patch_hash(row)
    results, render, slow = {}, [], []
    pairs = [Pair(row, m) for m in modes]
    for p in pairs:
        gen_probe(p)
        if not (wav_complete(p.k1) and wav_complete(p.k2)):
            slow.append((p, "kontakt"))
            continue
        k = p.key(bin_hash, lhash + phash)
        c = p.load_cache()
        if not args.force and not args.refit and c.get("key") == k and os.path.exists(p.ours):
            results[p.mode] = (c["line"], "cached")
            continue
        p._key = k
        if p.draws is not None:
            replay = p.replay_args()
            if replay is None or args.refit:
                slow.append((p, "fit"))
                continue
            render.append((p, replay))
        else:
            render.append((p, []))

    if render:
        fd, listing = tempfile.mkstemp(suffix=".tsv")
        with os.fdopen(fd, "w") as f:
            for p, extra in render:
                f.write("\t".join([p.mid, p.ours, " ".join(extra)]) + "\n")
        try:
            subprocess.run([bin_path, "--batch", listing, "--section", row["section"], "--library", row["lib"]],
                           check=True, capture_output=True, text=True, timeout=1800, env=env)
        finally:
            os.unlink(listing)
        for p, _ in render:
            fitted = p.draws is not None
            line, med = score(p, fitted)
            c = p.load_cache()
            if fitted and c.get("fit_median") is not None and med > c["fit_median"] + REFIT_DB:
                # the stored draws no longer explain the render: fit again
                slow.append((p, "refit"))
                continue
            c.update(key=p._key, line=line)
            if fitted and c.get("fit_median") is None:
                c["fit_median"] = med
            json.dump(c, open(p.cache, "w"))
            results[p.mode] = (line, "replayed" if fitted else "rendered")

    for p, why in slow:
        ok, text = run_suite(row["section"], p.mode, env)
        line = next((l for l in reversed(text.splitlines()) if l.startswith("all")), "")
        if not ok or not line:
            results[p.mode] = ("FAILED " + text.strip().splitlines()[-1][:80] if text.strip() else "FAILED", why)
            continue
        med = float(line.split("strict median")[1].split("dB")[0])
        c = {"key": p.key(bin_hash, lhash + phash), "line": line}
        if p.draws is not None:
            c["fit_median"] = med
        json.dump(c, open(p.cache, "w"))
        results[p.mode] = (line, why)
    return [(m, *results[m]) for m in modes if m in results]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("targets", nargs="+")
    ap.add_argument("--modes", help="default: every mode of the instrument's library")
    ap.add_argument("--jobs", type=int, default=6, help="sections side by side")
    ap.add_argument("--refit", action="store_true", help="fit every random draw again")
    ap.add_argument("--force", action="store_true", help="ignore stored results")
    args = ap.parse_args()
    table = sections()
    rows = []
    for t in args.targets:
        if t in table:
            rows.append(table[t])
        elif t == "all" or t in ROOTS:
            rows += [r for r in table.values() if t == "all" or r["lib"] == t]
        else:
            raise SystemExit(f"unknown section or library: {t}")
    bin_path = os.environ.get("BIN") or os.path.join(
        REPO, "target/x86_64-unknown-linux-gnu/desktop-dev/examples/render_css_live")
    h = hashlib.sha256()
    sha_file(bin_path, h)
    bin_hash = h.hexdigest()
    lhash = lib_hash()
    # each render process uses every core; a few sections at once overlap
    # the single-threaded parts (load, scoring)
    os.environ.setdefault("RENDER_THREADS", str(max(2, (os.cpu_count() or 8) // max(1, args.jobs) * 2)))
    t0 = time.time()
    meds = {}
    with cf.ThreadPoolExecutor(args.jobs) as ex:
        futs = {ex.submit(section_job, r, args.modes.split() if args.modes else MODES[r["lib"]], args, bin_path, bin_hash, lhash): r for r in rows}
        for f in cf.as_completed(futs):
            r = futs[f]
            try:
                res = f.result()
            except Exception as e:  # noqa: BLE001 — report and carry on
                print(f"{r['section']:20} FAILED {e}", flush=True)
                continue
            for mode, line, how in res:
                print(f"{r['section']:20} {mode:8} {how:8} {line}", flush=True)
                if line.startswith("all"):
                    meds.setdefault(r["lib"], []).append(float(line.split("strict median")[1].split("dB")[0]))
    for lib, v in meds.items():
        print(f"== {lib}: {len(v)} pairs, mean strict median {sum(v)/len(v):.1f} dB")
    print(f"== {time.time() - t0:.0f} s")


if __name__ == "__main__":
    main()
