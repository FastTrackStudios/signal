#!/usr/bin/env python3
"""Render MOJO 2 Kontakt references in one REAPER process (kontakt_batch.py, MOJO's probe).

    python3 mojo_kontakt.py <mode> <tag> [<tag> …]      e.g.  oneshots asx tp

Like kontakt_batch.py: one project, a track per instrument and run (r1, r2:
two Kontakt instances, so random draws differ), each with the instrument's
factory chain (target/orchestra-ab/chains/fxchain_mojo_<tag>.txt, from the
user's saved .RfxChain) and mojo_probe.py's MIDI for <mode>. Stems land in
target/orchestra-ab/sec/mojo_<tag>/<mode>/<mode>_kontakt_r<n>.wav beside
the probe and manifest. The render REAPER runs on a copy of the user's
settings (-cfgfile): it rewrites its ini, which must never be reaper.ini.

`--seed N`: instead, one REAPER per instrument with the KSP Seed CLAP
before Kontakt and KSP_SEED=N, to <mode>_kontakt_seed<N>.wav — Kontakt's
random draws pinned, so KONTRA with KONTRA_KSP_SEED=N draws the same.

Env: MOJO_HOST (default 192.168.1.10, voyager's wired address; the host key
is checked as voyager's). Tags are the spec's section ids (asx, tp, …).
"""
import os, re, subprocess, sys, tempfile, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import kontakt_batch as kb  # noqa: E402

HERE = kb.HERE
REPO = kb.REPO
HOST = os.environ.get("MOJO_HOST", "192.168.1.10")
SSH_OPTS = ["-o", "HostKeyAlias=voyager", "-o", "BatchMode=yes", "-o", "ConnectTimeout=15"]
INSTRUMENT = {
    "asx": "Alto Sax", "bsx": "Baritone Sax", "btb": "Bass Trombone",
    "btbm": "Bass Trombone Muted (Simulated)", "cl": "Clarinet", "flg": "Flugelhorn",
    "flgm": "Flugelhorn Muted (Simulated)", "hn": "French Horn",
    "hnm": "French Horn Muted (Simulated)", "ptp": "Piccolo Trumpet",
    "ptpm": "Piccolo Trumpet Muted (Simulated)", "ssx": "Soprano Sax", "tsx": "Tenor Sax",
    "tb": "Trombone", "tbm": "Trombone Muted", "tp": "Trumpet", "tpm": "Trumpet Muted",
}


def ssh(cmd, timeout=60, check=True):
    return kb.run(["ssh", *SSH_OPTS, HOST, cmd], timeout, check)


def scp(src, dst, timeout=600, check=True):
    return kb.run(["scp", "-q", *SSH_OPTS, src, dst], timeout, check)


SEED_BLOCK = ['<CLAP "CLAP: KSP Seed (FTS)" com.fts.ksp-seed ""', '  CFG 4 760 0 ""', '>']


def with_seed_plugin(track):
    """The KSP Seed CLAP (tests/ksp_seed) inserted before Kontakt in a track."""
    out, done = [], False
    for line in track.split("\n"):
        if not done and line.lstrip().startswith('<VST "VST3i: Kontakt 8'):
            ind = line[: len(line) - len(line.lstrip())]
            out += [ind + b for b in SEED_BLOCK]
            done = True
        out.append(line)
    if not done:
        sys.exit("no Kontakt line to put the seed plugin before")
    return "\n".join(out)


