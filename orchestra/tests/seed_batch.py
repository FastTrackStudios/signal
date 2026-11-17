#!/usr/bin/env python3
"""Render many seeded Kontakt references in ONE headless REAPER on the Kontakt host.

    python3 seed_batch.py [--name NAME] [--seed N] [--jobs FILE] [--keep] [<tag>:<mode>[:<seed>] ...]

Each job (instrument tag, probe mode, Kontakt chain, probe MIDI, seed) is a
track of one project: the chain's Kontakt with the KSP Seed CLAP before it,
the MIDI as the track's item. REAPER renders every track to its own stem
(RENDER_STEMS, `$track` pattern); each stem is cut to the length its job
renders alone (item + 1 s tail) and lands at

    <ab>/sec/<tag>/<mode>/<mode>_seed_<NAME>.wav    (NAME default: the seed)

where <ab> is target/orchestra-ab (env ORCH_AB). A stem is sample-identical
to the job rendered alone with seed_renders.sh (the KSP Seed plugin keeps one
generator state per track, seeded from the track name `<...>@<seed>`).

Jobs:
  <tag>:<mode>[:<seed>]  chain <ab>/chains/fxchain_<tag>.txt, MIDI
                         <ab>/sec/<tag>/<mode>/<mode>.mid (gen_probe.py's, as
                         null_suite.sh wrote it), seed --seed (default 777)
  --jobs FILE            one job per line: tag mode chain mid seed (tab or
                         space separated; '-' for the default chain / MIDI /
                         seed; '#' comments)

Every job must share the probe tempo (one project tempo). The render REAPER
runs on its own copy of the user's settings (-cfgfile
~/orchestra-ab/cfg/seedbatch.ini, see null_suite.sh) and only that process
is ever killed (on timeout). --rpp-only PATH writes the project and stops.

Env: KONTAKT_HOST (default cody@192.168.1.10, voyager on the direct link),
KONTAKT_SSH_OPTS (default "-o HostKeyAlias=voyager -o BatchMode=yes"),
ORCH_AB, SEED_BATCH_TIMEOUT (s, 3600).
"""
import argparse, os, re, shlex, subprocess, sys, tempfile, time, wave

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
AB = os.environ.get("ORCH_AB", os.path.join(REPO, "target", "orchestra-ab"))
# voyager over the direct ethernet link (the `voyager` alias goes over the
# home LAN); HostKeyAlias keeps its known_hosts entry
HOST = os.environ.get("KONTAKT_HOST", "cody@192.168.1.10")
SSH_OPTS = os.environ.get("KONTAKT_SSH_OPTS", "-o HostKeyAlias=voyager -o BatchMode=yes").split()
REAPER = "/Applications/REAPER.app/Contents/MacOS/REAPER"
SR = 48000
TAIL_FRAMES = SR  # make_ab_rpp.py's projects render the item plus a 1 s tail
SEED_CLAP = ['<CLAP "CLAP: KSP Seed (FTS)" com.fts.ksp-seed ""', '  CFG 4 760 0 ""', ">"]


def run(cmd, timeout, check=True):
    return subprocess.run(cmd, timeout=timeout, check=check, capture_output=True, text=True)


def ssh(cmd, timeout=60, check=True):
    return run(["ssh", *SSH_OPTS, HOST, cmd], timeout, check)


class Job:
    def __init__(self, tag, mode, chain, mid, seed):
        self.tag, self.mode, self.chain, self.mid, self.seed = tag, mode, chain, mid, seed
        self.track = f"{tag}__{mode}@{seed}"  # the KSP Seed plugin reads the seed after '@'
        self.dir = os.path.join(AB, "sec", tag, mode)
        self.frames = None

    def out(self, name):
        return os.path.join(self.dir, f"{self.mode}_seed_{name}.wav")


