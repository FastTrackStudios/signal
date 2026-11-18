#!/usr/bin/env python3
"""Render many Kontakt references in one REAPER process.

    python3 kontakt_batch.py <mode> <tag>:<transpose> [<tag>:<transpose> ...]

One project per call: a track per instrument and run (r1, r2 — two Kontakt
instances, so random draws differ between them), each with the instrument's
factory chain (target/orchestra-ab/chains/fxchain_<tag>.txt) and the mode's
probe MIDI. REAPER renders it once, every track to its own stem
($track.wav), on the Kontakt host; the stems land where null_suite.sh looks
for them (target/orchestra-ab/sec/<tag>/<mode>/<mode>_kontakt_r<n>.wav), so a
`SKIP_KONTAKT=1 null_suite.sh` afterwards scores them without the host.

Instruments whose r2 render is already there are skipped. The render REAPER
runs on its own copy of the user's settings (-cfgfile, see null_suite.sh):
it rewrites its ini, which must never be the user's reaper.ini.

Env: KONTAKT_HOST (default voyager).
"""
import os, re, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
HOST = os.environ.get("KONTAKT_HOST", "voyager")
REAPER = "/Applications/REAPER.app/Contents/MacOS/REAPER"
FLAGS = {
    "probe": [], "legato": ["--legato"], "leg2": ["--legato", "--zone2"],
    "marcato": ["--marcato"], "marc2": ["--marcato", "--overlay"],
    "shorts": ["--shorts"], "pizz": ["--shorts", "--pizz"],
    "trem": ["--tremolo"], "rebow": ["--tremolo", "--rebow"], "attack": ["--attack"],
}


def run(cmd, timeout, check=True):
    return subprocess.run(cmd, timeout=timeout, check=check, capture_output=True, text=True)


def ssh(cmd, timeout=60, check=True):
    return run(["ssh", HOST, cmd], timeout, check)


def track_block(rpp_text, name):
    """The <TRACK> block of a make_ab_rpp.py project, renamed and selected."""
    lines = rpp_text.split("\n")
    a = lines.index("  <TRACK")
    b = max(i for i, l in enumerate(lines) if l == "  >")
    blk = lines[a : b + 1]
    blk[1] = f'    NAME "{name}"'
    blk.insert(2, "    SEL 1")
    return "\n".join(blk)


def tempo_line(head):
    m = re.search(r"^  TEMPO .*$", head, re.M)
    if not m:
        sys.exit("make_ab_rpp.py wrote no TEMPO line")
    return m.group(0)


def main():
    mode, specs = sys.argv[1], sys.argv[2:]
    home = ssh("echo $HOME").stdout.strip()
    if not home:
        sys.exit(f"cannot reach {HOST}")
    rdir = f"{home}/orchestra-ab/batch/{mode}"
    tracks, header, want = [], None, []
    with tempfile.TemporaryDirectory() as tmp:
        for spec in specs:
            tag, tr = spec.split(":")
            d = os.path.join(REPO, "target/orchestra-ab/sec", tag, mode)
            if os.path.exists(os.path.join(d, f"{mode}_kontakt_r2.wav")):
                continue
            os.makedirs(d, exist_ok=True)
            mid = os.path.join(d, f"{mode}.mid")
            run(["python3", "-I", os.path.join(HERE, "gen_probe.py"), mid,
                 os.path.join(d, f"{mode}_manifest.tsv"), *FLAGS[mode], f"--transpose={tr}"], 120)
            chain = os.path.join(REPO, "target/orchestra-ab/chains", f"fxchain_{tag}.txt")
            one = os.path.join(tmp, f"{tag}.rpp")
            run(["python3", "-I", os.path.join(HERE, "make_ab_rpp.py"), chain, mid, "/dev/null", one], 120)
            text = open(one).read()
            head = text.split("  <TRACK")[0]
            tempo = tempo_line(head)
            if header is None:
                header = head
            elif tempo != tempo_line(header):
                sys.exit(f"{tag}: probe tempo differs from the batch's")
            for r in ("r1", "r2"):
                tracks.append(track_block(text, f"{tag}__{r}"))
                want.append((tag, r, d))
        if header is None or not tracks:
            print(f"{mode}: every render is already there")
            return
        header = re.sub(r'^  RENDER_FILE .*$', f'  RENDER_FILE "{rdir}"', header, flags=re.M)
        header = re.sub(r'^  RENDER_PATTERN .*$', '  RENDER_PATTERN "$track"', header, flags=re.M)
        # stems of the selected tracks (every track is selected)
        header = re.sub(r'^  RENDER_STEMS .*$', '  RENDER_STEMS 1', header, flags=re.M)
        rpp = os.path.join(tmp, f"batch_{mode}.rpp")
        open(rpp, "w").write(header + "\n".join(tracks) + "\n>\n")
        ssh(f"rm -rf '{rdir}' && mkdir -p '{rdir}'")
        run(["scp", "-q", rpp, f"{HOST}:{rdir}/batch_{mode}.rpp"], 300)
    cfg = f"{home}/orchestra-ab/cfg/render-batch.ini"
    ssh(f"cp ~/orchestra-ab/cfg/user-reaper.ini '{cfg}'")
    print(f"{mode}: rendering {len(tracks)} tracks in one REAPER", flush=True)
    t0 = time.time()
    # REAPER can return before the render ends: wait for its process to exit
    ssh(f"{REAPER} -newinst -nosplash -cfgfile '{cfg}' -renderproject '{rdir}/batch_{mode}.rpp' "
        f">/dev/null 2>&1; for i in $(seq 1200); do "
        f"pgrep -f '^{REAPER} .*/batch_{mode}[.]rpp' >/dev/null || break; sleep 3; done",
        timeout=3900, check=False)
    print(f"{mode}: rendered in {time.time() - t0:.0f}s", flush=True)
    missing = []
    for tag, r, d in want:
        res = run(["scp", "-q", f"{HOST}:{rdir}/{tag}__{r}.wav", os.path.join(d, f"{mode}_kontakt_{r}.wav")],
                  300, check=False)
        if res.returncode != 0:
            missing.append(f"{tag}__{r}")
    if missing:
        sys.exit(f"{mode}: no stem for {' '.join(missing)}")
    print(f"{mode}: {len(want)} stems fetched")


if __name__ == "__main__":
    main()
