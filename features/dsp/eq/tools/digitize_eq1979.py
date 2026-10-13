#!/usr/bin/env python3
"""Digitize EQ1979 frequency response PNG plots into calibration CSV files.

The source plots are reference material only. This script extracts the visible
magenta/pink response trace and maps pixels back onto the plot's logarithmic
frequency axis and dB axis. It depends on ImageMagick's `magick` binary for PNG
decoding so it can stay dependency-free from Python's side.
"""

from __future__ import annotations

import csv
import math
import subprocess
from dataclasses import dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[3]
SOURCE_DIR = REPO_ROOT.parent.parent / "reference" / "eq1979" / "freqresponse"
OUT_DIR = REPO_ROOT / "crates" / "eq-dsp" / "src" / "data" / "hardware_targets"


@dataclass(frozen=True)
class Axis:
    x0: int
    x1: int
    f0: float
    f1: float
    y0: int
    y1: int
    db_top: float
    db_bottom: float


@dataclass(frozen=True)
class Plot:
    source: str
    target: str
    axis: Axis
    stride_px: int = 10


LARGE_AXIS = Axis(
    x0=133,
    x1=1164,
    f0=5.0,
    f1=20_000.0,
    y0=11,
    y1=761,
    db_top=10.0,
    db_bottom=-10.0,
)

HPF_AXIS = Axis(
    x0=28,
    x1=1176,
    f0=50.0,
    f1=20_000.0,
    y0=8,
    y1=772,
    db_top=10.0,
    db_bottom=-30.0,
)

PLOTS = [
    Plot("raw.png", "eq1979_raw.csv", LARGE_AXIS),
    Plot("raw_mid_engaged.png", "eq1979_raw_mid_engaged.csv", LARGE_AXIS),
    Plot("ram_low_engaged.png", "eq1979_raw_low_engaged.csv", LARGE_AXIS),
    Plot("lpf_110Hz_7dB.png", "eq1979_low_110hz_7db.csv", LARGE_AXIS),
    Plot("14dB_1600hz.png", "eq1979_mid_1600hz_14db.csv", LARGE_AXIS),
    Plot("14dB_HS_boost.png", "eq1979_high_shelf_14db.csv", LARGE_AXIS),
    Plot("hpf_160hz.png", "eq1979_hpf_160hz.csv", HPF_AXIS, stride_px=8),
    Plot("hpf_330hz.png", "eq1979_hpf_330hz.csv", HPF_AXIS, stride_px=8),
]


def read_rgb(path: Path) -> tuple[int, int, bytes]:
    size = subprocess.check_output(["magick", "identify", "-format", "%w %h", str(path)], text=True)
    width, height = (int(part) for part in size.split())
    rgb = subprocess.check_output(["magick", str(path), "rgb:-"])
    expected = width * height * 3
    if len(rgb) != expected:
        raise RuntimeError(f"{path} decoded to {len(rgb)} bytes, expected {expected}")
    return width, height, rgb


def is_trace_pixel(rgb: bytes, offset: int) -> bool:
    r, g, b = rgb[offset], rgb[offset + 1], rgb[offset + 2]
    return r >= 165 and b >= 165 and g <= 170 and (r - g) >= 25 and (b - g) >= 25


def pixel_to_freq(x: float, axis: Axis) -> float:
    t = (x - axis.x0) / (axis.x1 - axis.x0)
    return 10.0 ** (math.log10(axis.f0) + t * (math.log10(axis.f1) - math.log10(axis.f0)))


def pixel_to_db(y: float, axis: Axis) -> float:
    t = (y - axis.y0) / (axis.y1 - axis.y0)
    return axis.db_top + t * (axis.db_bottom - axis.db_top)


def digitize(plot: Plot) -> list[tuple[float, float, float]]:
    path = SOURCE_DIR / plot.source
    width, height, rgb = read_rgb(path)
    axis = plot.axis
    rows: list[tuple[float, float, float]] = []

    for x in range(axis.x0, axis.x1 + 1, plot.stride_px):
        ys: list[int] = []
        for y in range(max(0, axis.y0), min(height, axis.y1 + 1)):
            off = (y * width + x) * 3
            if is_trace_pixel(rgb, off):
                ys.append(y)

        if not ys:
            continue

        ys.sort()
        median_y = ys[len(ys) // 2]
        freq = pixel_to_freq(x, axis)
        db = pixel_to_db(median_y, axis)
        weight = 1.0
        rows.append((freq, db, weight))

    # Drop duplicate/near-empty leading or trailing runs caused by clipped plot
    # edges, while preserving steep HPF/LPF tails.
    return [(round(freq, 6), round(db, 6), weight) for freq, db, weight in rows]


def write_csv(path: Path, rows: list[tuple[float, float, float]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(["frequency_hz", "magnitude_db", "weight"])
        writer.writerows(rows)


def main() -> None:
    for plot in PLOTS:
        rows = digitize(plot)
        if len(rows) < 20:
            raise RuntimeError(f"{plot.source} yielded only {len(rows)} points")
        out = OUT_DIR / plot.target
        write_csv(out, rows)
        print(f"{plot.source}: {len(rows)} points -> {out.relative_to(REPO_ROOT)}")


if __name__ == "__main__":
    main()
