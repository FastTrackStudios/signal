#!/usr/bin/env python3
"""Omnisphere experiments: render patch variants through the real plugin and
compare the held note's harmonic spectrum.

usage: exp.py <patch.prt_omn> <name>=<edit>[;<edit>...] ...
  edit forms (applied to the Nth occurrence, default 0):
    TAG.attr=VALUE[@N]        set attribute `attr` on the Nth <TAG ...> element
    TAG.attr=f:FLOAT[@N]      same, value given as a float (hex-encoded)
  e.g. exp.py swellee.prt_omn base= type05='OSC.type=f:0.5'
"""
import math, os, re, struct, subprocess, sys, tempfile
os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")

HARNESS = "/Volumes/dev-drive/daw/target/release/examples/omni_render"
OURS = "/Volumes/dev-drive/signal/target/release/examples/render_patch"
OUT = os.path.dirname(os.path.abspath(__file__))


def hexf(v):
    return struct.pack(">f", v).hex()


def apply(xml, edit):
    m = re.match(r"(\w+)\.(\w+)=(f:)?([^@]*)(?:@(\d+))?$", edit)
    if not m:
        raise SystemExit(f"bad edit {edit}")
    tag, attr, isf, val, nth = m.groups()
    val = hexf(float(val)) if isf else val
    nth = int(nth or 0)
    pos = [mm.start() for mm in re.finditer(r"<" + tag + r"\b", xml)]
    if nth >= len(pos):
        raise SystemExit(f"no <{tag}> #{nth}")
    start = pos[nth]
    end = xml.index(">", start)
    el = xml[start:end]
    if re.search(r"\b" + attr + r'="', el):
        el2 = re.sub(r"\b" + attr + r'="[^"]*"', f'{attr}="{val}"', el, count=1)
    else:
        el2 = el + f' {attr}="{val}"'
    return xml[:start] + el2 + xml[end:]


def wav_mono(path):
    d = open(path, "rb").read()
    i, ch, data = 12, 2, None
    while i < len(d):
        cid, sz = d[i : i + 4], struct.unpack("<I", d[i + 4 : i + 8])[0]
        if cid == b"fmt ":
            ch = struct.unpack("<H", d[i + 10 : i + 12])[0]
        if cid == b"data":
            data = d[i + 8 : i + 8 + sz]
        i += 8 + sz + (sz & 1)
    s = struct.unpack("<%df" % (len(data) // 4), data)
    return [sum(s[k : k + ch]) / ch for k in range(0, len(s), ch)]


def harmonics(x, sr, f0, lo, hi, n=12):
    seg = x[int(lo * sr) : int(hi * sr)]
    out = []
    for h in range(1, n + 1):
        f = f0 * h
        re_ = sum(v * math.cos(2 * math.pi * f * k / sr) for k, v in enumerate(seg))
        im = sum(v * math.sin(2 * math.pi * f * k / sr) for k, v in enumerate(seg))
        out.append(math.hypot(re_, im) / len(seg) * 2)
    return out


def render(patch_xml, name, note, sr=48000, exe=None):
    exe = exe or HARNESS
    with tempfile.NamedTemporaryFile("w", suffix=".prt_omn", delete=False) as f:
        f.write(patch_xml)
        p = f.name
    wav = os.path.join(OUT, f"exp_{name}{'_ours' if exe == OURS else ''}.wav")
    r = subprocess.run(
        [exe, p, wav, "--note", str(note), "--vel", "100", "--hold", "1.5", "--tail", "0.3", "--sr", str(sr)],
        capture_output=True, text=True, timeout=120,
    )
    os.unlink(p)
    if r.returncode != 0:
        raise SystemExit(r.stderr[-2000:])
    return wav


def main():
    patch = sys.argv[1]
    note = int(os.environ.get("NOTE", "48"))
    base = open(patch, "rb").read().decode("utf-8", "replace")
    f0 = 440 * 2 ** ((note - 69) / 12)
    for spec in sys.argv[2:]:
        name, _, edits = spec.partition("=")
        xml = base
        for e in filter(None, edits.split(";")):
            xml = apply(xml, e)
        for tag, exe in (("omni", HARNESS), ("ours", OURS)):
            if tag == "ours" and not os.environ.get("OURS"):
                continue
            wav = render(xml, name, note, exe=exe)
            x = wav_mono(wav)
            hs = harmonics(x, 48000, f0, 0.8, 1.1)
            ref = max(hs[0], 1e-9)
            rms = math.sqrt(sum(v * v for v in x[int(0.4 * 48000) : int(1.4 * 48000)]) / (1.0 * 48000))
            print(f"{name:10s} {tag} rms {rms:.4f}  harm dB: " + " ".join(f"{20*math.log10(max(h,1e-9)/ref):6.1f}" for h in hs))


main()
