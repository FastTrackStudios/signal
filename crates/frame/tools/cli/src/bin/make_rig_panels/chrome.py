"""gen_chrome.py DOM_DUMP OUT_RS

Turn rig_shot's DOM dump (RIG_SHOT_DOM=1: every element's box and computed
style, every text run, every inline SVG's children) into Rust data for
frame's `make_rig_panels`: the rig row's chrome — panel grounds, rules,
buttons, pickers, knob bodies/arcs/pointers, icons and every label (as
outlines in the fonts Blitz used: Noto Sans for the UI, Hack for
`ui-monospace`) — in DOM paint order, with markers where the visualizer
widgets sit.
"""
import math, re, sys
from fontTools.ttLib import TTFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.varLib import instancer
import uharfbuzz as hb

ROW = (285.0, 95.0, 1510.0, 630.0)  # x0, y0, x1, y1 (rig px)

NOTO = "/nix/store/vypcbjrblyw90m5g63y6zh8fwq13mx4x-noto-fonts-2026.08.01/share/fonts/noto/NotoSans.ttf"
HACK = {400: "/nix/store/n7j26wd3g1zr8yq5yvssr4904lbhs2cc-hack-font-3.003/share/fonts/truetype/Hack-Regular.ttf",
        700: "/nix/store/n7j26wd3g1zr8yq5yvssr4904lbhs2cc-hack-font-3.003/share/fonts/truetype/Hack-Bold.ttf"}
FALLBACK = ["/nix/store/0lhf2h2fnjqvpzmk92s5axr2ybvs981s-dejavu-fonts-2.37/share/fonts/truetype/DejaVuSans.ttf",
            "/nix/store/vypcbjrblyw90m5g63y6zh8fwq13mx4x-noto-fonts-2026.08.01/share/fonts/noto/NotoSansSymbols2-Regular.otf"]

_fonts = {}


def font(path, weight=None):
    key = (path, weight)
    if key in _fonts:
        return _fonts[key]
    tt = TTFont(path)
    if "fvar" in tt and weight is not None:
        axes = {a.axisTag: a for a in tt["fvar"].axes}
        loc = {}
        if "wght" in axes:
            a = axes["wght"]
            loc["wght"] = max(a.minValue, min(a.maxValue, weight))
        for tag, a in axes.items():
            if tag != "wght":
                loc[tag] = a.defaultValue
        tt = instancer.instantiateVariableFont(tt, loc)
    import io
    buf = io.BytesIO()
    tt.save(buf)
    data = buf.getvalue()
    face = hb.Face(data)
    hbf = hb.Font(face)
    upem = face.upem
    hbf.scale = (upem, upem)
    gs = tt.getGlyphSet()
    order = tt.getGlyphOrder()
    cmap = tt.getBestCmap()
    hhea = tt["hhea"]
    os2 = tt["OS/2"]
    asc, desc, gap = hhea.ascent, -hhea.descent, hhea.lineGap
    if os2.fsSelection & (1 << 7):  # USE_TYPO_METRICS
        asc, desc, gap = os2.sTypoAscender, -os2.sTypoDescender, os2.sTypoLineGap
    f = dict(tt=tt, hb=hbf, upem=upem, gs=gs, order=order, cmap=cmap, asc=asc, desc=desc, gap=gap)
    _fonts[key] = f
    return f


def shape_run(f, text, size, spacing):
    """[(glyph name, x px, font)] and the advance, px."""
    buf = hb.Buffer()
    buf.add_str(text)
    buf.guess_segment_properties()
    hb.shape(f["hb"], buf, {"kern": True, "liga": True})
    k = size / f["upem"]
    x = 0.0
    out = []
    for info, pos in zip(buf.glyph_infos, buf.glyph_positions):
        out.append((f["order"][info.codepoint], x + pos.x_offset * k, f, info.codepoint))
        x += pos.x_advance * k + spacing
    return out, x