def parse_jobs(args):
    jobs = []

    def add(tag, mode, chain="-", mid="-", seed="-"):
        chain = os.path.join(AB, "chains", f"fxchain_{tag}.txt") if chain == "-" else chain
        mid = os.path.join(AB, "sec", tag, mode, f"{mode}.mid") if mid == "-" else mid
        seed = args.seed if seed == "-" else int(seed)
        for p in (chain, mid):
            if not os.path.isfile(p):
                sys.exit(f"{tag}:{mode}: missing {p}")
        jobs.append(Job(tag, mode, chain, mid, seed))

    if args.jobs:
        for line in open(args.jobs):
            line = line.split("#")[0].strip()
            if line:
                add(*line.split())
    for spec in args.specs:
        parts = spec.split(":")
        if len(parts) not in (2, 3):
            sys.exit(f"bad job {spec!r}: <tag>:<mode>[:<seed>]")
        add(parts[0], parts[1], seed=parts[2] if len(parts) == 3 else "-")
    names = [j.track for j in jobs]
    dup = {n for n in names if names.count(n) > 1}
    if dup:
        sys.exit(f"duplicate jobs: {' '.join(sorted(dup))}")
    if not jobs:
        sys.exit("no jobs")
    return jobs


def track_block(rpp_text, job):
    """The <TRACK> of a make_ab_rpp.py project: named for the job, selected (a
    stem), the KSP Seed CLAP inserted right before Kontakt."""
    lines = rpp_text.split("\n")
    a = lines.index("  <TRACK")
    b = max(i for i, l in enumerate(lines) if l == "  >")
    blk = lines[a : b + 1]
    blk[1] = f'    NAME "{job.track}"'
    blk.insert(2, "    SEL 1")
    k = next((i for i, l in enumerate(blk) if l.lstrip().startswith('<VST "VST3i: Kontakt')), None)
    if k is None:
        sys.exit(f"{job.chain}: no Kontakt VST3 in the chain")
    ind = blk[k][: len(blk[k]) - len(blk[k].lstrip())]
    blk[k:k] = [ind + l for l in SEED_CLAP]
    length = float(re.search(r"^      LENGTH ([\d.]+)$", "\n".join(blk), re.M).group(1))
    job.frames = round(length * SR) + TAIL_FRAMES
    return "\n".join(blk)


def build_project(jobs, render_dir, tmp):
    header, tracks = None, []
    for j in jobs:
        one = os.path.join(tmp, f"{j.tag}__{j.mode}.rpp")
        run(["python3", "-I", os.path.join(HERE, "make_ab_rpp.py"), j.chain, j.mid, "/dev/null", one], 120)
        text = open(one).read()
        head = text.split("  <TRACK")[0]
        if header is None:
            header = head
        elif re.search(r"^  TEMPO .*$", head, re.M).group(0) != re.search(r"^  TEMPO .*$", header, re.M).group(0):
            sys.exit(f"{j.tag}:{j.mode}: probe tempo differs from the batch's (one project tempo)")
        tracks.append(track_block(text, j))
    header = re.sub(r"^  RENDER_FILE .*$", f'  RENDER_FILE "{render_dir}"', header, flags=re.M)
    header = re.sub(r"^  RENDER_PATTERN .*$", '  RENDER_PATTERN "$track"', header, flags=re.M)
    # stems of the selected tracks (every track is selected)
    header = re.sub(r"^  RENDER_STEMS .*$", "  RENDER_STEMS 1", header, flags=re.M)
    return header + "\n".join(tracks) + "\n>\n"


def cut_wav(src, dst, frames):
    """dst = the first `frames` frames of src (a stem runs to the longest job's end)."""
    r = wave.open(src, "rb")
    if r.getframerate() != SR or r.getnframes() < frames:
        sys.exit(f"{src}: {r.getnframes()} frames at {r.getframerate()} Hz, want >= {frames} at {SR}")
    data = r.readframes(frames)
    params = r.getparams()
    r.close()
    tmp = dst + ".part"
    w = wave.open(tmp, "wb")
    w.setparams(params)
    w.writeframes(data)
    w.close()
    os.replace(tmp, dst)


