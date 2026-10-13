#!/usr/bin/env python3
"""A FET limiting amplifier face (after the 1176) in frame markup.

Everything is plain frame nodes: the knobs, dot scales, push-button ratio bank,
jewel lamp and VU meter are drawn, then turned into controls by attaching
`control` data whose slots point at the needle / lamp / readout nodes. Change
any colour or shape by hand (or with a model) and it keeps working.

Writes `07-fet-1176.frame` (the surface) and `fet.catalog.json` (the parameters
it binds to — the compressor profile's macro slots: input, output, attack,
release, ratio, meter — so the mock engine can play it).

    python3 examples/surfaces/make_1176.py
"""
import json, math, pathlib, uuid

OUT = pathlib.Path(__file__).resolve().parent

INK = "#e2dccd"       # panel legends
DIM = "#9a9384"       # secondary legends / readouts
PANEL = "#151518"
BLUE = "#1b5f79"


def nid():
    return str(uuid.uuid4())


def num(v):
    return f"{v:.2f}".rstrip("0").rstrip(".")


def text(name, s, x, y, w, h, size, fill, align="center", id=None):
    i = f' id="{id}"' if id else ""
    return f'<Text name="{name}"{i} x={num(x)} y={num(y)} w={num(w)} h={num(h)} size={num(size)} fill="{fill}" text-align="{align}">{s}</Text>'


def radial(stops):
    return "{" + json.dumps([{"kind": {"RadialGradient": {"stops": [{"position": p, "color": c} for p, c in stops]}}}]) + "}"


def linear(stops):
    return "{" + json.dumps([{"kind": {"LinearGradient": {"stops": [{"position": p, "color": c} for p, c in stops]}}}]) + "}"


def control(kind, source, slots):
    c = {"kind": kind, "binding": {"source": source, "curve": "Linear", "min": 0.0, "max": 1.0}, "slots": [{"role": r, "node": n} for r, n in slots]}
    return "{" + json.dumps(c) + "}"


def param(addr):
    return {"Param": {"addr": addr}}


def knob(name, addr, cx, cy, d, scale, legend_y=None):
    """A silver-top knob: black skirt, knurled chrome cap, white index; a
    printed dot scale with numerals around it; legend and live readout."""
    needle, value = nid(), nid()
    out = []
    # Printed scale (outside the control, so it doesn't take clicks).
    r_dot, r_num = d / 2 + 7, d / 2 + 19
    for k, label in enumerate(scale):
        a = math.radians(-135 + 270 * k / (len(scale) - 1))
        dx, dy = math.sin(a), -math.cos(a)
        out.append(f'<Ellipse name="dot" x={num(cx + r_dot * dx - 1.6)} y={num(cy + r_dot * dy - 1.6)} w=3.2 h=3.2 fill="{INK}" />')
        if label:
            out.append(text("scale", label, cx + r_num * dx - 14, cy + r_num * dy - 7, 28, 14, 9.5, DIM))
    # The control: a square dial whose centre is the needle's pivot.
    x, y = cx - d / 2, cy - d / 2
    cap = d * 0.66
    ctl = control({"Knob": {"sweep_degrees": 270.0}}, param(addr), [("Needle", needle), ("Value", value)])
    out.append(f'''<Frame name="{name}" x={num(x)} y={num(y)} w={num(d)} h={num(d)} control={ctl}>
  <Ellipse name="skirt" w={num(d)} h={num(d)} fill={radial([(0, "#2a2a2e"), (0.8, "#111113"), (1, "#050506")])} effects={{[{{"kind":"DropShadow","color":"#00000099","offset_y":4,"radius":8}}]}} />
  <Ellipse name="knurl" x={num((d - cap) / 2 - 3)} y={num((d - cap) / 2 - 3)} w={num(cap + 6)} h={num(cap + 6)} fill="#3a3a3f" stroke="#6d6d74" stroke-weight=1 />
  <Ellipse name="cap" x={num((d - cap) / 2)} y={num((d - cap) / 2)} w={num(cap)} h={num(cap)} fill={radial([(0, "#f4f4f2"), (0.55, "#c9c9c6"), (1, "#8e8e8b")])} />
  <Rectangle name="index" id="{needle}" x={num(d / 2 - 2)} y={num(d * 0.06)} w=4 h={num(d * 0.36)} radius=2 fill="#f7f4ea" />
</Frame>''')
    ly = legend_y if legend_y is not None else cy + d / 2 + 26
    out.append(text(f"{name} legend", name.upper(), cx - 50, ly, 100, 16, 11, INK))
    out.append(text(f"{name} value", "", cx - 50, ly + 16, 100, 14, 10, DIM, id=value))
    return out


