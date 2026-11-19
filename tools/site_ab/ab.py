#!/usr/bin/env python3
"""All-site A/B of two parity-capture binaries on the live site list (the batch A/B).

    ab.py <worktree> <tag> <binary A> <binary B> [site ...] [--out DIR] [--timeout-ms N]

Each site is captured A, B, A, B (1280x800, 30 s limit by default). A pixel
differs when any channel is more than 8 apart. Per site, one line: the
difference inside each arm, the four cross pairs, failures, seconds,
script-log sizes, and each run's elapsed_ms, build_ms, script errors and
script ms. Lines are appended to <out>/ab-<tag>.txt; frames, layouts, stdout
JSON and stderr go to <out>/x-<tag>/ (abinputs reads the layouts).

Sites come from <worktree>/websuite/realsite-top20.json; with no site
arguments every site in that list runs. Binaries must be absolute paths.

This is the batch's scratch ab.py (batch71 copy) moved into the repo, so the
batch and the lanes run one reviewed copy.
"""
import argparse
import json
import os
import pathlib
import re
import subprocess
import sys
import time

import numpy as np

PAIRS = [(0, 1), (0, 3), (2, 1), (2, 3)]


def ppm(path):
    data = path.read_bytes()
    # P6, width, height, maxval, then ONE whitespace byte and the pixels (which
    # may themselves start with a whitespace byte: do not split past maxval).
    magic, w, h, rest = data.split(maxsplit=3)
    w, h = int(w), int(h)
    pixels = rest[len(rest.split(maxsplit=1)[0]) + 1:]
    return np.frombuffer(pixels[:w * h * 3], dtype=np.uint8).astype(np.int16).reshape(h, w, 3)


def diff(a, b):
    if a is None or b is None:
        return None
    if a.shape != b.shape:
        return 100.0
    return float((np.abs(a - b).max(axis=2) > 8).mean() * 100)


def fmt(v):
    return 'n/a' if v is None else '%.2f' % v


def capture(binary, site, i, frames, timeout_ms):
    """One capture. Returns a dict with the frame (or None) and the run's numbers."""
    sid = site['id']
    frame = frames / ('%s-%d.ppm' % (sid, i))
    log = frames / ('%s-%d.scripts.json' % (sid, i))
    out = frames / ('%s-%d.stdout.json' % (sid, i))
    for p in (frame, log, out):
        if p.exists():
            p.unlink()
    t = time.time()
    r = subprocess.run([binary, '--url', site['url'], '--width', '1280', '--height', '800',
                        '--timeout-ms', str(timeout_ms), '--dump-frame', str(frame), '--dump-scripts', str(log),
                        '--dump-layout', str(frames / ('%s-%d.layout.json' % (sid, i)))],
                       capture_output=True, text=True, env=dict(os.environ, RUST_LOG='rustkit_engine=info'))
    run = {'secs': '%.1f' % (time.time() - t)}
    out.write_text(r.stdout)
    (frames / ('%s-%d.stderr.txt' % (sid, i))).write_text(r.stderr)
    # build_ms of the engine's relayout log lines; the max is the initial document build.
    ms = [float(m) for m in re.findall(r'build_ms=([\d.]+)', r.stderr)]
    run['build'] = '%.1f/%d' % (max(ms), len(ms)) if ms else '-'
    run['shot'] = ppm(frame) if frame.exists() else None
    try:
        records = json.loads(log.read_text())
        records = records.get('scripts', records) if isinstance(records, dict) else records
        run['scripts'] = str(len(records))
    except (OSError, ValueError):
        run['scripts'] = '-'
    try:
        d = json.loads(r.stdout)
        run['elapsed'] = str(d.get('elapsed_ms'))
        ss = d.get('script_stats') or {}
        run['errs'], run['sms'] = str(ss.get('threw', '-')), str(ss.get('elapsed_ms', '-'))
    except ValueError:
        run['elapsed'], run['errs'], run['sms'] = '-', '-', '-'
    return run


def site_line(sid, runs, timeout_ms):
    shots = [r['shot'] for r in runs]
    failed = sum(1 for s in shots if s is None)
    col = lambda k: ','.join(r[k] for r in runs)  # noqa: E731
    return ('%-12s within A %s%%  within B %s%%  across %s  failed: %d  secs A,B,A,B %s  script records A,B,A,B %s'
            '  elapsed_ms %s  build_ms %s  script_errors %s  script_ms %s') % (
        sid, fmt(diff(shots[0], shots[2])), fmt(diff(shots[1], shots[3])),
        ' '.join(fmt(diff(shots[a], shots[b])) + '%' for a, b in PAIRS),
        failed, col('secs'), col('scripts'), col('elapsed'), col('build'), col('errs'), col('sms'))


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    ap.add_argument('worktree', type=pathlib.Path)
    ap.add_argument('tag')
    ap.add_argument('binary_a')
    ap.add_argument('binary_b')
    ap.add_argument('sites', nargs='*')
    ap.add_argument('--out', type=pathlib.Path, help='output dir (default <worktree>/scratch)')
    ap.add_argument('--timeout-ms', type=int, default=30000)
    a = ap.parse_args(argv)
    out = a.out or a.worktree / 'scratch'
    out.mkdir(parents=True, exist_ok=True)
    sites = json.loads((a.worktree / 'websuite/realsite-top20.json').read_text())['sites']
    if a.sites:
        sites = [s for s in sites if s['id'] in a.sites]
    frames = out / ('x-' + a.tag)
    frames.mkdir(exist_ok=True)
    table = out / ('ab-%s.txt' % a.tag)
    for site in sites:
        runs = [capture(binary, site, i, frames, a.timeout_ms)
                for i, binary in enumerate([a.binary_a, a.binary_b, a.binary_a, a.binary_b])]
        line = site_line(site['id'], runs, a.timeout_ms)
        print(line, flush=True)
        with open(table, 'a') as f:
            f.write(line + '\n')
    return 0


if __name__ == '__main__':
    sys.exit(main())
