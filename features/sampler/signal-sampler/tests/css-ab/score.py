#!/usr/bin/env python3
"""CSS A/B scorer — render our engine against a real Kontakt bounce and score it.

The loop this drives:

  1. render the SAME MIDI the Kontakt reference was bounced from, through
     `fts signal pack render-report --midi`, in no-prefire mode (the document
     scheduler fires at note-on so arrivals land late like real CSS);
  2. slice both renders into the manifest's parameter sections;
  3. per section, report the level ratio (ref/ours) and the mean absolute
     envelope difference in dB — the two numbers every round has been scored on;
  4. write an A/B page that plays either render, seeking to any section.

Stdlib only, on purpose: this must run in the dev shell with no pip step.

Usage
-----
    # full loop: render ours, then score
    python3 tests/css-ab/score.py --pack "<...>/1st Violins - Legato - Mix.signalpack"

    # score renders that already exist
    python3 tests/css-ab/score.py --ours out/ours.wav --no-render

    # one section, while iterating on it
    python3 tests/css-ab/score.py --pack ... --sections S10,S13

Reference audio lives outside git (78-320 MB of Kontakt bounces) under
`$CSS_REF_DIR` (default /run/media/AudioHaven/Signal/Reference/CSS). See
README.md for what is in there and how to re-bounce it.
"""

from __future__ import annotations

import argparse
import array
import json
import math
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

HERE = Path(__file__).resolve().parent
CRATE = HERE.parent.parent
REPO = CRATE.parent.parent.parent

DEFAULT_REF_DIR = Path(os.environ.get("CSS_REF_DIR", "/run/media/AudioHaven/Signal/Reference/CSS"))

# Envelope resolution. 10 ms hops matched the earlier rounds' numbers, so
# scoreboards stay comparable across the whole effort.
HOP_MS = 10.0
# Envelope floor. Digital-zero tails otherwise produce meaningless 140 dB
# differences (this bit us on S10 in round 1 — the "near-silent" artifact).
ENV_FLOOR = 1e-4


# ---------------------------------------------------------------- audio in


@dataclass
class Audio:
    rate: int
    samples: array.array  # mono sum, float-ish in [-1, 1] as doubles

    def slice_s(self, start: float, end: float) -> array.array:
        a = max(0, int(start * self.rate))
        b = min(len(self.samples), int(end * self.rate))
        return self.samples[a:b] if b > a else array.array("d")


def parse_riff(path: Path) -> tuple[int, int, int, bool, bytes]:
    """(channels, bits, rate, is_float, data) — `wave` rejects the
    WAVE_FORMAT_EXTENSIBLE float our renderer writes, so parse RIFF directly."""
    with open(path, "rb") as f:
        blob = f.read()
    if blob[:4] != b"RIFF" or blob[8:12] != b"WAVE":
        raise SystemExit(f"{path}: not a RIFF/WAVE file")
    pos, fmt, data = 12, None, None
    while pos + 8 <= len(blob):
        cid = blob[pos : pos + 4]
        size = int.from_bytes(blob[pos + 4 : pos + 8], "little")
        body = blob[pos + 8 : pos + 8 + size]
        if cid == b"fmt ":
            fmt = body
        elif cid == b"data":
            data = body
        pos += 8 + size + (size & 1)
    if fmt is None or data is None:
        raise SystemExit(f"{path}: missing fmt/data chunk")
    tag = int.from_bytes(fmt[0:2], "little")
    ch = int.from_bytes(fmt[2:4], "little")
    rate = int.from_bytes(fmt[4:8], "little")
    bits = int.from_bytes(fmt[14:16], "little")
    if tag == 0xFFFE and len(fmt) >= 26:  # EXTENSIBLE: real tag is the GUID head
        tag = int.from_bytes(fmt[24:26], "little")
    return ch, bits, rate, tag == 3, data