def start_and_wait(rdir, rpp, cfg, env, label):
    """Run one headless REAPER on the host until it exits; returns seconds.
    Started detached; its PID is the only one ever waited on or killed."""
    t0 = time.time()
    pid = ssh(f"cd {shlex.quote(rdir)} && env -u KSP_SEED {env} "
              f"nohup {REAPER} -newinst -nosplash -cfgfile {shlex.quote(cfg)} "
              f"-renderproject {shlex.quote(rpp)} >/dev/null 2>&1 & echo $!").stdout.strip()
    if not pid.isdigit():
        sys.exit(f"{label}: REAPER did not start: {pid!r}")
    limit = int(os.environ.get("SEED_BATCH_TIMEOUT", "3600"))
    while ssh(f"kill -0 {pid} 2>/dev/null && echo up", check=False).stdout.strip() == "up":
        if time.time() - t0 > limit:
            ssh(f"kill {pid}", check=False)
            sys.exit(f"{label}: render still running after {limit}s, killed REAPER {pid}")
        time.sleep(2)
    return time.time() - t0


def host_home():
    home = ssh("echo $HOME").stdout.strip()
    if not home:
        sys.exit(f"cannot reach {HOST}")
    return home


def render_singles(jobs, args):
    """The old way, for comparison: each job in a REAPER of its own (KSP_SEED
    in the environment, the track as make_ab_rpp.py names it), as seed_renders.sh."""
    home = host_home()
    rdir = f"{home}/orchestra-ab/batch/seed-{args.name}"
    cfg = f"{home}/orchestra-ab/cfg/seedbatch.ini"
    ssh(f"rm -rf {shlex.quote(rdir)} && mkdir -p {shlex.quote(rdir)} && "
        f"cp ~/orchestra-ab/cfg/user-reaper.ini {shlex.quote(cfg)}")
    total = 0.0
    with tempfile.TemporaryDirectory() as tmp:
        for j in jobs:
            one = os.path.join(tmp, "one.rpp")
            out = f"{rdir}/{j.tag}__{j.mode}.wav"
            run(["python3", "-I", os.path.join(HERE, "make_ab_rpp.py"), j.chain, j.mid, out, one], 120)
            text = open(one).read()
            k = text.index('      <VST "VST3i: Kontakt')
            text = text[:k] + "".join("      " + l + "\n" for l in SEED_CLAP) + text[k:]
            open(one, "w").write(text)
            run(["scp", "-q", *SSH_OPTS, one, f"{HOST}:{rdir}/{j.tag}__{j.mode}.rpp"], 600)
            secs = start_and_wait(rdir, f"{rdir}/{j.tag}__{j.mode}.rpp", cfg,
                                  f"KSP_SEED={j.seed} KSP_SEED_LOG={shlex.quote(f'{rdir}/{j.tag}__{j.mode}.log')}", j.track)
            total += secs
            print(f"{j.track}: rendered alone in {secs:.1f}s", flush=True)
            got = os.path.join(tmp, "got.wav")
            if run(["scp", "-q", *SSH_OPTS, f"{HOST}:{out}", got], 900, check=False).returncode:
                sys.exit(f"{j.track}: no render")
            os.makedirs(j.dir, exist_ok=True)
            cut_wav(got, j.out(args.name), round(float(re.search(r"^      LENGTH ([\d.]+)$", text, re.M).group(1)) * SR) + TAIL_FRAMES)
            ssh(f"rm -f {shlex.quote(out)}", check=False)
    print(f"{args.name}: {len(jobs)} single renders in {total:.1f}s")


