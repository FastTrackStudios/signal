#!/usr/bin/env python3
"""Render every plugin face to an image and index them: the design library.

    scripts/face_gallery.py OUT_DIR [--only SUBSTR] [--dark] [--jobs N]

Walks examples/plugins/<set>/*.frame — each device's faces in the shapes a
host shows (rack, -lane, -tall, -stacked), not the scaled (@x), phone, back
or dark variants (dark with --dark) — and renders each with frame's GPU
effects stage (render_surface --fx gpu, demo feeds, t = 1.3 s) to
OUT_DIR/<set>/<face>.webp. OUT_DIR/index.json lists them by kind (reverb,
delay, saturation, compressor, eq, modulation, motion, drive, amp…), with
whether each is a placeholder (its generator says "placeholder until its
machine is drawn"), its size and its image.

Needs `cargo build --release -p frame-cli --features anyrender --bin
render_surface` (the recipe `just faces-gallery` builds it) and ImageMagick.
"""
import argparse
import concurrent.futures as cf
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PLUGINS = ROOT / "examples" / "plugins"
SOURCES = ROOT / "examples" / "plugin-faces" / "src"
RENDER = ROOT / "target" / "release" / "render_surface"

# Filename words → kind, first match wins.
KINDS = [
    ("verb", "Reverb"), ("reverb", "Reverb"), ("spring", "Reverb"),
    ("dly", "Delay"), ("delay", "Delay"), ("echo", "Delay"),
    ("motion", "Motion"), ("trem", "Motion"), ("rotary", "Motion"), ("vibrato", "Motion"),
    ("mod-", "Modulation"), ("chorus", "Modulation"), ("phaser", "Modulation"), ("flanger", "Modulation"),
    ("sc-eq", "EQ"), ("eq", "EQ"),
    ("comp", "Compressor"), ("fet", "Compressor"), ("opto", "Compressor"), ("vari-mu", "Compressor"),
    ("twin-valve", "Compressor"), ("leveler", "Compressor"),
    ("drive", "Drive"), ("pedal", "Drive"), ("fuzz", "Drive"), ("boost", "Drive"),
    ("amp", "Amp"), ("cab", "Cab"), ("gate", "Gate"), ("tuner", "Utility"),
]
SET_KINDS = {"comp": "Compressor", "saturate": "Saturation", "eq": "EQ",
             "delay": "Delay", "reverb": "Reverb", "modulation": "Modulation"}


def kind_of(set_name: str, stem: str) -> str:
    if set_name in SET_KINDS:
        return SET_KINDS[set_name]
    for word, kind in KINDS:
        if word in stem:
            return kind
    return "Rig"


def placeholders() -> set[str]:
    """The face stems whose generator is a placeholder."""
    out = set()
    for src in SOURCES.rglob("*.rs"):
        text = src.read_text(errors="ignore")
        if "placeholder until its machine is drawn" not in text:
            continue
        out.update(re.findall(r'"(\d+-[a-z0-9-]+)"', text))
    return out


def faces(dark: bool, only: str | None):
    for set_dir in sorted(p for p in PLUGINS.iterdir() if p.is_dir()):
        catalogs = list(set_dir.glob("*.catalog.json"))
        if not catalogs:
            continue
        for frame in sorted(set_dir.glob("*.frame")):
            stem = frame.stem
            if "@" in stem or "-phone" in stem or "-back" in stem:
                continue
            if stem.endswith("-dark") and not dark:
                continue
            if only and only not in f"{set_dir.name}/{stem}":
                continue
            yield set_dir.name, stem, frame, catalogs[0]


def size_of(frame: Path):
    head = frame.read_text(errors="ignore")[:4000]
    m = re.search(r"<Frame [^>]*?\bw=([\d.]+) h=([\d.]+)", head)
    return (float(m.group(1)), float(m.group(2))) if m else None


def render(out: Path, set_name: str, stem: str, frame: Path, catalog: Path):
    dest = out / set_name / f"{stem}.webp"
    dest.parent.mkdir(parents=True, exist_ok=True)
    png = dest.with_suffix(".png")
    cmd = [str(RENDER), "--markup", str(frame), "--catalog", str(catalog), "--out", str(png),
           "--fx", "gpu", "--demo-feeds", "--time", "1.3", "--scale", "1.5", "--level", "-12"]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0 or not png.exists():
        return None, (r.stderr or r.stdout).strip()[-300:]
    subprocess.run(["magick", str(png), "-resize", "1200x>", "-quality", "82", str(dest)], check=True)
    png.unlink()
    return dest, None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--only")
    ap.add_argument("--dark", action="store_true")
    ap.add_argument("--jobs", type=int, default=6)
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    if not RENDER.exists():
        sys.exit(f"no {RENDER}: cargo build --release -p frame-cli --features anyrender --bin render_surface")
    stubs = placeholders()
    todo = list(faces(a.dark, a.only))
    entries, failed = [], []
    with cf.ThreadPoolExecutor(a.jobs) as pool:
        futs = {pool.submit(render, out, *f): f for f in todo}
        for fut in cf.as_completed(futs):
            set_name, stem, frame, _ = futs[fut]
            dest, err = fut.result()
            if dest is None:
                failed.append({"face": f"{set_name}/{stem}", "error": err})
                continue
            base = re.sub(r"-(tall|lane|stacked)$", "", stem.removesuffix("-dark"))
            shape = next((s for s in ("tall", "lane", "stacked") if stem.removesuffix("-dark").endswith("-" + s)), "rack")
            entries.append({
                "set": set_name, "face": stem, "device": base, "shape": shape,
                "kind": kind_of(set_name, stem),
                "placeholder": base in stubs,
                "size": size_of(frame),
                "image": str(dest.relative_to(out)),
            })
    entries.sort(key=lambda e: (e["kind"], e["set"], e["face"]))
    (out / "index.json").write_text(json.dumps({"faces": entries, "failed": failed}, indent=1))
    print(f"{len(entries)} faces rendered, {len(failed)} failed → {out}/index.json")
    for f in failed[:10]:
        print("  failed:", f["face"], f["error"][:160])


if __name__ == "__main__":
    main()