def shape(fam, weight, text, size, spacing):
    primary = font(fam, weight) if fam == NOTO else font(HACK[700 if weight >= 600 else 400])
    # split into runs by which font has the character
    runs = []
    for ch in text:
        f = primary
        if ord(ch) not in primary["cmap"]:
            for fb in FALLBACK:
                cand = font(fb, weight)
                if ord(ch) in cand["cmap"]:
                    f = cand
                    break
        if runs and runs[-1][0] is f:
            runs[-1][1] += ch
        else:
            runs.append([f, ch])
    glyphs, x = [], 0.0
    for f, s in runs:
        g, adv = shape_run(f, s, size, spacing)
        glyphs += [(n, x + gx, ff, gid) for n, gx, ff, gid in g]
        x += adv
    return glyphs, x, primary


def outline(glyphs, size, ox, baseline):
    parts = []
    for name, gx, f, _ in glyphs:
        pen = SVGPathPen(f["gs"], ntos=lambda v: ("%.2f" % v).rstrip("0").rstrip(".") or "0")
        k = size / f["upem"]
        tp = TransformPen(pen, (k, 0, 0, -k, ox + gx, baseline))
        f["gs"][name].draw(tp)
        parts.append(pen.getCommands())
    return " ".join(p for p in parts if p)


# ── colours ───────────────────────────────────────────────────────────────

def oklch_to_srgb(L, C, h):
    a, b = C * math.cos(math.radians(h)), C * math.sin(math.radians(h))
    l_ = L + 0.3963377774 * a + 0.2158037573 * b
    m_ = L - 0.1055613458 * a - 0.0638541728 * b
    s_ = L - 0.0894841775 * a - 1.2914855480 * b
    l, m, s = l_ ** 3, m_ ** 3, s_ ** 3
    r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s
    g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s
    bb = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s

    def enc(c):
        c = max(0.0, min(1.0, c))
        return 12.92 * c if c <= 0.0031308 else 1.055 * c ** (1 / 2.4) - 0.055
    return enc(r), enc(g), enc(bb)


def parse_abs(s):
    m = re.search(r"ColorComponents\(([-\d.e]+), ([-\d.e]+), ([-\d.e]+)\), alpha: ([\d.e-]+), color_space: (\w+)", s)
    if not m:
        return None
    c0, c1, c2, a = (float(m.group(i)) for i in range(1, 5))
    space = m.group(5)
    if space == "Oklch":
        c0, c1, c2 = oklch_to_srgb(c0, c1, c2)
    elif space == "Oklab":
        C, h = math.hypot(c1, c2), math.degrees(math.atan2(c2, c1))
        c0, c1, c2 = oklch_to_srgb(c0, C, h)
    elif space != "Srgb":
        raise ValueError(space)
    return (c0, c1, c2, a)


def hex8(c, mul=1.0):
    r, g, b, a = c
    q = lambda v: max(0, min(255, round(v * 255)))
    return "#%02x%02x%02x%02x" % (q(r), q(g), q(b), q(a * mul))


def css_color(s):
    s = s.strip()
    if s == "transparent":
        return (0, 0, 0, 0)
    m = re.match(r"rgba?\(([^)]*)\)", s)
    if m:
        v = [float(x) for x in m.group(1).split(",")]
        return (v[0] / 255, v[1] / 255, v[2] / 255, v[3] if len(v) > 3 else 1.0)
    named = {"black": "#000000", "white": "#ffffff", "red": "#ff0000", "green": "#008000", "blue": "#0000ff", "gray": "#808080", "grey": "#808080"}
    s = named.get(s.lower(), s)
    h = s.lstrip("#")
    if len(h) in (3,):
        h = "".join(c * 2 for c in h)
    a = int(h[6:8], 16) / 255 if len(h) == 8 else 1.0
    return (int(h[0:2], 16) / 255, int(h[2:4], 16) / 255, int(h[4:6], 16) / 255, a)


def split_args(s):
    out, depth, cur = [], 0, ""
    for ch in s:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur.strip())
            cur = ""
        else:
            cur += ch
    out.append(cur.strip())
    return out