def render(jobs, args):
    home = host_home()
    rdir = f"{home}/orchestra-ab/batch/seed-{args.name}"
    with tempfile.TemporaryDirectory() as tmp:
        rpp = os.path.join(tmp, "seed_batch.rpp")
        open(rpp, "w").write(build_project(jobs, rdir, tmp))
        ssh(f"rm -rf {shlex.quote(rdir)} && mkdir -p {shlex.quote(rdir)}")
        run(["scp", "-q", *SSH_OPTS, rpp, f"{HOST}:{rdir}/seed_batch.rpp"], 600)
    cfg = f"{home}/orchestra-ab/cfg/seedbatch.ini"
    ssh(f"cp ~/orchestra-ab/cfg/user-reaper.ini {shlex.quote(cfg)}")
    print(f"{args.name}: rendering {len(jobs)} tracks in one REAPER", flush=True)
    secs = start_and_wait(rdir, f"{rdir}/seed_batch.rpp", cfg,
                          f"KSP_SEED_LOG={shlex.quote(rdir + '/seed.log')}", args.name)
    print(f"{args.name}: rendered in {secs:.1f}s", flush=True)
    with tempfile.TemporaryDirectory() as tmp:
        missing = []
        for j in jobs:
            stem = os.path.join(tmp, f"{j.track}.wav")
            if run(["scp", "-q", *SSH_OPTS, f"{HOST}:{rdir}/{j.track}.wav", stem], 900, check=False).returncode:
                missing.append(j.track)
                continue
            os.makedirs(j.dir, exist_ok=True)
            cut_wav(stem, j.out(args.name), j.frames)
            os.remove(stem)
    os.makedirs(os.path.join(AB, "batch"), exist_ok=True)
    run(["scp", "-q", *SSH_OPTS, f"{HOST}:{rdir}/seed.log", os.path.join(AB, "batch", f"seed-{args.name}.log")], 300, check=False)
    if not args.keep:
        # the host's disk is small: stems are fetched, the project and log stay
        ssh(f"rm -f {shlex.quote(rdir)}/*.wav", check=False)
    if missing:
        sys.exit(f"{args.name}: no stem for {' '.join(missing)}")
    print(f"{args.name}: {len(jobs)} stems -> <ab>/sec/<tag>/<mode>/<mode>_seed_{args.name}.wav "
          f"(log {AB}/batch/seed-{args.name}.log)")
    return secs


def compare(jobs, args):
    """<mode>_seed_<NAME>.wav against <mode>_seed_<REF>.wav: frames, PCM bytes,
    and (numpy) the null — residual energy over the reference's, in dB."""
    try:
        import numpy as np
        sys.path.insert(0, HERE)
        import null_test
    except ImportError:
        np = None
    bad = 0
    for j in jobs:
        a, b = j.out(args.name), j.out(args.compare)
        ra, rb = wave.open(a, "rb"), wave.open(b, "rb")
        na, nb = ra.getnframes(), rb.getnframes()
        same_fmt = ra.getparams()[:3] == rb.getparams()[:3]
        da, db_ = ra.readframes(na), rb.readframes(nb)
        ra.close(), rb.close()
        line = f"{j.track:28} frames {na}/{nb}"
        if same_fmt and da == db_:
            line += "  PCM identical (null -inf dB)"
        else:
            bad += 1
            if np is not None:
                x, y = null_test.load(a), null_test.load(b)
                n = min(len(x), len(y))
                r = float(np.sum((x[:n] - y[:n]) ** 2)) / max(float(np.sum(y[:n] ** 2)), 1e-30)
                line += f"  DIFFERS: null {10 * np.log10(max(r, 1e-30)):.1f} dB"
            else:
                line += "  DIFFERS"
        print(line)
    print(f"{len(jobs) - bad}/{len(jobs)} identical to <mode>_seed_{args.compare}.wav")
    return bad


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("specs", nargs="*")
    ap.add_argument("--jobs")
    ap.add_argument("--seed", type=int, default=777)
    ap.add_argument("--name", help="output <mode>_seed_<NAME>.wav (default: the seed)")
    ap.add_argument("--keep", action="store_true", help="keep the stems on the host")
    ap.add_argument("--rpp-only", metavar="PATH")
    ap.add_argument("--compare", metavar="REF",
                    help="render nothing: compare <mode>_seed_<NAME>.wav with <mode>_seed_<REF>.wav")
    ap.add_argument("--singles", action="store_true",
                    help="render each job in a REAPER of its own instead (the old way; for timing and checks)")
    args = ap.parse_args()
    if not args.name:
        args.name = str(args.seed)
    if not re.fullmatch(r"[A-Za-z0-9_.-]+", args.name):
        sys.exit("--name: letters, digits, _ . - only")
    jobs = parse_jobs(args)
    if args.rpp_only:
        with tempfile.TemporaryDirectory() as tmp:
            open(args.rpp_only, "w").write(build_project(jobs, "/tmp/seed_batch", tmp))
        print(f"{len(jobs)} tracks -> {args.rpp_only}")
        return
    if args.compare:
        sys.exit(1 if compare(jobs, args) else 0)
    if args.singles:
        render_singles(jobs, args)
    else:
        render(jobs, args)


if __name__ == "__main__":
    main()
