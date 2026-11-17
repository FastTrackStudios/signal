#!/usr/bin/env python3
"""Decode Kontakt NKIs with the Native Access key kept in this process.
usage: kontakt_decode.py <product> <out_root> <nki_root> [nki_rel ...]   (no rels = all .nki under root)
Writes <out_root>/<rel without .nki>/ ; prints one status line per NKI (no keys)."""
import os, re, sys, subprocess, pathlib, concurrent.futures as cf
NKI = os.environ.get("NKI_BIN", "/run/media/Development/FastTrackStudio-legacy/nkx-extract/target/release/nki")
product, out_root, nki_root = sys.argv[1], pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
rels = sys.argv[4:] or sorted(str(p.relative_to(nki_root)) for p in nki_root.rglob("*.nki") if not p.name.startswith("._"))
t = open("/run/media/AudioHaven/Kontakt/NativeAccess.xml", encoding="utf-8", errors="replace").read()
key = None
for p in re.split(r"(?=<Product )", t):
    m = re.search(r"<Name>([^<]*)</Name>", p)
    if m and m.group(1).strip().lower() == product.lower():
        key = {k: re.search(rf"<{k}>([^<]*)</{k}>", p).group(1) for k in ("SNPID", "HU", "JDX")}
        break
del t
if not key: sys.exit(f"no key for {product}")
def one(rel):
    od = out_root / rel[:-4]
    od.mkdir(parents=True, exist_ok=True)
    try:
        r = subprocess.run([NKI, str(nki_root / rel), "--jdx", key["JDX"], "--hu", key["HU"], "--snpid", key["SNPID"].lstrip("0") or "0",
                            "--zones", "--script", "--persist", "--out-dir", str(od)], capture_output=True, text=True, timeout=600)
        err = (r.stderr or "").replace(key["JDX"], "<k>").replace(key["HU"], "<k>").strip().splitlines()
        (od / "decode.log").write_text("\n".join(err) + "\n")
        z = od / "zones.tsv"
        nz = sum(1 for _ in open(z)) - 1 if z.exists() else -1
        ks = sorted(x.name for x in od.glob("script_*.ksp"))
        return f"{r.returncode}\t{nz}\t{len(ks)}\t{rel}\t{err[-1][:120] if err else ''}"
    except Exception as e:
        return f"EXC\t-1\t0\t{rel}\t{type(e).__name__}"
with cf.ThreadPoolExecutor(int(os.environ.get("JOBS", "8"))) as ex:
    for line in ex.map(one, rels):
        print(line, flush=True)