def select_bank(name, addr, x, y, labels, order):
    """A column of push-in selectors. `labels` top to bottom; `order` gives
    each label's option index in the catalog."""
    out = [text(f"{name} legend", name.upper(), x - 10, y - 24, 66, 14, 10, INK)]
    for row, label in enumerate(labels):
        lamp = nid()
        ctl = control({"Button": {"mode": {"Select": {"index": order[label], "count": len(labels)}}}}, param(addr), [("Led", lamp)])
        by = y + row * 40
        out.append(f'''<Frame name="{name} {label}" x={num(x)} y={num(by)} w=46 h=32 radius=4 fill={linear([(0, "#2f2f34"), (1, "#222226")])} stroke="#050506" stroke-weight=1 control={ctl}>
  <Rectangle name="engaged" id="{lamp}" x=4 y=4 w=38 h=24 radius=3 fill="#d8cfb8" opacity=0.9 />
  {text("label", label, 0, 9, 46, 14, 11, "#111113")}
</Frame>''')
    return out


def vu(x, y, w, h):
    """An ivory VU window. The needle's parent is a square frame centred on
    the pivot (below the window), so the needle rotates about the pivot."""
    needle, value = nid(), nid()
    px, py, r = w / 2, h + 4, 88
    out = []
    marks = [(-20, "20"), (-10, "10"), (-7, "7"), (-5, "5"), (-3, "3"), (-2, ""), (-1, ""), (0, "0"), (1, ""), (2, ""), (3, "3")]
    for db, label in marks:
        p = (db + 20) / 23
        a = math.radians((p - 0.5) * 96)
        tx, ty = px + r * math.sin(a), py - r * math.cos(a)
        red = db >= 0
        c, s = math.cos(a), math.sin(a)
        # a 1.6 × 10 tick, rotated about its own centre
        tw, th = 1.6, 10.0 if label else 6.0
        ox, oy = tx - (c * tw / 2 - s * th / 2), ty - (s * tw / 2 + c * th / 2)
        tr = json.dumps({"a": c, "b": -s, "tx": ox, "c": s, "d": c, "ty": oy})
        out.append(f'<Rectangle name="tick" w={num(tw)} h={num(th)} fill="{"#b3261e" if red else "#1c1c1c"}" transform={{{tr}}} />')
        if label:
            out.append(text("mark", label, px + (r + 13) * math.sin(a) - 12, py - (r + 13) * math.cos(a) - 7, 24, 12, 9, "#b3261e" if red else "#1c1c1c"))
    # The red zone, 0..+3, as a thin ring sector.
    a0, a1 = math.radians((20 / 23 - 0.5) * 96), math.radians(0.5 * 96)
    ro, ri = r + 3, r - 1
    pt = lambda rr, aa: (px + rr * math.sin(aa), py - rr * math.cos(aa))
    (x0, y0), (x1, y1), (x2, y2), (x3, y3) = pt(ro, a0), pt(ro, a1), pt(ri, a1), pt(ri, a0)
    zone = f"M {x0:.2f} {y0:.2f} A {ro} {ro} 0 0 1 {x1:.2f} {y1:.2f} L {x2:.2f} {y2:.2f} A {ri} {ri} 0 0 0 {x3:.2f} {y3:.2f} Z"
    out.append(f'<Vector name="red zone" w={num(w)} h={num(h)} fill="#b3261e" geometry="{zone}" />')
    out.append(text("vu", "VU", w / 2 - 20, h * 0.52, 40, 18, 15, "#1c1c1c"))
    out.append(text("gr", "", w / 2 - 40, h * 0.52 + 18, 80, 12, 8.5, "#6b6252", id=value))
    out.append(f'''<Frame name="needle pivot" x={num(px - r)} y={num(py - r)} w={num(2 * r)} h={num(2 * r)}>
  <Rectangle name="needle" id="{needle}" x={num(r - 0.8)} y={num(r - 94)} w=1.6 h=94 fill="#141414" />
</Frame>''')
    ctl = control({"Gauge": {"sweep_degrees": 96.0, "floor_db": -20.0, "ceiling_db": 3.0}}, {"Stream": {"id": "1176/gr"}}, [("Needle", needle), ("Value", value)])
    inner = "\n  ".join(out)
    return f'''<Frame name="VU" x={num(x)} y={num(y)} w={num(w)} h={num(h)} radius=6 clip fill={radial([(0, "#fbf3dc"), (1, "#e6d9b6")])} stroke="#0b0b0c" stroke-weight=3 effects={{[{{"kind":"InnerShadow","color":"#00000066","offset_y":2,"radius":6}}]}} control={ctl}>
  {inner}
</Frame>'''


W, H = 1060, 330
nodes = []
# Brushed anodising: faint horizontal grain.
for k in range(0, H, 3):
    nodes.append(f'<Rectangle name="grain" x=40 y={k} w={W - 80} h=1 fill="#ffffff" opacity={0.012 + 0.01 * ((k * 7) % 5) / 5:.4f} />')
# Rack ears with slotted holes.
for ex in (0, W - 40):
    nodes.append(f'<Frame name="ear" x={ex} y=0 w=40 h={H} fill={linear([(0, "#26262a"), (0.5, "#34343a"), (1, "#26262a")])}>'
                 + "".join(f'<Rectangle name="slot" x=8 y={yy} w=24 h=12 radius=6 fill="#070708" />' for yy in (40, H - 52)) + "</Frame>")
