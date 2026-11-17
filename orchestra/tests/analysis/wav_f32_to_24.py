"""wav_f32_to_24.py <in.wav> <out.wav> : a 32-bit float WAV (KONTRA's
render, WAVE_FORMAT_EXTENSIBLE) as 24-bit PCM, which null_test.py reads."""
import struct
import sys
import wave

import numpy as np


def main():
    b = open(sys.argv[1], "rb").read()
    pos, ch, sr, data = 12, 2, 48000, b""
    while pos + 8 <= len(b):
        cid, n = b[pos:pos + 4], struct.unpack("<I", b[pos + 4:pos + 8])[0]
        if cid == b"fmt ":
            ch, sr = struct.unpack("<HI", b[pos + 10:pos + 16])
        elif cid == b"data":
            data = b[pos + 8:pos + 8 + n]
        pos += 8 + n + (n & 1)
    x = np.frombuffer(data, dtype="<f4")
    y = np.clip(np.round(x * 8388607.0), -8388608, 8388607).astype("<i4")
    w = wave.open(sys.argv[2], "wb")
    w.setnchannels(ch)
    w.setsampwidth(3)
    w.setframerate(sr)
    w.writeframes(y.view(np.uint8).reshape(-1, 4)[:, :3].tobytes())
    w.close()


if __name__ == "__main__":
    main()
