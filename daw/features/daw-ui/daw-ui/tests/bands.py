"""Measure a strip's coloured bands by scanning one column of pixels.

The track colour is the only flat, saturated fill in an MCP strip, so the
bands can be found without knowing the layout: walk a column and report every
run of "coloured" rows. Pixels come from `magick ... txt:-` so this needs
nothing but a stock python.
"""
import re, subprocess, sys

def column(path, x, y0, y1):
    out = subprocess.run(
        ["magick", path, "-crop", f"1x{y1-y0}+{x}+{y0}", "+repage", "-depth", "8", "txt:-"],
        capture_output=True, text=True, check=True).stdout
    px = []
    for line in out.splitlines()[1:]:
        m = re.search(r"#([0-9A-Fa-f]{6})", line)
        if m:
            v = int(m.group(1), 16)
            px.append(((v >> 16) & 255, (v >> 8) & 255, v & 255))
    return px

def coloured(p):
    mx, mn = max(p), min(p)
    return mx > 70 and mx - mn > 40           # saturated, not chrome grey

def runs(px, y0):
    out, start = [], None
    for i, p in enumerate(px):
        if coloured(p):
            if start is None:
                start = i
        elif start is not None:
            out.append((y0 + start, i - start)); start = None
    if start is not None:
        out.append((y0 + start, len(px) - start))
    return [r for r in out if r[1] >= 3]

if __name__ == "__main__":
    path, x, y0, y1 = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
    scale = float(sys.argv[5]) if len(sys.argv) > 5 else 1.0
    rs = runs(column(path, x, y0, y1), y0)
    print(f"{path} column x={x} (scale {scale})")
    for (s, h) in rs:
        print(f"  rows {s:5d}..{s+h:5d}  height {h:4d}  = {h/scale:6.1f} @1x")