def read_wav(path: Path) -> Audio:
    ch, bits, rate, is_float, raw = parse_riff(path)
    width = bits // 8
    n = len(raw) // (width * ch)
    out = array.array("d", bytes(8 * n))
    if width == 2:
        pcm = array.array("h")
        pcm.frombytes(raw)
        if sys.byteorder == "big":
            pcm.byteswap()
        scale = 1.0 / 32768.0
        for i in range(n):
            acc = 0.0
            base = i * ch
            for c in range(ch):
                acc += pcm[base + c]
            out[i] = acc * scale / ch
    elif width == 3:
        # 24-bit little-endian packed; no array typecode covers it.
        scale = 1.0 / 8388608.0
        stride = 3 * ch
        for i in range(n):
            acc = 0.0
            base = i * stride
            for c in range(ch):
                o = base + 3 * c
                v = raw[o] | (raw[o + 1] << 8) | (raw[o + 2] << 16)
                if v & 0x800000:
                    v -= 0x1000000
                acc += v
            out[i] = acc * scale / ch
    elif width == 4:
        pcm = array.array("f" if is_float else "i")
        pcm.frombytes(raw)
        if sys.byteorder == "big":
            pcm.byteswap()
        scale = 1.0 if pcm.typecode == "f" else 1.0 / 2147483648.0
        for i in range(n):
            acc = 0.0
            base = i * ch
            for c in range(ch):
                acc += pcm[base + c]
            out[i] = acc * scale / ch
    else:
        raise SystemExit(f"{path}: unsupported sample width {width * 8}-bit")
    return Audio(rate, out)


# ------------------------------------------------------------- envelopes


def envelope(a: Audio, start: float, end: float) -> list[float]:
    """RMS envelope in HOP_MS steps over [start, end), floored at ENV_FLOOR."""
    hop = max(1, int(a.rate * HOP_MS / 1000.0))
    seg = a.slice_s(start, end)
    env: list[float] = []
    for i in range(0, len(seg) - hop + 1, hop):
        acc = 0.0
        for j in range(i, i + hop):
            v = seg[j]
            acc += v * v
        env.append(max(math.sqrt(acc / hop), ENV_FLOOR))
    return env


def rms(a: Audio, start: float, end: float) -> float:
    seg = a.slice_s(start, end)
    if not seg:
        return 0.0
    acc = 0.0
    for v in seg:
        acc += v * v
    return math.sqrt(acc / len(seg))


@dataclass
class SectionScore:
    name: str
    level_ratio: float  # ref / ours, linear (1.0 = same loudness)
    mean_abs_db: float  # mean |20*log10(ref/ours)| over the envelope
    shape_db: float  # the same, after gain-matching ours to ref
    max_abs_db: float
    ref_rms_db: float
    ours_rms_db: float

    def bad(self) -> bool:
        return self.shape_db >= 4.0 or not (0.8 <= self.level_ratio <= 1.25)

    def row(self) -> str:
        return (
            f"{'!!' if self.bad() else '  '} {self.name:24s} lvl {self.level_ratio:6.2f}  "
            f"shape {self.shape_db:6.2f} dB  raw {self.mean_abs_db:6.2f} dB  "
            f"max {self.max_abs_db:7.2f} dB  ref {self.ref_rms_db:7.2f}  ours {self.ours_rms_db:7.2f}"
        )


def score_section(ref: Audio, ours: Audio, start: float, end: float, name: str) -> SectionScore:
    """Two independent numbers, deliberately kept apart.

    `level_ratio` is loudness — a single gain constant, fixed by a trim.
    `shape_db` is contour — the envelope difference AFTER that gain is matched
    out, i.e. what is left when loudness is not the problem. Rounds 1-7 scored
    on the un-normalized `mean_abs_db`, where a section could look bad purely
    because of a trim, or look fine while its contour was wrong in
    compensating directions. Both are reported; `shape_db` is what a fix to
    the legato *model* should move.
    """
    er, eo = envelope(ref, start, end), envelope(ours, start, end)
    n = min(len(er), len(eo))
    diffs = [20.0 * math.log10(er[i] / eo[i]) for i in range(n)]
    rr, ro = rms(ref, start, end), rms(ours, start, end)
    ratio = (rr / ro) if ro > 0 else float("inf")
    offset = 20.0 * math.log10(ratio) if ro > 0 else 0.0
    return SectionScore(
        name=name,
        level_ratio=ratio,
        mean_abs_db=(sum(abs(d) for d in diffs) / len(diffs)) if diffs else 0.0,
        shape_db=(sum(abs(d - offset) for d in diffs) / len(diffs)) if diffs else 0.0,
        max_abs_db=max((abs(d) for d in diffs), default=0.0),
        ref_rms_db=20.0 * math.log10(max(rr, ENV_FLOOR)),
        ours_rms_db=20.0 * math.log10(max(ro, ENV_FLOOR)),
    )


