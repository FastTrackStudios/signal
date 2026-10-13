# Komplete Kontrol "Light Guide" — protocol notes

Reverse-engineered from open-source references (DrivenByMoss,
SynthesiaKontrol, KompleteSynthesia, rebellion). This documents how the
per-key RGB LED strip ("Light Guide") is actually driven, why it is **not**
reachable over the device's ALSA/PipeWire MIDI ports, and the byte-level
format this crate implements.

## TL;DR — the crux

**The Light Guide is not a MIDI feature on any Komplete Kontrol generation.**
It is raw USB:

- **MkI** — USB HID output report, prefix `0x82`, 3 RGB bytes/key.
- **MkII** — USB HID output report, prefix `0x81`, 1 palette-index byte/key.
- **MK3** — USB **bulk** transfer to a vendor interface (**interface 4,
  endpoint 4**), a 403-byte framed message carrying `128 × {0x92, key,
  color}`. **Not** an HID report, **not** the USB-MIDI class endpoint.

**DrivenByMoss contains no light-guide code for MkII/MK3 at all** — only for
MkI (over HID). Its `mkii` package (which also serves MK3) implements only the
**NIHIA DAW-integration protocol** (transport / mixer / plug-in params /
screens) over the DAW MIDI port. So sending standard MIDI to the three ALSA
ports (`KONTROL S88 MK3 Main / DAW / Ext`) **cannot** light the keys.

Confidence: **high** for "MIDI cannot do it / MK3 uses USB bulk on iface 4";
**medium** for the exact MK3 init bytes and the S88 MK3 USB product ID (the
only open MK3 implementation, tillt/KompleteSynthesia, self-describes these as
partially reverse-engineered / placeholder).

## Ports (DrivenByMoss, `KontrolProtocolDeviceDescriptorV3.java`)

S-series MK3 Linux port names (each pair = `{input, output}`):

```java
{ "KONTROL S88 MK3 KONTROL S88 MK3 #2", "KONTROL S88 MK3 KONTROL S88 MK3" }
```

- The `#2` / macOS `…DAW` port carries the **NIHIA** host-integration traffic.
- The plain / `…Main` port is ordinary keyboard MIDI (notes, PB, mod, touch).

DrivenByMoss gates MkII off on Linux (no NI DAW-integration host agent
exists there); only MK3 is allowed, because MK3 speaks the protocol directly
over its DAW MIDI port:

```java
if (OperatingSystem.get() == LINUX && this.version < KontrolProtocol.VERSION_3)
    throw new FrameworkException("Komplete Kontrol MkII is not supported on Linux …");
```

All NIHIA control I/O uses **MIDI channel 16** (0-based index 15).

## NIHIA handshake / init (DAW port — this does NOT light keys)

CC handshake (`KontrolProtocolControlSurface.java`):

```java
public static final int CC_HELLO   = 0x01;  // init handshake (+ ack)
public static final int CC_GOODBYE = 0x02;  // stop the protocol
public void initHandshake() { this.sendCommand(CC_HELLO, this.requiredVersion); }
// sendCommand -> this.output.sendCCEx(15, command, value);  // channel 16
```

So the handshake is **CC 0x01, value = requested protocol version (≤4), on
channel 16** to the DAW port; the device replies with CC 0x01. Shutdown sends
CC 0x02.

NIHIA SysEx header:

```java
byte[] NHIA_SYSEX_HEADER = { 0xF0, 0x00, 0x21, 0x09, 0x00, 0x00, 0x44, 0x43, 0x01, 0x00 };
// 00 21 09 = Native Instruments manufacturer ID; 44 43 = "DC"
```

State messages are `HEADER + [stateID, value, index, …] + F7` (track name
0x48, tempo 0x19, identity 0x07, …). **None of these are key-LED commands.**

`terminar/rebellion` documents the same protocol but via IPC to the NI agent
service (macOS/Windows only), and corroborates the LED color model as
"15 colors × 4 brightness steps".

## Light Guide message format (the real key-lighting protocol, from USB projects)

### MkII (ojacques/SynthesiaKontrol, USB HID)

VID `0x17cc`; PIDs `S49MK2=0x1610`, `S61MK2=0x1620`, `S88MK2=0x1630`.

```python
device.write([0xa0, 0x00, 0x00])     # init (enables light control)
device.write([0x81] + bufferC)       # 0x81 prefix + one palette byte per key
```

Key index = `midiNote + offset` (`S88 offset = -21`, i.e. note 21 (A0) → key
0). `0x00` = off.

### MK3 (tillt/KompleteSynthesia — the definitive MK3 reference, USB **bulk**)

```objc
const uint32_t kUSBDeviceInterfaceMK3         = 0x04;  // interface 4
const uint32_t kUSBDeviceInterfaceEndpointMK3 = 0x04;  // endpoint 4 (bulk OUT)

const uint8_t kKompleteKontrolLightGuidePrefixMK3[] =
  {0x93,0x02,0xCD,0x01,0x16,0x92,0xCD,0x01,0x51,0x81,0xCC,0xFC,0xDC,0x00,0x80}; // 15 bytes
const uint8_t kCommandLightGuideKeyCommandMK3   = 0x92;
const size_t  kKompleteKontrolLightGuideMessageSizeMK3 = 403;
```

Frame layout (403 bytes total):

```
[0..4)   little-endian u32 length = 399 (0x8F 0x01 0x00 0x00)
[4..19)  15-byte prefix (above)
[19..403) 128 × { 0x92, keyIndex, colorByte }         (384 bytes)
```