def parse_gradient(style):
    m = re.search(r"linear-gradient\((.*)\)\s*;?", style)
    if not m:
        return None
    body = m.group(1)
    # up to the matching paren
    depth, end = 0, len(body)
    for i, ch in enumerate(body):
        if ch == "(":
            depth += 1
        elif ch == ")":
            if depth == 0:
                end = i
                break
            depth -= 1
    parts = split_args(body[:end])
    angle = 180.0
    first = parts[0]
    dirs = {"to top": 0.0, "to right": 90.0, "to bottom": 180.0, "to left": 270.0}
    if first in dirs:
        angle = dirs[first]
        parts = parts[1:]
    elif first.endswith("deg"):
        angle = float(first[:-3])
        parts = parts[1:]
    stops = []
    for i, p in enumerate(parts):
        pm = re.match(r"(.*?)\s+([\d.]+)%$", p)
        if pm:
            col, pos = pm.group(1), float(pm.group(2)) / 100
        else:
            col, pos = p, None
        stops.append([pos, css_color(col)])
    n = len(stops)
    for i, s in enumerate(stops):
        if s[0] is None:
            s[0] = 0.0 if i == 0 else (1.0 if i == n - 1 else i / (n - 1))
    # CSS interpolates transparent in premultiplied space: a stop fading to
    # `transparent` keeps the other stop's colour.
    for i, s in enumerate(stops):
        if s[1][3] == 0:
            nb = stops[i - 1] if i > 0 else stops[i + 1]
            s[1] = (nb[1][0], nb[1][1], nb[1][2], 0.0)
    return angle, [(p, hex8(c)) for p, c in stops]


# ── the dump ──────────────────────────────────────────────────────────────

class El:
    def __init__(self, depth, id_, tag, box, cls, style):
        self.depth, self.id, self.tag, self.box, self.cls, self.style = depth, id_, tag, box, cls, style
        self.cs = {}
        self.attrs = {}
        self.svg = []
        self.parent = None


def parse(path):
    items = []  # El or ("text", depth, box, fs, color, text)
    cur = None
    for line in open(path):
        line = line.rstrip("\n")
        if line.startswith("EL "):
            m = re.match(r'EL d=(\d+) id=(\d+) (\S+) \(([-\d.]+),([-\d.]+),([-\d.]+),([-\d.]+)\) class="(.*?)" style="(.*)" d=".*"$', line)
            if not m:
                m2 = re.match(r'EL d=(\d+) id=(\d+) (\S+) \(([-\d.]+),([-\d.]+),([-\d.]+),([-\d.]+)\) class="(.*?)" style="(.*)', line)
                m = m2
            d, i, tag = int(m.group(1)), int(m.group(2)), m.group(3)
            box = tuple(float(m.group(k)) for k in range(4, 8))
            cur = El(d, i, tag, box, m.group(8), m.group(9))
            items.append(cur)
        elif line.startswith("  CS "):
            body = line[5:]
            for part in body.split(" | "):
                k, _, v = part.partition("=")
                cur.cs[k.strip()] = v.strip()
        elif line.startswith("  ATTRS "):
            for m in re.finditer(r'([\w:-]+)="((?:[^"\\]|\\.)*)"', line[8:]):
                cur.attrs[m.group(1)] = m.group(2)
        elif line.startswith("  SVGCHILD "):
            m = re.match(r"  SVGCHILD (\S+) (.*) text=\"(.*)\"$", line)
            tag, rest, txt = m.group(1), m.group(2), m.group(3)
            attrs = {mm.group(1): mm.group(2) for mm in re.finditer(r'([\w:-]+)="((?:[^"\\]|\\.)*)"', rest)}
            cur.svg.append((tag, attrs, txt))
        elif line.startswith("TEXT "):
            m = re.match(r'TEXT d=(\d+) box=\(([-\d.]+),([-\d.]+),([-\d.]+),([-\d.]+)\) fs=([\d.]+) color=(AbsoluteColor \{.*?\}) (".*")$', line)
            d = int(m.group(1))
            box = tuple(float(m.group(k)) for k in range(2, 6))
            items.append(("text", d, box, float(m.group(6)), parse_abs(m.group(7)), eval(m.group(8))))
    # parents by depth
    stack = []
    for it in items:
        d = it.depth if isinstance(it, El) else it[1]
        while stack and stack[-1].depth >= d:
            stack.pop()
        par = stack[-1] if stack else None
        if isinstance(it, El):
            it.parent = par
            stack.append(it)
        else:
            items[items.index(it)] = it + (par,)
    return items


