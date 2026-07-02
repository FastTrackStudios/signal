#!/usr/bin/env python3
"""Build an Omnisphere VST3 state chunk from a Multi XML (.mlt_omn content).

The chunk is a JUCE plugin state wrapper (verified against a real
`load_plugin --save-state` dump):
  bytes 0..4    "DAW3"
  bytes 4..8    u32le: length of everything after byte 12
  bytes 8..12   u32le: 999_999_999 (magic)
  bytes 12..24  u32le × 3: 0, 1, 0
  bytes 24..28  u32le: XML payload length (payload ends "… \\0")
  bytes 28..32  u32le: 0
  bytes 32..    XML payload
  then          20 zero bytes + "JUCEPrivateData\\0" + 3 zero bytes

Usage:
  omni_inject_state.py <multi.xml|.mlt_omn> <out_state.bin> [template_state.bin]
"""

import struct
import sys

TRAILER = b"\x00" * 20 + b"JUCEPrivateData\x00" + b"\x00" * 3


def build_state(xml: bytes, template: bytes | None) -> bytes:
    payload = xml.rstrip(b"\x00").rstrip() + b" \x00"
    trailer = TRAILER
    if template is not None:
        t_xml_len = struct.unpack_from("<I", template, 24)[0]
        trailer = template[32 + t_xml_len :]
    counted = (
        struct.pack("<IIIII", 0, 1, 0, len(payload), 0) + payload + trailer
    )
    return b"DAW3" + struct.pack("<II", len(counted), 999_999_999) + counted


def main() -> None:
    xml_path, out_path = sys.argv[1], sys.argv[2]
    template = open(sys.argv[3], "rb").read() if len(sys.argv) > 3 else None
    xml = open(xml_path, "rb").read()
    state = build_state(xml, template)
    open(out_path, "wb").write(state)
    print(f"{out_path}: {len(state)} bytes (xml {len(xml)})")


if __name__ == "__main__":
    main()
