#!/usr/bin/env python3
"""Design the example surfaces through frame-mcp's tools — exactly the calls
a model makes — against the demo guitar catalog.

    python3 examples/surfaces/design_examples.py   # rewrites examples/surfaces/*.frame
"""
import json, subprocess, sys, pathlib

ROOT = pathlib.Path(__file__).resolve().parents[2]
MCP = ROOT / "target/release/frame-mcp"
OUT = ROOT / "examples/surfaces"


def design(name, calls):
    path = OUT / name
    path.unlink(missing_ok=True)
    msgs = [{"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}]
    for i, (tool, args) in enumerate(calls):
        msgs.append({"jsonrpc": "2.0", "id": i + 1, "method": "tools/call", "params": {"name": tool, "arguments": args}})
    out = subprocess.run([str(MCP), "--file", str(path)], input="\n".join(map(json.dumps, msgs)) + "\n",
                         capture_output=True, text=True, check=True).stdout
    for line in out.splitlines():
        r = json.loads(line).get("result", {})
        if r.get("isError"):
            sys.exit(f"{name}: {r['content'][0]['text']}")
    last = json.loads(out.splitlines()[-1])["result"]["content"][0]["text"]
    print(f"{name}: {last.splitlines()[0]}")
    for l in last.splitlines()[1:4]:
        print("   ", l[:150])


def ctl(parent, kind, bind, size="medium", name=None):
    a = {"parent": parent, "kind": kind, "bind": bind, "size": size}
    if name:
        a["name"] = name
    return ("add_control", a)


def write(parent, markup):
    return ("write", {"parent": parent, "markup": markup})


# 1 — a stage rig: big knobs, output strip, a row of footswitches.
design("01-stage.frame", [
    write("Surface", '''<Frame name="Stage" layout="column" gap=20 padding=24 fill="#101216" hug=true>
  <Text name="Title" w=460 h=30 size=22 fill="#f2f2f2">Sunday Set — Guitar Rig</Text>
  <Frame name="Main" layout="row" gap=20 hug=true>
    <Frame name="Tone" layout="row" gap=18 padding=18 fill="#1a1d23" radius=14 hug=true />
    <Frame name="Out" layout="row" gap=10 padding=18 fill="#1a1d23" radius=14 hug=true />
  </Frame>
  <Frame name="Switches" layout="row" gap=16 hug=true />
</Frame>'''),
    *[ctl("Tone", "knob", f"param:{a}", "large") for a in ["amp/gain", "drive/amount", "delay/mix", "reverb/mix"]],
    ctl("Out", "fader", "param:rig/volume", name="Volume"),
    ctl("Out", "meter", "stream:out_level:0", name="L"),
    ctl("Out", "meter", "stream:out_level:1", name="R"),
    *[ctl("Switches", "footswitch", b) for b in ["param:drive/on", "param:delay/on", "param:reverb/on", "action:tap_tempo", "action:next_song"]],
    ("lint", {}),
])

# 2 — a pedalboard: three coloured enclosures, knobs on top, a stomp below.
pedals = [
    ("Overdrive", "#1f6f43", ["drive/amount", "drive/tone"], "drive/on"),
    ("Echo", "#1d4f8f", ["delay/time", "delay/feedback", "delay/mix"], "delay/on"),
    ("Hall", "#5b2d8e", ["reverb/decay", "reverb/tone", "reverb/mix"], "reverb/on"),
]
calls = [write("Surface", '<Frame name="Board" layout="row" gap=22 padding=28 fill="#2a2724" hug=true />')]
for name, color, knobs, stomp in pedals:
    calls.append(write("Board", f'''<Frame name="{name}" layout="column" gap=14 padding=18 fill="{color}" radius=16 hug=true>
  <Text name="{name} title" w=180 h=26 size=18 fill="#ffffff">{name.upper()}</Text>
  <Frame name="{name} knobs" layout="row" gap=12 hug=true />
  <Frame name="{name} stomp" layout="row" hug=true />
</Frame>'''))
    calls += [ctl(f"{name} knobs", "knob", f"param:{k}", "medium", name=k.split("/")[1].title()) for k in knobs]
    calls.append(ctl(f"{name} stomp", "footswitch", f"param:{stomp}", name=f"{name} On"))
calls.append(("lint", {}))
design("02-pedalboard.frame", calls)

# 3 — an amp head, daylight theme: one long row of big knobs.
design("03-amp-head.frame", [
    ("set_theme", {"theme": "daylight"}),
    write("Surface", '''<Frame name="Amp" layout="column" gap=18 padding=28 fill="#e9e1cf" radius=18 hug=true>
  <Frame name="Badge" layout="row" gap=12 hug=true>
    <Text name="Brand" w=260 h=34 size=26 fill="#1a1a1a">FRAME 50</Text>
    <Text name="Model" w=260 h=34 size=14 fill="#6b5f48">hand-wired · two channel</Text>
  </Frame>
  <Frame name="Controls" layout="row" gap=16 padding=18 fill="#f7f2e7" radius=12 hug=true />
</Frame>'''),
    *[ctl("Controls", "knob", f"param:amp/{k}", "large") for k in ["channel", "gain", "bass", "mid", "treble", "presence", "master"]],
    ("lint", {}),
])

# 4 — a mixer: input meter, amp master fader, main fader + stereo meters + pan.
design("04-mixer.frame", [
    write("Surface", '''<Frame name="Mixer" layout="row" gap=14 padding=22 fill="#0e0f12" hug=true>
  <Frame name="Input" layout="column" gap=10 padding=16 fill="#17191e" radius=12 hug=true />
  <Frame name="Amp" layout="column" gap=10 padding=16 fill="#17191e" radius=12 hug=true />
  <Frame name="Main bus" layout="row" gap=10 padding=16 fill="#17191e" radius=12 hug=true />
</Frame>'''),
    ctl("Input", "meter", "stream:in_level:0", name="In"),
    ctl("Input", "readout", "stream:tempo", name="Tempo"),
    ctl("Amp", "fader", "param:amp/master", name="Amp Master"),
    ctl("Main bus", "fader", "param:rig/volume", name="Main"),
    ctl("Main bus", "meter", "stream:out_level:0", name="Out L"),
    ctl("Main bus", "meter", "stream:out_level:1", name="Out R"),
    ctl("Main bus", "knob", "param:rig/pan", "small", name="Pan"),
    ("lint", {}),
])

# 5/6 — the generator's full surface, both themes.
design("05-generated-dark.frame", [("generate", {"theme": "stage-dark"}), ("lint", {})])
design("06-generated-daylight.frame", [("generate", {"theme": "daylight"}), ("lint", {})])