def px(v):
    m = re.search(r"([-\d.]+)px", v or "")
    return float(m.group(1)) if m else 0.0


def ancestors(el):
    while el is not None:
        yield el
        el = el.parent


def opacity(el):
    o = 1.0
    for a in ancestors(el):
        o *= float(a.cs.get("op", "1") or 1)
    return o


def eq_label(el):
    """Part of the EQ graph's own DOM (its labels, pills and range select):
    `viz.eq_graph` draws those itself."""
    return any(a.attrs.get("data-testid", "") in ("eq-freq-label", "eq-db-label", "eq-db-range") for a in ancestors(el))


def clips(el):
    """The intersection of overflow-hidden ancestors' padding boxes."""
    x0, y0, x1, y1 = ROW
    for a in ancestors(el.parent if el is not None else None):
        if "overflow-hidden" in a.cls or re.search(r"overflow:\s*hidden", a.style):
            bx, by, bw, bh = a.box
            x0, y0, x1, y1 = max(x0, bx), max(y0, by), min(x1, bx + bw), min(y1, by + bh)
    return (x0, y0, x1, y1)


# ── SVG path transform (uniform scale + translate) ────────────────────────

def xform_path(d, s, tx, ty):
    toks = re.findall(r"[MmLlHhVvCcSsQqTtAaZz]|[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?", d)
    out, i, cmd = [], 0, None
    counts = dict(M=2, L=2, H=1, V=1, C=6, S=4, Q=4, T=2, A=7, Z=0)
    while i < len(toks):
        t = toks[i]
        if re.match(r"[A-Za-z]", t):
            cmd = t
            out.append(cmd)
            i += 1
            if cmd in "Zz":
                continue
        n = counts[cmd.upper()]
        vals = [float(v) for v in toks[i:i + n]]
        i += n
        rel = cmd.islower()
        up = cmd.upper()
        if up == "A":
            rx, ry, rot, la, sw, x, y = vals
            x, y = (x * s, y * s) if rel else (x * s + tx, y * s + ty)
            out.append(f"{rx*s:.3f} {ry*s:.3f} {rot:g} {int(la)} {int(sw)} {x:.3f} {y:.3f}")
        elif up == "H":
            out.append(f"{vals[0]*s if rel else vals[0]*s+tx:.3f}")
        elif up == "V":
            out.append(f"{vals[0]*s if rel else vals[0]*s+ty:.3f}")
        else:
            pts = []
            for k in range(0, n, 2):
                x, y = vals[k], vals[k + 1]
                x, y = (x * s, y * s) if rel else (x * s + tx, y * s + ty)
                pts.append(f"{x:.3f} {y:.3f}")
            out.append(" ".join(pts))
        # implicit repeats: M → L
        if cmd in "Mm":
            cmd = "l" if cmd == "m" else "L"
    return " ".join(out)


def circle_d(cx, cy, r):
    return f"M {cx-r:.3f} {cy:.3f} A {r:.3f} {r:.3f} 0 1 0 {cx+r:.3f} {cy:.3f} A {r:.3f} {r:.3f} 0 1 0 {cx-r:.3f} {cy:.3f} Z"


def rrect_d(x, y, w, h, r):
    r = min(r, w / 2, h / 2)
    if r <= 0:
        return f"M {x:.3f} {y:.3f} H {x+w:.3f} V {y+h:.3f} H {x:.3f} Z"
    return (f"M {x+r:.3f} {y:.3f} H {x+w-r:.3f} A {r:.3f} {r:.3f} 0 0 1 {x+w:.3f} {y+r:.3f} V {y+h-r:.3f} "
            f"A {r:.3f} {r:.3f} 0 0 1 {x+w-r:.3f} {y+h:.3f} H {x+r:.3f} A {r:.3f} {r:.3f} 0 0 1 {x:.3f} {y+h-r:.3f} "
            f"V {y+r:.3f} A {r:.3f} {r:.3f} 0 0 1 {x+r:.3f} {y:.3f} Z")


# ── emit ──────────────────────────────────────────────────────────────────

