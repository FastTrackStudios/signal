#!/usr/bin/env python3
"""kontakt_keyrun.py <product> <cmd...>  — runs cmd with {JDX} {HU} {SNPID} substituted; key never printed (redacted from output)."""
import re, sys, subprocess
product, cmd = sys.argv[1], sys.argv[2:]
t = open("/run/media/AudioHaven/Kontakt/NativeAccess.xml", encoding="utf-8", errors="replace").read()
key = None
for p in re.split(r"(?=<Product )", t):
    m = re.search(r"<Name>([^<]*)</Name>", p)
    if m and m.group(1).strip().lower() == product.lower():
        key = {k: re.search(rf"<{k}>([^<]*)</{k}>", p).group(1) for k in ("SNPID", "HU", "JDX")}; break
if not key: sys.exit(f"no key for {product}")
key["SNPID"] = key["SNPID"].lstrip("0") or "0"
args = [a.replace("{JDX}", key["JDX"]).replace("{HU}", key["HU"]).replace("{SNPID}", key["SNPID"]) for a in cmd]
p = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace")
for line in p.stdout:
    sys.stdout.write(line.replace(key["JDX"], "<k>").replace(key["HU"], "<k>"))
sys.exit(p.wait())
