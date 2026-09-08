#!/usr/bin/env python3
"""Extract 1 kHz applied-gain references, preserving the recorded stimulus and encoding.
Run from repo root. Bulk input defaults to the external Plugin Analysis archive.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct

parser = argparse.ArgumentParser()
parser.add_argument('--archive', type=Path, default=Path('/run/media/AudioHaven/Plugin Analysis'))
parser.add_argument('--output', type=Path, default=Path('features/fx/comp/comp-dsp/tests/fixtures/la2a_release.csv'))
args = parser.parse_args()
root = args.archive / 'UADx LA-2A Gray/captures/release'
meta_path = root / 'metadata.json'
meta = json.loads(meta_path.read_text())
# These historical files predate explicit quantization ranges. This fallback
# is the original writer's format, not the new writer's -60..+40 dB range.
lo = meta.get('store_min_db', -48.0)
hi = meta.get('store_max_db', 6.0)
frequency = meta['frequencies'].index(1000.0)
cycle_ms = meta['time_high_ms'] + meta['time_low_ms']
start = round(cycle_ms / meta['row_ms'])
high = round(meta['time_high_ms'] / meta['row_ms'])
end = round(2 * cycle_ms / meta['row_ms'])
lines = ['# Applied output gain, not gain reduction. Positive values include makeup.',
         '# Metadata SHA256: ' + hashlib.sha256(meta_path.read_bytes()).hexdigest(),
         '# stimulus: ' + json.dumps({k: meta[k] for k in ['sample_rate', 'gain_high_db', 'gain_low_db', 'time_high_ms', 'time_low_ms', 'row_ms', 'settle_ms', 'latency_samples']}),
         f'# quantization: {lo}..{hi} dB; one code step = {(hi-lo)/255:.9f} dB',
         '# peak_reduction,time_since_release_ms,applied_gain_db']
for scenario in meta['scenarios']:
    name = re.sub('[^A-Za-z0-9_-]', '_', scenario['name'])
    path = root / (name + '.bin')
    blob = path.read_bytes()
    count, rows = struct.unpack('<II', blob[:8])
    assert count == len(meta['frequencies']) and len(blob) == 8 + count * rows
    row = blob[8 + frequency * rows:8 + (frequency + 1) * rows]
    knob = scenario['params'][0]['value']
    lines.append('# ' + name + ' SHA256: ' + hashlib.sha256(blob).hexdigest())
    for offset in [-100, -50, -1, *range(0, 201, 5), *range(225, 1501, 25), 2000, 3000, 3999]:
        index = start + high + round(offset / meta['row_ms'])
        assert index < min(end, rows)
        lines.append(f'{knob:.15g},{offset},{lo+row[index]*(hi-lo)/255:.9f}')
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text('\n'.join(lines)+'\n')
print(f'Wrote {args.output}')