def rs(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main(dump, out_path):
    items = parse(dump)
    out = []  # Rust expressions
    widgets = 0

    def emit(kind, **kw):
        out.append((kind, kw))

    for it in items:
        if isinstance(it, El):
            el = it
            x, y, w, h = el.box
            if w <= 0 or h <= 0:
                continue
            clip = clips(el)
            if clip[2] <= clip[0] or clip[3] <= clip[1]:
                continue
            op = opacity(el)
            if eq_label(el):
                continue
            if el.tag == "object":
                emit("widget", x=x, y=y, w=w, h=h, op=op, clip=clip)
                widgets += 1
                continue
            bg = parse_abs(el.cs.get("bg", "")) if el.cs.get("bg") else None
            grad = parse_gradient(el.style) if "gradient" in el.style else None
            rad = 0.0
            mrad = re.search(r"width: NonNegative\((?:Length\()?([\d.]+) px", el.cs.get("rad", ""))
            pr = re.search(r"Percentage\(([\d.]+)\)", el.cs.get("rad", ""))
            if mrad:
                rad = float(mrad.group(1))
            elif pr:
                rad = float(pr.group(1)) * min(w, h)
            sides = {}
            styles = el.cs.get("bs", "").split()
            for i, k in enumerate(("bt", "bl", "bb", "br")):
                v = el.cs.get(k, "")
                mw = re.search(r"BorderSideWidth\(([\d.]+)px\)", v)
                bw = float(mw.group(1)) if mw else 0.0
                col = parse_abs(v)
                if i < len(styles) and styles[i] in ("None", "Hidden"):
                    continue
                if bw > 0 and col and col[3] > 0:
                    sides[k] = (bw, col)
            shadow = "box-shadow" in el.style and "0 2px 6px" in el.style
            if (bg and bg[3] > 0) or grad or sides:
                uniform = len(sides) == 4 and len({(s[0], hex8(s[1])) for s in sides.values()}) == 1
                emit("box", x=x, y=y, w=w, h=h, r=rad,
                     fill=hex8(bg) if bg and bg[3] > 0 else "",
                     grad=grad,
                     stroke=(hex8(sides["bt"][1]), sides["bt"][0]) if uniform else None,
                     shadow=shadow, op=op, clip=clip)
                if sides and not uniform:
                    for k, (bw, col) in sides.items():
                        rx, ry, rw, rh = {"bt": (x, y, w, bw), "bb": (x, y + h - bw, w, bw), "bl": (x, y, bw, h), "br": (x + w - bw, y, bw, h)}[k]
                        emit("box", x=rx, y=ry, w=rw, h=rh, r=0.0, fill=hex8(col), grad=None, stroke=None, shadow=False, op=op, clip=clip)
            if el.tag == "svg" and el.svg:
                vb = el.attrs.get("viewBox") or el.attrs.get("viewbox")
                if vb:
                    vx, vy, vw, vh = (float(v) for v in re.split(r"[ ,]+", vb.strip()))
                else:
                    vx, vy, vw, vh = 0.0, 0.0, w, h
                s = min(w / vw, h / vh)
                # preserveAspectRatio="none" (the comp overlay) scales each axis
                non_uniform = el.attrs.get("preserveAspectRatio", "") == "none" or el.attrs.get("preserve_aspect_ratio", "") == "none"
                sx, sy = (w / vw, h / vh) if non_uniform else (s, s)
                ox = x + (w - vw * sx) / 2 - vx * sx
                oy = y + (h - vh * sy) / 2 - vy * sy
                color = parse_abs(el.cs.get("color", "")) or (1, 1, 1, 1)
                base = {k: el.attrs.get(k) for k in ("fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin")}

                def paint_of(v, dflt):
                    if v is None:
                        v = dflt
                    if v is None or v == "none":
                        return ""
                    if v == "currentColor":
                        return hex8(color)
                    return hex8(css_color(v))
                for tag, at, txt in reversed(el.svg):  # dumped deepest-last, reversed = document order
                    get = lambda k: at.get(k, base.get(k))
                    fill = paint_of(get("fill"), "black")
                    stroke = paint_of(get("stroke"), None)
                    sw = float(get("stroke-width") or 1) * sx
                    cap = get("stroke-linecap") or "butt"
                    fo = float(at.get("fill-opacity", 1))
                    so = float(at.get("stroke-opacity", 1))
                    d = None
                    if tag == "path" and at.get("d"):
                        d = xform_path(at["d"], sx, ox, oy) if not non_uniform else None
                        if non_uniform:
                            d = xform_path(at["d"], sx, ox, oy)  # comp overlay has no paths
                    elif tag == "line":
                        x1, y1, x2, y2 = (float(at.get(k, 0)) for k in ("x1", "y1", "x2", "y2"))
                        d = f"M {x1*sx+ox:.3f} {y1*sy+oy:.3f} L {x2*sx+ox:.3f} {y2*sy+oy:.3f}"
                        fill = ""
                    elif tag == "circle":
                        d = circle_d(float(at.get("cx", 0)) * sx + ox, float(at.get("cy", 0)) * sy + oy, float(at.get("r", 0)) * sx)
                    elif tag == "rect":
                        rx_ = float(at.get("rx", 0))
                        d = rrect_d(float(at.get("x", 0)) * sx + ox, float(at.get("y", 0)) * sy + oy, float(at.get("width", 0)) * sx, float(at.get("height", 0)) * sy, rx_ * sx)
                    elif tag == "polyline" or tag == "polygon":
                        pts = [float(v) for v in re.split(r"[ ,]+", at.get("points", "").strip()) if v]
                        d = "M " + " L ".join(f"{pts[k]*sx+ox:.3f} {pts[k+1]*sy+oy:.3f}" for k in range(0, len(pts), 2))
                        if tag == "polygon":
                            d += " Z"
                    elif tag == "text" and txt.strip():
                        fs = float(at.get("font-size", at.get("font_size", 9))) * sy
                        anchor = at.get("text-anchor", at.get("text_anchor", "start"))
                        glyphs, tw, f = shape(NOTO, 400, txt, fs, 0.0)
                        tx = float(at.get("x", 0)) * sx + ox
                        if anchor == "middle":
                            tx -= tw / 2
                        elif anchor == "end":
                            tx -= tw
                        ty = float(at.get("y", 0)) * sy + oy
                        emit("path", d=outline(glyphs, fs, tx, ty), fill=paint_of(at.get("fill"), "black"), stroke="", sw=0.0, cap="butt", op=op, clip=clip)
                        continue
                    if not d:
                        continue
                    if fill and fo < 1:
                        fill = fill[:7] + "%02x" % round(int(fill[7:9], 16) * fo)
                    if stroke and so < 1:
                        stroke = stroke[:7] + "%02x" % round(int(stroke[7:9], 16) * so)
                    emit("path", d=d, fill=fill, stroke=stroke, sw=sw if stroke else 0.0, cap=cap, op=op, clip=clip)
        else:
            _, depth, box, fs, color, text, par = it
            if par is None or eq_label(par):
                continue
            clip = clips(par) if par else ROW
            x0, y0, x1, y1 = clip
            bx, by, bw, bh = box
            if bx >= x1 or by >= y1 or bx + bw <= x0 or by + bh <= y0:
                continue
            ff = par.cs.get("ff", "")
            mono = "onospace" in ff
            fw = 400
            m = re.search(r"value: (\d+)", par.cs.get("fw", ""))
            if m:
                fw = int(m.group(1)) / 64
            tt = par.cs.get("tt", "")
            if tt.strip() == "TextTransform(1)":
                text = text.upper()
            ls = 0.0
            mls = re.search(r"([\d.]+)em", par.style)
            if "letter-spacing" in par.style and mls:
                ls = float(mls.group(1)) * fs
            elif "tracking-wide" in par.cls:
                ls = 0.025 * fs
            words = text.split(" ")
            # lines: greedy wrap to the box width
            def sh(s):
                if mono:
                    g, w_, f_ = shape("mono", fw, s, fs, ls)
                else:
                    g, w_, f_ = shape(NOTO, fw, s, fs, ls)
                return g, w_, f_
            lines = []
            cur = ""
            for wd in words:
                trial = wd if not cur else cur + " " + wd
                if cur and len(words) >= 3 and sh(trial)[1] > bw + 1.0:
                    lines.append(cur)
                    cur = wd
                else:
                    cur = trial
            lines.append(cur)
            f = sh(lines[0])[2]
            k = fs / f["upem"]
            asc, desc = f["asc"] * k, f["desc"] * k
            natural = (f["asc"] + f["desc"] + f["gap"]) * k
            rows = max(len(lines), round(bh / (fs * 1.45)))
            lh = bh / rows
            # a button / centred flex puts the run in the middle
            centred = par.tag == "button" or "justify-center" in par.cls or "text-center" in par.cls
            d_all = []
            for li, ln in enumerate(lines):
                g, lw, _ = sh(ln)
                tx = bx + (bw - lw) / 2 if centred else bx
                top = by + li * lh
                base_y = top + (lh - (asc + desc)) / 2 + asc
                d_all.append(outline(g, fs, tx, base_y))
            emit("path", d=" ".join(d_all), fill=hex8(color), stroke="", sw=0.0, cap="butt", op=opacity(par), clip=clip)

    # Rust
    with open(out_path, "w") as fh:
        fh.write("//! Generated by `chrome.py` from `rig_shot`'s DOM dump (RIG_SHOT_DOM=1):\n")
        fh.write("//! the rig row's chrome in paint order, in rig px (1600 × 1000 window).\n")
        fh.write("//! Do not edit by hand; regenerate.\n\n")
        fh.write("use super::chrome_types::{Item, Fill};\n\n")
        fh.write("pub const ITEMS: &[Item] = &[\n")
        for kind, kw in out:
            clip = "(%.1f, %.1f, %.1f, %.1f)" % kw["clip"]
            if kind == "widget":
                fh.write(f"    Item::Widget {{ x: {kw['x']:.1f}, y: {kw['y']:.1f}, w: {kw['w']:.1f}, h: {kw['h']:.1f}, opacity: {kw['op']:.3f} }},\n")
            elif kind == "box":
                if kw["grad"]:
                    ang, stops = kw["grad"]
                    st = ", ".join(f"({p:.3f}, {rs(c)})" for p, c in stops)
                    fill = f"Fill::Linear({ang:.1f}, &[{st}])"
                elif kw["fill"]:
                    fill = f"Fill::Solid({rs(kw['fill'])})"
                else:
                    fill = "Fill::None"
                stroke = f"Some(({rs(kw['stroke'][0])}, {kw['stroke'][1]:.2f}))" if kw["stroke"] else "None"
                fh.write(f"    Item::Box {{ x: {kw['x']:.2f}, y: {kw['y']:.2f}, w: {kw['w']:.2f}, h: {kw['h']:.2f}, r: {kw['r']:.2f}, fill: {fill}, stroke: {stroke}, shadow: {str(kw['shadow']).lower()}, opacity: {kw['op']:.3f}, clip: {clip} }},\n")
            else:
                fh.write(f"    Item::Path {{ d: {rs(kw['d'])}, fill: {rs(kw['fill'])}, stroke: {rs(kw['stroke'])}, weight: {kw['sw']:.2f}, round: {str(kw['cap'] == 'round').lower()}, opacity: {kw['op']:.3f}, clip: {clip} }},\n")
        fh.write("];\n")
    print(f"{len(out)} items, {widgets} widgets")


if __name__ == "__main__":
    # "mono" resolves to Hack
    _shape = shape

    def shape(fam, weight, text, size, spacing):  # noqa: F811
        if fam == "mono":
            f = font(HACK[700 if weight >= 600 else 400])
            return shape_mono(f, weight, text, size, spacing)
        return _shape(fam, weight, text, size, spacing)

    def shape_mono(f, weight, text, size, spacing):
        runs = []
        for ch in text:
            ff = f
            if ord(ch) not in f["cmap"]:
                for fb in FALLBACK:
                    cand = font(fb, weight)
                    if ord(ch) in cand["cmap"]:
                        ff = cand
                        break
            if runs and runs[-1][0] is ff:
                runs[-1][1] += ch
            else:
                runs.append([ff, ch])
        glyphs, x = [], 0.0
        for ff, s in runs:
            g, adv = shape_run(ff, s, size, spacing)
            glyphs += [(n, x + gx, fff, gid) for n, gx, fff, gid in g]
            x += adv
        return glyphs, x, f
    globals()["shape"] = shape
    main(sys.argv[1], sys.argv[2])