def seeded(mode, tags, seed):
    """One headless REAPER per instrument (the seed plugin pins one random
    state per process), its one track rendered with KSP_SEED=seed, to
    <mode>_kontakt_seed<seed>.wav."""
    home = ssh("echo $HOME").stdout.strip()
    cfg = f"{home}/orchestra-ab/cfg/render-mojo-seed.ini"
    ssh(f"cp ~/orchestra-ab/cfg/user-reaper.ini '{cfg}'")
    for tag in tags:
        d = os.path.join(REPO, "target/orchestra-ab/sec", f"mojo_{tag}", mode)
        out = os.path.join(d, f"{mode}_kontakt_seed{seed}.wav")
        if os.path.exists(out):
            continue
        os.makedirs(d, exist_ok=True)
        mid = os.path.join(d, f"{mode}.mid")
        kb.run(["python3", "-I", os.path.join(HERE, "mojo_probe.py"), mode, INSTRUMENT[tag], mid,
                os.path.join(d, f"{mode}_manifest.tsv")], 120)
        rdir = f"{home}/orchestra-ab/batch/mojo-{mode}-seed{seed}-{tag}"
        with tempfile.TemporaryDirectory() as tmp:
            one = os.path.join(tmp, f"{tag}.rpp")
            chain = os.path.join(REPO, "target/orchestra-ab/chains", f"fxchain_mojo_{tag}.txt")
            kb.run(["python3", "-I", os.path.join(HERE, "make_ab_rpp.py"), chain, mid, "/dev/null", one], 120)
            text = open(one, encoding="latin-1").read()
            head = text.split("  <TRACK")[0]
            head = re.sub(r'^  RENDER_FILE .*$', f'  RENDER_FILE "{rdir}"', head, flags=re.M)
            head = re.sub(r'^  RENDER_PATTERN .*$', '  RENDER_PATTERN "$track"', head, flags=re.M)
            head = re.sub(r'^  RENDER_STEMS .*$', '  RENDER_STEMS 1', head, flags=re.M)
            rpp = os.path.join(tmp, f"seed_{tag}.rpp")
            open(rpp, "w", encoding="latin-1").write(
                head + with_seed_plugin(kb.track_block(text, f"mojo_{tag}")) + "\n>\n")
            ssh(f"rm -rf '{rdir}' && mkdir -p '{rdir}'")
            scp(rpp, f"{HOST}:{rdir}/seed_{tag}.rpp")
        t0 = time.time()
        # macOS has no `timeout`: the wait is bounded here, the ssh call by kb.run's timeout
        ssh(f"KSP_SEED={seed} KSP_SEED_LOG='{rdir}/seed.log' {kb.REAPER} -newinst -nosplash -cfgfile '{cfg}' -renderproject "
            f"'{rdir}/seed_{tag}.rpp' >/dev/null 2>&1; for i in $(seq 600); do "
            f"pgrep -f '^{kb.REAPER} .*/seed_{tag}[.]rpp' >/dev/null || break; sleep 2; done",
            timeout=1500, check=False)
        res = scp(f"{HOST}:{rdir}/mojo_{tag}.wav", out, check=False)
        scp(f"{HOST}:{rdir}/seed.log", os.path.join(d, f"{mode}_kontakt_seed{seed}.log"), check=False)
        print(f"{mode} {tag} seed {seed}: {'fetched' if res.returncode == 0 else 'NO STEM'} "
              f"({time.time() - t0:.0f}s)", flush=True)


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    if "--seed" in sys.argv:
        i = sys.argv.index("--seed")
        seed = int(sys.argv[i + 1])
        args = sys.argv[1:i] + sys.argv[i + 2:]
        return seeded(args[0], args[1:], seed)
    mode, tags = sys.argv[1], sys.argv[2:]
    home = ssh("echo $HOME").stdout.strip()
    rdir = f"{home}/orchestra-ab/batch/mojo-{mode}"
    tracks, header, want = [], None, []
    with tempfile.TemporaryDirectory() as tmp:
        for tag in tags:
            d = os.path.join(REPO, "target/orchestra-ab/sec", f"mojo_{tag}", mode)
            if os.path.exists(os.path.join(d, f"{mode}_kontakt_r2.wav")):
                continue
            os.makedirs(d, exist_ok=True)
            mid = os.path.join(d, f"{mode}.mid")
            kb.run(["python3", "-I", os.path.join(HERE, "mojo_probe.py"), mode, INSTRUMENT[tag], mid,
                    os.path.join(d, f"{mode}_manifest.tsv")], 120)
            chain = os.path.join(REPO, "target/orchestra-ab/chains", f"fxchain_mojo_{tag}.txt")
            one = os.path.join(tmp, f"{tag}.rpp")
            kb.run(["python3", "-I", os.path.join(HERE, "make_ab_rpp.py"), chain, mid, "/dev/null", one], 120)
            text = open(one, encoding="latin-1").read()
            head = text.split("  <TRACK")[0]
            if header is None:
                header = head
            elif kb.tempo_line(head) != kb.tempo_line(header):
                sys.exit(f"{tag}: probe tempo differs from the batch's")
            for r in ("r1", "r2"):
                tracks.append(kb.track_block(text, f"mojo_{tag}__{r}"))
                want.append((tag, r, d))
        if header is None or not tracks:
            print(f"{mode}: every render is already there")
            return
        header = re.sub(r'^  RENDER_FILE .*$', f'  RENDER_FILE "{rdir}"', header, flags=re.M)
        header = re.sub(r'^  RENDER_PATTERN .*$', '  RENDER_PATTERN "$track"', header, flags=re.M)
        header = re.sub(r'^  RENDER_STEMS .*$', '  RENDER_STEMS 1', header, flags=re.M)
        rpp = os.path.join(tmp, f"batch_mojo_{mode}.rpp")
        open(rpp, "w", encoding="latin-1").write(header + "\n".join(tracks) + "\n>\n")
        ssh(f"rm -rf '{rdir}' && mkdir -p '{rdir}'")
        scp(rpp, f"{HOST}:{rdir}/batch_mojo_{mode}.rpp")
    cfg = f"{home}/orchestra-ab/cfg/render-mojo.ini"
    ssh(f"cp ~/orchestra-ab/cfg/user-reaper.ini '{cfg}'")
    print(f"{mode}: rendering {len(tracks)} tracks in one REAPER", flush=True)
    t0 = time.time()
    ssh(f"{kb.REAPER} -newinst -nosplash -cfgfile '{cfg}' -renderproject '{rdir}/batch_mojo_{mode}.rpp' "
        f">/dev/null 2>&1; for i in $(seq 1200); do "
        f"pgrep -f '^{kb.REAPER} .*/batch_mojo_{mode}[.]rpp' >/dev/null || break; sleep 3; done",
        timeout=3900, check=False)
    print(f"{mode}: rendered in {time.time() - t0:.0f}s", flush=True)
    missing = []
    for tag, r, d in want:
        res = scp(f"{HOST}:{rdir}/mojo_{tag}__{r}.wav", os.path.join(d, f"{mode}_kontakt_{r}.wav"), check=False)
        if res.returncode != 0:
            missing.append(f"mojo_{tag}__{r}")
    if missing:
        sys.exit(f"{mode}: no stem for {' '.join(missing)}")
    print(f"{mode}: {len(want)} stems fetched")


if __name__ == "__main__":
    main()
