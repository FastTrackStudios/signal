#!/usr/bin/env python3
"""MOJO 2: our engine against KONTRA running MOJO's own script, voice by voice.

    mojo_vs_kontra.py <mode> <tag> [--kontra BIN] [--ours BIN] [--seed N]

KONTRA (kontra-inspect play) runs the instrument's real KSP on the probe of
target/orchestra-ab/sec/mojo_<tag>/<mode>/ and reports every voice it
started (file, sample position, gain) and its audio. Its round-robin picks
(KONTRA's own random) are read off those voices and replayed into
render_css_live --rrs, so the two renders play the same recordings; then

- the voice lists are compared in order (file, start position, gain), and
- null_test.py scores our audio against KONTRA's (the KONTRA wav stands in
  for "Kontakt": a residual here is OUR model differing from the script).

`--seed N`: no replay — both draw the script's random numbers from KSP
seed N (KONTRA_KSP_SEED, render_css_live --ksp-seed), so every pick and
the key noise must agree on their own; keyclicks are compared too.

A difference between KONTRA and Kontakt is KONTRA's to fix, not ours to fit.
Writes kontra.wav, kontra_play.json, ours_vs_kontra.wav, vs_kontra.txt,
vs_kontra.json beside the probe.
"""
import csv, json, math, os, re, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
LIB = "/run/media/AudioHaven/Sampled/Horns/MOJO 2 Horn Section"
KONTAKT_LIB = "/run/media/AudioHaven/Kontakt/MOJO 2 Horn Section 2-3.0/Instruments/Instruments"
INSTRUMENT = {
    "asx": "Alto Sax", "bsx": "Baritone Sax", "btb": "Bass Trombone",
    "btbm": "Bass Trombone Muted (Simulated)", "cl": "Clarinet", "flg": "Flugelhorn",
    "flgm": "Flugelhorn Muted (Simulated)", "hn": "French Horn",
    "hnm": "French Horn Muted (Simulated)", "ptp": "Piccolo Trumpet",
    "ptpm": "Piccolo Trumpet Muted (Simulated)", "ssx": "Soprano Sax", "tsx": "Tenor Sax",
    "tb": "Trombone", "tbm": "Trombone Muted", "tp": "Trumpet", "tpm": "Trumpet Muted",
}


def float_to_pcm24(path):
    """KONTRA writes 32-bit float WAVE_FORMAT_EXTENSIBLE, which null_test.py's
    `wave` cannot open: rewrite it in place as 24-bit PCM (clipped)."""
    import struct
    d = open(path, "rb").read()
    if d[:4] != b"RIFF":
        return
    o, fmt, data = 12, None, None
    while o + 8 <= len(d):
        cid, n = d[o:o + 4], struct.unpack("<I", d[o + 4:o + 8])[0]
        if cid == b"fmt ":
            fmt = struct.unpack("<HHIIHH", d[o + 8:o + 24])
        elif cid == b"data":
            data = d[o + 8:o + 8 + n]
        o += 8 + n + (n & 1)
    if not fmt or data is None or fmt[5] != 32 or fmt[0] not in (3, 0xFFFE):
        return
    import array
    ch, rate = fmt[1], fmt[2]
    f = array.array("f"); f.frombytes(data)
    out = bytearray()
    for x in f:
        v = int(max(-1.0, min(1.0, x)) * 8388607)
        out += (v & 0xFFFFFF).to_bytes(3, "little")
    hdr = struct.pack("<4sI4s4sIHHIIHH4sI", b"RIFF", 36 + len(out), b"WAVE", b"fmt ", 16, 1, ch, rate,
                      rate * ch * 3, ch * 3, 24, b"data", len(out))
    open(path, "wb").write(hdr + bytes(out))


def arg(name, default):
    return next((sys.argv[i + 1] for i, a in enumerate(sys.argv) if a == name), default)