# ----------------------------------------------------------------- render


def render_ours(fts: Path, pack: Path, midi: Path, out_dir: Path, manifest: dict) -> Path:
    """Render the manifest's MIDI through our engine. Returns the wav path."""
    out_dir.mkdir(parents=True, exist_ok=True)
    html, wav = out_dir / "ours.html", out_dir / "ours.wav"
    cmd = [
        str(fts), "signal", "pack", "render-report", str(pack),
        "--midi", str(midi),
        "--notes", "",
        "--cc1", str(manifest.get("cc1", 80)),
        "--cc2", str(manifest.get("cc2", 0)),
        "--bpm", str(manifest.get("bpm", 120.0)),
        "--pure",
        "--label", "OURS",
        "--out", str(html),
        "--wav", str(wav),
    ]
    env = dict(os.environ)
    # Fire at note-on, CSS-style: match the reference BEFORE prefire shifting
    # is re-enabled. Without this every arrival is early by the mode delay and
    # the envelope diff is meaningless.
    env["SIGNAL_NO_PREFIRE"] = "1"
    print("$ SIGNAL_NO_PREFIRE=1 " + " ".join(cmd), file=sys.stderr)
    subprocess.run(cmd, check=True, env=env)
    return wav


def render_ref_report(fts: Path, pack: Path, ref_wav: Path, out_dir: Path, manifest: dict) -> Path:
    """Wrap the Kontakt reference in a report page (waveform + beat grid)."""
    html = out_dir / "ref.html"
    cmd = [
        str(fts), "signal", "pack", "render-report", str(pack),
        "--notes", "",
        "--audio-in", str(ref_wav),
        "--bpm", str(manifest.get("bpm", 120.0)),
        "--label", "CSS REFERENCE",
        "--out", str(html),
    ]
    subprocess.run(cmd, check=True)
    return html


# ------------------------------------------------------------------- page

AB_PAGE = """<!doctype html>
<meta charset="utf-8"><title>CSS A/B — {title}</title>
<style>
 body{{background:#14161a;color:#e8e8ea;font:14px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;margin:0;padding:24px}}
 h1{{font-size:16px;letter-spacing:.08em;text-transform:uppercase;color:#9aa4b2;margin:0 0 16px}}
 .src{{display:flex;gap:8px;margin-bottom:16px}}
 button{{background:#22262e;color:#e8e8ea;border:1px solid #333a45;border-radius:6px;padding:6px 12px;cursor:pointer;font:inherit}}
 button.on{{background:#3b82f6;border-color:#3b82f6;color:#fff}}
 table{{border-collapse:collapse;width:100%}}
 td,th{{text-align:left;padding:4px 10px;border-bottom:1px solid #262b33;white-space:nowrap}}
 th{{color:#9aa4b2;font-weight:500}}
 td.n{{text-align:right;font-variant-numeric:tabular-nums}}
 tr.bad td{{color:#f59e0b}}
 audio{{width:100%;margin-bottom:16px}}
</style>
<h1>CSS A/B — {title}</h1>
<div class="src">
  <button id="bref" class="on">Reference (Kontakt)</button>
  <button id="bours">Ours</button>
  <span style="color:#6b7280;padding:6px 0">— click a section to play it</span>
</div>
<audio id="a" controls src="{ref_rel}"></audio>
<table><thead><tr><th>section</th><th>t</th><th class="n">lvl</th><th class="n">shape dB</th><th class="n">max dB</th></tr></thead>
<tbody>{rows}</tbody></table>
<script>
const REF={ref_json}, OURS={ours_json};
const a=document.getElementById('a');
let src='ref';
function pick(s){{const t=a.currentTime;src=s;a.src=(s==='ref')?REF:OURS;a.currentTime=t;a.play();
  bref.classList.toggle('on',s==='ref');bours.classList.toggle('on',s==='ours');}}
bref.onclick=()=>pick('ref');bours.onclick=()=>pick('ours');
for(const el of document.querySelectorAll('tr[data-t]'))
  el.onclick=()=>{{a.currentTime=parseFloat(el.dataset.t);a.play();}};
</script>
"""


