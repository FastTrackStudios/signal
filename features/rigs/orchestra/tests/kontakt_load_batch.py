#!/usr/bin/env python3
"""Capture factory Kontakt chains for a list of NKIs by loading each one on the Kontakt host.

usage: kontakt_load_batch.py <load_list.tsv> <host library root> [--only tag,tag]

`load_list.tsv`: `<tag>\\t<NKI path relative to the library root>` per line.
For each NKI whose chain is not there yet, runs `~/orchestra-ab/bin/kt_load_path.sh` on the host,
which drives Kontakt's own GUI in the REAPER project already open there (KONTAKT menu > Reset multi,
Load…, the NKI's path into the open panel, ⌘S). The saved project's FX chain becomes
target/orchestra-ab/chains/fxchain_<tag>.txt (SHOW 0), the form make_ab_rpp.py and
kontakt_batch.py read. Kontakt only plays plugin states it wrote itself, so there is no way to
generate these without loading (see nkx-extract docs/KONTAKT_STATE.md).

The host's screen is taken over while this runs (Kontakt ignores background clicks). Before
starting: open the project there with `REAPER -newinst -nosplash -cfgfile <a copy of the
user's ini> ~/orchestra-ab/perc_load.rpp` with an instrument loaded and its FX window floating,
and make sure no other agent is driving that screen.

Env: KONTAKT_HOST (ssh target, default the ethernet address with HostKeyAlias voyager),
KT_RPP (default ~/orchestra-ab/perc_load.rpp), KT_WAIT (seconds per load, default 25).
"""
import os, re, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
CHAINS = os.path.join(REPO, "target", "orchestra-ab", "chains")
SSH = ["ssh", "-o", "HostKeyAlias=voyager", "-o", "ConnectTimeout=10",
       os.environ.get("KONTAKT_HOST", "cody@192.168.1.10")]
SCP_HOST = os.environ.get("KONTAKT_HOST", "cody@192.168.1.10")


def sh_quote(s):
    return "'" + s.replace("'", "'\\''") + "'"


def fxchain(rpp_text):
    """The first <FXCHAIN …> block of a project, SHOW 0, as a chain file."""
    i = rpp_text.find("<FXCHAIN")
    if i < 0:
        return None
    ind = len(rpp_text[:i]) - len(rpp_text[:i].rstrip(" "))
    out = []
    for line in rpp_text[i:].split("\n"):
        out.append(line)
        if len(out) > 1 and line.strip() == ">" and len(line) - len(line.lstrip(" ")) == ind:
            break
    blk = re.sub(r"^(\s*)SHOW \d+", r"\1SHOW 0", "\n".join(out), flags=re.M)
    return " " * ind + blk + "\n"


def main():
    args = sys.argv[1:]
    only = None
    if "--only" in args:
        k = args.index("--only"); only = set(args[k + 1].split(",")); del args[k:k + 2]
    lst, host_root = args
    rpp = os.environ.get("KT_RPP", "~/orchestra-ab/perc_load.rpp")
    wait = os.environ.get("KT_WAIT", "25")
    os.makedirs(CHAINS, exist_ok=True)
    done = failed = streak = 0
    last = None
    for line in open(lst):
        parts = line.rstrip("\n").split("\t")
        if len(parts) < 2 or (only and parts[0] not in only):
            continue
        tag, rel = parts[0], parts[1]
        chain = os.path.join(CHAINS, f"fxchain_{tag}.txt")
        if os.path.exists(chain):
            continue
        nki = f"{host_root}/{rel}"
        saved = f"~/orchestra-ab/perc_saved/{tag}.rpp"
        cmd = (f"mkdir -p ~/orchestra-ab/perc_saved && cd ~/orchestra-ab && KT_RPP={rpp} KT_WAIT={wait} "
               f"bin/kt_load_path.sh {sh_quote(nki)} {saved}")
        r = subprocess.run(SSH + [cmd], capture_output=True, text=True, timeout=300)
        if r.returncode != 0:
            print(f"{tag}: load failed: {(r.stderr or r.stdout).strip()[-300:]}", flush=True)
            failed += 1; streak += 1
            if streak >= 3:
                print("3 failures in a row: stopping (look at /tmp/kt_*.png on the host)", flush=True)
                break
            continue
        local = os.path.join(CHAINS, f".{tag}.rpp")
        subprocess.run(["scp", "-q", "-o", "HostKeyAlias=voyager", f"{SCP_HOST}:orchestra-ab/perc_saved/{tag}.rpp",
                        local], check=True, timeout=120)
        blk = fxchain(open(local, errors="replace").read())
        os.remove(local)
        state = re.sub(r"^\s*(FLOATPOS|FXID|WAK|PRESETNAME|LASTSEL|SHOW).*$", "", blk or "", flags=re.M)
        if not blk or state == last:
            print(f"{tag}: the saved chain is the previous one: the load did not happen", flush=True)
            failed += 1; streak += 1
            if streak >= 3:
                print("3 failures in a row: stopping (look at /tmp/kt_*.png on the host)", flush=True)
                break
            continue
        last = state; streak = 0
        open(chain, "w").write(blk)
        done += 1
        print(f"{tag}: chain {len(blk) // 1024} KB", flush=True)
    print(f"done {done}, failed {failed}")


if __name__ == "__main__":
    main()
