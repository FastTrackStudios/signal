#!/usr/bin/env python3
"""Put two benchmark runs side by side. This script never generates input.

A performance claim is a *difference*, and a difference read off two
tables in two scrollbacks is how a contended machine gets mistaken for an
optimization. This prints both runs' per-stage medians and the delta, and
it prints each run's load average next to them, because a run taken under
a load of 90 is not comparable to one taken under a load of 20 however
green the arrow looks.
"""
import argparse
import json
import statistics
from pathlib import Path


def load(directory: Path):
    """Per-phase stage medians, from the raw samples rather than report.json.

    The raw journal is the artifact that always exists; report.json is
    only there once someone has run `run.py` over the directory.
    """
    data = json.loads((directory / 'samples.json').read_text())
    grouped: dict[str, list[dict]] = {}
    for sample in data['samples']:
        grouped.setdefault(sample['phase'], []).append(sample)
    phases: dict[str, dict[str, float]] = {}
    for phase, samples in grouped.items():
        held = {stage: statistics.median(s[stage] for s in samples)
                for stage in ('event_ms', 'update_ms', 'layout_ms', 'total_ms')}
        held['frames'] = len(samples)
        held['over_budget'] = sum(s['total_ms'] > data['budget_ms'] for s in samples)
        phases[phase] = held
    return data, phases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('baseline', type=Path)
    parser.add_argument('candidate', type=Path)
    parser.add_argument('--stage', default='total_ms',
                        choices=('event_ms', 'update_ms', 'layout_ms', 'total_ms'))
    args = parser.parse_args()

    base_run, base = load(args.baseline)
    cand_run, cand = load(args.candidate)
    stage = args.stage

    def context(run, directory):
        nodes = run.get('dom_nodes') or {}
        counted = ' '.join(f'{k}={v}' for k, v in nodes.items())
        return (f"{directory}  profile={run['profile']} frames={run['frames_per_phase']}"
                f" phase={run['selected_phase'] or 'all'}"
                f" load={run['loadavg'].split()[0]}"
                + (f"\n           nodes: {counted}" if counted else ''))

    print(f"baseline   {context(base_run, args.baseline)}")
    print(f"candidate  {context(cand_run, args.candidate)}")
    if base_run['project'] != cand_run['project'] or base_run['viewport'] != cand_run['viewport']:
        print('WARNING: different project or viewport — these are not comparable')
    if base_run['profile'] != cand_run['profile']:
        print('WARNING: different profile — these are not comparable')
    print()
    print(f"| Phase | base {stage} | cand {stage} | delta | over budget |")
    print('|---|---:|---:|---:|---:|')
    for phase in base:
        if phase not in cand:
            continue
        b, c = base[phase][stage], cand[phase][stage]
        delta = (c - b) / b * 100 if b else 0.0
        budget = (f"{base[phase]['over_budget']}/{base[phase]['frames']}"
                  f" → {cand[phase]['over_budget']}/{cand[phase]['frames']}")
        print(f"| {phase} | {b:.2f} | {c:.2f} | {delta:+.1f}% | {budget} |")


if __name__ == '__main__':
    main()