# Wordmark.
nodes.append(text("model", "FET LIMITING AMPLIFIER", 60, 18, 300, 16, 11, DIM, align="left"))
nodes.append(text("maker", "frame · 1176-style", 60, H - 30, 300, 14, 9.5, "#6d6a62", align="left"))
# Input / Output.
atten = ["∞", "48", "36", "30", "24", "18", "12", "6", "0"]
nodes += knob("Input", "1176/input", 170, 158, 112, atten)
nodes += knob("Output", "1176/output", 380, 158, 112, atten)
# Attack over Release.
seven = [str(k) for k in range(1, 8)]
nodes += knob("Attack", "1176/attack", 548, 98, 58, seven, legend_y=140)
nodes += knob("Release", "1176/release", 548, 232, 58, seven, legend_y=274)
# Ratio bank: printed 20 at the top, 4 at the bottom, "All" above them.
nodes += select_bank("Ratio", "1176/ratio", 636, 70, ["All", "20", "12", "8", "4"], {"4": 0, "8": 1, "12": 2, "20": 3, "All": 4})
# The meter's painted band.
nodes.append(f'<Rectangle name="meter band" x=712 y=0 w=226 h={H} fill={linear([(0, "#164f65"), (0.5, BLUE), (1, "#164f65")])} />')
jewel = nid()
jewel_ctl = control({"Button": {"mode": "Toggle"}}, param("1176/on"), [("Led", jewel)])
nodes.append(f'''<Frame name="Power" x=812 y=16 w=26 h=26 control={jewel_ctl}>
  <Ellipse name="bezel" w=26 h=26 fill="#b9b9b6" />
  <Ellipse name="jewel" id="{jewel}" x=4 y=4 w=18 h=18 fill={radial([(0, "#ff8a7a"), (0.45, "#e33a2a"), (1, "#7a120b")])} effects={{[{{"kind":"DropShadow","color":"#ff3b2f99","radius":10}}]}} />
</Frame>''')
nodes.append(vu(737, 54, 176, 122))
nodes.append(text("band title", "LIMITING AMPLIFIER", 712, 200, 226, 18, 13, "#f0f5f8"))
nodes.append(text("band sub", "PEAK LIMITER", 712, 222, 226, 14, 9.5, "#bcd8e4"))
# Meter source.
nodes += select_bank("Meter", "1176/meter", 960, 70, ["GR", "+8", "+4", "Off"], {"GR": 0, "+8": 1, "+4": 2, "Off": 3})

body = "\n    ".join("\n    ".join(n.splitlines()) for n in nodes)
markup = f'''<Document>
  <Page name="Surface">
    <Frame name="FET Limiter" w={W} h={H} radius=4 clip fill={linear([(0, "#1d1d21"), (0.5, PANEL), (1, "#1a1a1e")])} effects={{[{{"kind":"DropShadow","color":"#000000aa","offset_y":10,"radius":30}}]}}>
    {body}
    </Frame>
  </Page>
</Document>
'''
(OUT / "07-fet-1176.frame").write_text(markup)


def p(addr, name, kind, rng, default, featured=True, short=None):
    return {"addr": addr, "name": name, "short_name": short, "group": "FET", "kind": kind, "range": rng, "default": default, "bipolar": False, "featured": featured}


def rng(lo, hi, unit="None", steps=None):
    return {"min": lo, "max": hi, "scale": "Linear", "unit": unit, "steps": steps}


catalog = {
    "id": "comp.fet_1176",
    "version": 1,
    "params": [
        p("1176/input", "Input", "Continuous", rng(-48.0, 0.0, "Db"), 0.62),
        p("1176/output", "Output", "Continuous", rng(-48.0, 0.0, "Db"), 0.75),
        p("1176/attack", "Attack", "Continuous", rng(1.0, 7.0), 0.5),
        p("1176/release", "Release", "Continuous", rng(1.0, 7.0), 0.5),
        p("1176/ratio", "Ratio", {"Choice": {"options": ["4", "8", "12", "20", "All"]}}, rng(0.0, 1.0, steps=5), 0.0),
        p("1176/meter", "Meter", {"Choice": {"options": ["GR", "+8", "+4", "Off"]}}, rng(0.0, 1.0, steps=4), 0.0),
        p("1176/on", "Power", "Toggle", rng(0.0, 1.0, steps=2), 1.0),
    ],
    "actions": [],
    "streams": [
        {"id": "1176/gr", "name": "Gain Reduction", "kind": "GainReduction", "rate_hz": 30.0},
        {"id": "1176/out", "name": "Output", "kind": {"Level": {"channels": 2}}, "rate_hz": 30.0},
    ],
    "collections": [],
}
(OUT / "fet.catalog.json").write_text(json.dumps(catalog, indent=2))
print("wrote 07-fet-1176.frame and fet.catalog.json")