Per-key write:

```objc
_keys[key*3 + 0] = 0x92;          // command
_keys[key*3 + 1] = key;           // key index (0..127)
_keys[key*3 + 2] = color;         // palette byte (same table as MkII)
```

Sent via **USB bulk write** (`bulkWriteData`), not an HID SetReport.

MK3 init (also via HID SetReport, distinct from MkII's `0xA0 00 00`; author
notes it may be incomplete — KK software repeats it on an ~8 s interval):

```objc
const uint8_t kKompleteKontrolInitMK3[] = {0x06,0x00,0x00,0x00,0x93,0x02,0xcd,0x01,0x2c,0x90};
```

MK3 USB Product IDs (from `USBController.h`): **`S61MK3 = 0x2110` confirmed**;
`S49MK3 = 0x2100` and `S88MK3 = 0x2120` are placeholders in tillt's source.
**UPDATE:** `S88 MK3 = 0x2120` is now **confirmed** — enumeration on this rig
(`probe list`, 2026-07-11) reports `VID 0x17cc PID 0x2120`.

### Color encoding — palette index (identical MkII & MK3)

The color byte is **not RGB**; it's `colorBase + intensity`:

- `intensity = color & 0x03` (Low=0, Medium=1, High=2, Bright=3)
- `hue = (color >> 2) - 1` → 17 hues, `0x00` = off/black.

Hue bases (tillt `HIDController.h`, corroborated by SynthesiaKontrol
`color_scan.py` on an S61 MK2):

```
RED=0x04  ORANGE=0x08  YELLOW=0x10  GREEN=0x1C
BLUE=0x2C PURPLE=0x34  PINK=0x38    WHITE=0x44
```

Approx RGB per hue index (tillt `kMK2Palette`, on-screen preview only — the
device only accepts the single index byte):

```
0 red FF0000  1 FF3F00  2 orange FF7F00  3 amber FFCF00  4 yellow FFFF00
5 lime 7FFF00  6 green 00FF00  7 00FF7F  8 cyan 00FFFF   9 azure 007FFF
10 blue 0000FF 11 3F00FF 12 purple 7F00FF 13 pink FF00FF 14 FF007F
15 FF003F 16 white FFFFFF
```

## Transport feasibility over ALSA / PipeWire MIDI — the crux, confidence HIGH

You **cannot** drive the Light Guide from the ALSA/PipeWire MIDI ports. Every
working implementation uses raw USB (HID for MkI/MkII, **bulk on iface 4/ep 4**
for MK3, reachable only via libusb after claiming the interface). The DAW MIDI
port carries only the NIHIA protocol, which has **no** key-LED command.

Honest gap: the MK3 bulk payload internally *looks* MIDI-note-on-shaped
(`0x92 key color`), so one could speculate the firmware also accepts a note
stream on some MIDI endpoint — but **no examined project does this**, and tillt
had to move MK3 specifically to a USB *bulk* endpoint (away from the HID report
that works for MkII). Strong evidence the LEDs are not MIDI-addressable on MK3.
The `probe` example still fires MIDI note-ons at Main/DAW as an explicit
falsification test.

Practical Linux path (what this crate implements in `usb.rs`): libusb (via
`rusb`), VID `0x17cc`, claim interface 4, bulk-write the `length + prefix +
128×{0x92,key,color}` frame; palette byte per the table above. Confirm the S88
MK3 PID by enumeration.

## Sources

DrivenByMoss (git-moss, `master`):
- Tree: https://api.github.com/repos/git-moss/DrivenByMoss/git/trees/master?recursive=1
- https://raw.githubusercontent.com/git-moss/DrivenByMoss/master/src/main/java/de/mossgrabers/controller/ni/kontrol/mkii/controller/KontrolProtocolControlSurface.java
- .../mkii/controller/KontrolProtocol.java, KontrolProtocolColorManager.java, KontrolProtocolDeviceDescriptorV2.java, KontrolProtocolDeviceDescriptorV3.java
- .../mkii/KontrolProtocolControllerSetup.java
- https://raw.githubusercontent.com/git-moss/DrivenByMoss/master/src/main/java/de/mossgrabers/controller/ni/kontrol/mki/controller/Kontrol1LightGuide.java

ojacques/SynthesiaKontrol (`master`):
- https://raw.githubusercontent.com/ojacques/SynthesiaKontrol/master/SynthesiaKontrol.py
- https://raw.githubusercontent.com/ojacques/SynthesiaKontrol/master/color_scan.py

tillt/KompleteSynthesia (`main`, definitive MK3 reference):
- https://raw.githubusercontent.com/tillt/KompleteSynthesia/main/KompleteSynthesia/HIDController.h
- https://raw.githubusercontent.com/tillt/KompleteSynthesia/main/KompleteSynthesia/HIDController.m
- https://raw.githubusercontent.com/tillt/KompleteSynthesia/main/KompleteSynthesia/USBController.h
- https://raw.githubusercontent.com/tillt/KompleteSynthesia/main/KompleteSynthesia/USBController.m
- https://github.com/tillt/KompleteSynthesia/discussions/29
- https://www.native-instruments.com/forum/threads/programming-the-guide-lights.320806/ (init-byte source)

terminar/rebellion (NIHIA IPC, `main`):
- https://raw.githubusercontent.com/terminar/rebellion/main/README.md