def write_ab_page(path: Path, title: str, ref: Path, ours: Path, scores: list[SectionScore], sections) -> None:
    rows = []
    for sc, (name, start, end) in zip(scores, sections):
        bad = "" if not sc.bad() else ' class="bad"'
        rows.append(
            f'<tr{bad} data-t="{start:.3f}"><td>{name}</td><td>{start:.1f}–{end:.1f}s</td>'
            f'<td class="n">{sc.level_ratio:.2f}</td><td class="n">{sc.shape_db:.2f}</td>'
            f'<td class="n">{sc.max_abs_db:.1f}</td></tr>'
        )
    path.write_text(
        AB_PAGE.format(
            title=title,
            rows="".join(rows),
            ref_rel=os.path.relpath(ref, path.parent),
            ref_json=json.dumps(os.path.relpath(ref, path.parent)),
            ours_json=json.dumps(os.path.relpath(ours, path.parent)),
        )
    )


# ------------------------------------------------------------------- main


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--manifest", type=Path, default=CRATE / "scripts" / "css-param-test.manifest.json")
    ap.add_argument("--ref", type=Path, default=DEFAULT_REF_DIR / "CSS Param Test.wav")
    ap.add_argument("--midi", type=Path, default=DEFAULT_REF_DIR / "CSS-Param-Test.mid")
    ap.add_argument("--pack", type=Path, help="signalpack to render (required unless --no-render)")
    ap.add_argument("--ours", type=Path, help="pre-rendered wav to score instead of rendering")
    ap.add_argument("--no-render", action="store_true", help="score existing renders only")
    ap.add_argument("--out", type=Path, default=REPO / "scratch" / "css-ab")
    ap.add_argument("--fts", type=Path, default=REPO / "target" / "debug" / "fts")
    ap.add_argument("--sections", help="comma-separated section prefixes, e.g. S10,S13")
    ap.add_argument("--json", type=Path, help="write the scoreboard as JSON here")
    args = ap.parse_args()

    manifest = json.loads(args.manifest.read_text())
    sections = [(s["name"], float(s["start_s"]), float(s["end_s"])) for s in manifest["sections"]]
    if args.sections:
        want = tuple(x.strip() for x in args.sections.split(",") if x.strip())
        sections = [s for s in sections if s[0].startswith(want)]
        if not sections:
            raise SystemExit(f"no sections match {args.sections}")

    args.out.mkdir(parents=True, exist_ok=True)
    if args.ours and args.no_render:
        ours_wav = args.ours
    elif args.no_render:
        ours_wav = args.out / "ours.wav"
    else:
        if not args.pack:
            raise SystemExit("--pack is required unless --no-render")
        if not args.fts.exists():
            raise SystemExit(f"{args.fts} not built — cargo build -p fts-cli --bin fts")
        ours_wav = render_ours(args.fts, args.pack, args.midi, args.out, manifest)

    for p in (args.ref, ours_wav):
        if not p.exists():
            raise SystemExit(f"missing render: {p}")

    print(f"ref  {args.ref}\nours {ours_wav}\n", file=sys.stderr)
    ref_a, ours_a = read_wav(args.ref), read_wav(ours_wav)

    scores = [score_section(ref_a, ours_a, st, en, name) for name, st, en in sections]
    for sc in scores:
        print(sc.row())
    overall = sum(s.mean_abs_db for s in scores) / len(scores)
    shape = sum(s.shape_db for s in scores) / len(scores)
    print(
        f"\n   OVERALL shape {shape:.2f} dB   raw {overall:.2f} dB"
        f"   over {len(scores)} sections"
    )

    page = args.out / "ab.html"
    write_ab_page(page, args.manifest.stem, args.ref, ours_wav, scores, sections)
    print(f"   A/B page {page}", file=sys.stderr)

    if args.json:
        args.json.write_text(
            json.dumps(
                {"overall_shape_db": shape, "overall_mean_abs_db": overall, "sections": [vars(s) for s in scores]},
                indent=1,
            )
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
