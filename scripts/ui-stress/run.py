#!/usr/bin/env python3
"""Summarize dioxus-test samples. This script never generates input."""
import argparse
import json
import math
from pathlib import Path
import statistics
import subprocess


def distribution(values):
    if not values:
        return None
    values = sorted(values)
    percentile = lambda p: values[min(len(values) - 1, math.ceil(p * len(values)) - 1)]
    return dict(count=len(values), mean=statistics.fmean(values), p50=percentile(.50),
                p95=percentile(.95), p99=percentile(.99), worst=values[-1])


def summarize(data):
    samples = data['samples']
    if not samples:
        raise ValueError('No measured DOM frames')
    budget = data['budget_ms']
    phases = {}
    for phase in dict.fromkeys(s['phase'] for s in samples):
        selected = [s for s in samples if s['phase'] == phase]
        phases[phase] = {stage: distribution([s[stage] for s in selected])
                         for stage in ('event_ms', 'update_ms', 'layout_ms', 'total_ms')}
        phases[phase]['over_budget'] = sum(s['total_ms'] > budget for s in selected)
        # Frames on which the gesture actually moved the pane. A phase
        # that stops moving — a camera at the end of the song, a lane
        # stack that fits — reports beautiful medians for an idle window,
        # so the share is printed next to them and a phase that moved on
        # fewer than half its frames is called out rather than averaged in.
        recorded = data.get('effective_frames')
        phases[phase]['effective'] = None if recorded is None else recorded.get(phase, 0)
        # The whole point, in the units the goal is stated in: N frames
        # at 120 Hz take N/120 seconds. What they actually took, over
        # what they may take, is how far off the target is — and unlike a
        # median it cannot be improved by frames that did nothing.
        phases[phase]['measured_s'] = sum(s['total_ms'] for s in selected) / 1000
        phases[phase]['budget_s'] = len(selected) * budget / 1000
    return {**{k: v for k, v in data.items() if k != 'samples'}, 'phases': phases,
            'dom_budget_pass': all(p['over_budget'] == 0 for p in phases.values())}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--enforce', action='store_true')
    args = parser.parse_args()
    report = summarize(json.loads((args.directory / 'samples.json').read_text()))
    report['git_revision'] = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    report['git_status'] = subprocess.check_output(['git', 'status', '--short'], text=True)
    (args.directory / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    lines = ['# Workstation DOM stress benchmark', '', report['measurement'], '',
             f"120 Hz DOM budget: {report['budget_ms']:.3f} ms. Pass: {report['dom_budget_pass']}", '',
             'This is a necessary CPU budget check, not a display FPS measurement. GPU paint and presentation require separate measurement.', '',
             '| Phase | p50 ms | p95 ms | p99 ms | Worst ms | Over budget | Moved |',
             '|---|---:|---:|---:|---:|---:|---:|']
    idle = []
    for phase, result in report['phases'].items():
        d = result['total_ms']
        moved = result['effective']
        share = '?' if moved is None else f"{moved}/{d['count']}"
        if moved is not None and moved * 2 < d['count']:
            idle.append(f'{phase} ({share})')
        lines.append(f"| {phase} | {d['p50']:.2f} | {d['p95']:.2f} | {d['p99']:.2f} | {d['worst']:.2f} | {result['over_budget']}/{d['count']} | {share} |")
    slowest = max(report['phases'].items(), key=lambda kv: kv[1]['measured_s'] / kv[1]['budget_s'])
    lines += ['', '| Phase | frames | measured | at 120 Hz | over by |', '|---|---:|---:|---:|---:|']
    for phase, result in report['phases'].items():
        over = result['measured_s'] / result['budget_s']
        lines.append(f"| {phase} | {result['total_ms']['count']} | {result['measured_s']:.2f} s "
                     f"| {result['budget_s']:.2f} s | {over:.1f}x |")
    lines += ['', f"Worst: {slowest[0]} — "
                  f"{slowest[1]['measured_s']:.1f} s of work for "
                  f"{slowest[1]['budget_s']:.1f} s of frames."]
    if idle:
        lines += ['', 'NOT A RESULT — these phases spent most of their frames moving nothing, '
                      'so their timings are an idle window rather than the workload: '
                      + ', '.join(idle) + '.']
    lines += ['', f"Profile: {report['profile']}; viewport: {report['viewport']}; CPU: {report['cpu']}; load: {report['loadavg']}", '', 'Per-stage distributions are in report.json; every sample is retained in samples.json.']
    (args.directory / 'report.md').write_text('\n'.join(lines) + '\n')
    print('\n'.join(lines))
    if args.enforce and not report['dom_budget_pass']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