def main():
    mode, tag = sys.argv[1], sys.argv[2]
    inst = INSTRUMENT[tag]
    kontra = arg("--kontra", "/run/media/Development/KONTRA-mojo/target/release/kontra-inspect")
    ours_bin = arg("--ours", os.path.join(REPO, "target/x86_64-unknown-linux-gnu/desktop-dev/examples/render_css_live"))
    npy = os.environ.get("NPY_PYTHON", sys.executable)
    seed = arg("--seed", None)
    d = os.path.join(REPO, "target/orchestra-ab/sec", f"mojo_{tag}", mode)
    mid = os.path.join(d, f"{mode}.mid")

    # 1. KONTRA: the script's voices and audio
    scn = os.path.join(d, "kontra_scenario.json")
    with open(scn, "w") as f:
        subprocess.run([sys.executable, "-I", os.path.join(HERE, "analysis", "mid2kontra.py"), mid],
                       stdout=f, check=True, timeout=120)
    kwav, kjson = os.path.join(d, "kontra.wav"), os.path.join(d, "kontra_play.json")
    subprocess.run([kontra, "play", os.path.join(KONTAKT_LIB, f"{inst} 2.0.nki"), "--scenario", scn,
                    "--json", kjson],
                   env={**os.environ, "KONTRA_INSPECT_WAV": kwav, **({"KONTRA_KSP_SEED": seed} if seed else {})},
                   check=True, timeout=1800, capture_output=True)
    float_to_pcm24(kwav)
    res = json.load(open(kjson))["result"]
    kv = [e for e in res["stage_events"] if e["type"] == "voice" and e["event"] == "Start"]

    # 2. KONTRA's sample → our zone's rr_index (via the organized-file map)
    organized = {}
    for row in csv.DictReader(open(os.path.join(LIB, "_notes", "raw_to_organized.tsv")), delimiter="\t"):
        organized[row["raw"]] = row["organized"]
    zone_rr, cur = {}, None
    for line in open(os.path.join(LIB, "_patches", inst, "library.styx")):
        s = line.strip()
        if s == "{":
            cur = {}
        elif s == "}" and cur is not None:
            zone_rr.setdefault(cur["file"], cur.get("rr_index", "0")); cur = None
        elif cur is not None:
            k, _, v = s.partition(" ")
            cur[k] = v.strip().strip('"')

    def ours_file(sample):
        rel = sample.split("Samples/", 1)[1]
        c = rel.split(".nkx/")[0]
        raw = os.path.join(c + ".nkx", c, rel.split(".nkx/", 1)[1])[:-4] + ".wav"
        return organized.get(raw)

    rrs, kvoices = [], []
    for e in kv:
        f = ours_file(e["sample"])
        g = e.get("gain") or [[0.0]]
        kvoices.append((e["frame"], f, float(e.get("position") or 0), 20 * math.log10(abs(g[0][0]) or 1e-9)))
        # a legato transition draws no RR (its zone is the interval's)
        if f and "keyclick" not in f.lower() and "/Legato_" not in f and "/Octave_Legato_" not in f:
            rrs.append(zone_rr.get(f, "0"))

    # 3. ours, replaying KONTRA's picks
    owav, otrace = os.path.join(d, "ours_vs_kontra.wav"), os.path.join(d, "ours_vs_kontra_trace.txt")
    with open(otrace, "w") as f:
        draws = ["--ksp-seed", seed] if seed else ["--rrs", ",".join(rrs)]
        subprocess.run([ours_bin, mid, owav, "--library", "mojo", "--section", inst, *draws,
                        "--trace"], stdout=f, stderr=subprocess.STDOUT, check=True, timeout=1800)
    pat = re.compile(r"\[v\d+\]\s+([\d.]+)s\s+\S+\s+(\S.*?\.wav)\s+note.*?gain\s+([\d.]+).*?start\s+(\d+)")
    ovoices = []
    for line in open(otrace):
        m = pat.match(line)
        if m:
            ovoices.append((round(float(m[1]) * 48000), m[2], float(m[4]), 20 * math.log10(float(m[3]) or 1e-9)))

    # 4. voice lists side by side (keyclicks apart: not modelled yet), and the audio null
    clicks = [] if seed else [k for k in kvoices if k[1] and "keyclick" in k[1].lower()]
    if not seed:
        kvoices = [k for k in kvoices if not (k[1] and "keyclick" in k[1].lower())]
    out = [f"KONTRA voices {len(kvoices)} (+{len(clicks)} keyclicks, not modelled), ours {len(ovoices)}"]
    for i in range(max(len(kvoices), len(ovoices))):
        k = kvoices[i] if i < len(kvoices) else None
        o = ovoices[i] if i < len(ovoices) else None
        flag = ""
        if k and o:
            if k[1] != o[1]:
                flag = " FILE"
            elif abs(k[2] - o[2]) > 2:
                flag = " START"
            if abs(k[0] - o[0]) > 2:
                flag += " TIME"
        else:
            flag = " MISSING"
        kf = f"{k[0]:8} {os.path.basename(k[1] or '?'):48} pos {k[2]:8.0f} g {k[3]:6.2f}" if k else "-"
        of = f"{o[0]:8} {os.path.basename(o[1]):48} pos {o[2]:8.0f} g {o[3]:6.2f}" if o else "-"
        out.append(f"{i:3} K {kf} | O {of}{flag}")
    man = os.path.join(d, f"{mode}_manifest.tsv")
    nt = subprocess.run([npy, "-I", os.path.join(HERE, "null_test.py"), man, owav, kwav,
                         "--json", os.path.join(d, "vs_kontra.json")], capture_output=True, text=True, timeout=1800)
    out += ["", nt.stdout.strip()]
    open(os.path.join(d, "vs_kontra.txt"), "w").write("\n".join(out) + "\n")
    print("\n".join(out[:1] + [l for l in out[1:] if l.endswith(("FILE", "START", "TIME", "MISSING"))][:40]))
    print("\n".join(nt.stdout.strip().splitlines()[-3:]))


if __name__ == "__main__":
    main()
