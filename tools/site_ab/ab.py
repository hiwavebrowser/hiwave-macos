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


ANSI = re.compile(r'\x1b\[[0-9;]*m')


def failure_reason(rc, stdout, have_frame):
    """Why a capture gave no frame: the capture's own JSON status ('timeout',
    'error', ...), 'no frame' when it said ok, 'crash' when it printed no JSON."""
    if have_frame:
        return 'ok'
    try:
        status = json.loads(stdout.strip().splitlines()[-1]).get('status')
    except (ValueError, IndexError, AttributeError):
        return 'crash'
    if status == 'ok':
        return 'no frame'
    return str(status or 'crash')


def shape_source(src):
    """A script source as shape only: inline#N as is; a URL as host/…/file, no query."""
    if '://' not in src:
        return src
    rest = src.split('://', 1)[1].split('?', 1)[0].split('#', 1)[0]
    host, _, path = rest.partition('/')
    name = path.rstrip('/').rsplit('/', 1)[-1]
    return host + ('/…/' + name if name else '')


def stall_hint(stderr):
    """Where a failed capture was when it stopped, from the engine's info log
    (RUST_LOG=rustkit_engine=info): the last script it started and how many
    forced layouts it had run. '' when the log says nothing."""
    text = ANSI.sub('', stderr)
    started = re.findall(r'Running (?:page|inline) script\b.*?source=(\S+)', text)
    forced = text.count('Forced read builds the tree')
    if not started and not forced:
        return ''
    where = 'stalled in %s' % shape_source(started[-1]) if started else 'stalled'
    return where + (' after %d forced layouts' % forced if forced else '')


def arm_summary(runs, arm):
    """'A timeout 2/2', 'B ok 1/2 timeout 1/2': each reason with its count, in run order."""
    reasons = [r['reason'] for r in runs[arm::2]]
    order = []
    for x in reasons:
        if x not in order:
            order.append(x)
    return '%s %s' % ('AB'[arm], ' '.join('%s %d/%d' % (x, reasons.count(x), len(reasons)) for x in order))


def evidence(runs):
    """('full' | 'partial' | 'none', cross pairs with both frames)."""
    n = sum(1 for a, b in PAIRS if runs[a]['shot'] is not None and runs[b]['shot'] is not None)
    return ('full' if n == len(PAIRS) else 'partial' if n else 'none'), n


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
    run['reason'] = failure_reason(r.returncode, r.stdout, run['shot'] is not None)
    run['hint'] = '' if run['shot'] is not None else stall_hint(r.stderr)
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


def failure_note(runs, timeout_ms):
    """'A timeout 2/2, B timeout 2/2, limit 30 s; stalled in inline#10 after 140 forced layouts'."""
    note = '%s, %s, limit %g s' % (arm_summary(runs, 0), arm_summary(runs, 1), timeout_ms / 1000)
    # Runs stalled in the same place differ only in how far they got: one hint
    # per place, with the forced-layout counts as a range.
    places = {}
    for r in runs:
        if r['hint']:
            m = re.match(r'(.*) after (\d+) forced layouts$', r['hint'])
            where, n = (m.group(1), int(m.group(2))) if m else (r['hint'], None)
            places.setdefault(where, []).append(n)
    hints = []
    for where, counts in places.items():
        counts = [c for c in counts if c is not None]
        if not counts:
            hints.append(where)
        elif min(counts) == max(counts):
            hints.append('%s after %d forced layouts' % (where, counts[0]))
        else:
            hints.append('%s after %d–%d forced layouts' % (where, min(counts), max(counts)))
    return note + ('; ' + ' | '.join(hints) if hints else '')


def site_line(sid, runs, timeout_ms):
    """One table row. A site where no cross pair has both frames reads NO EVIDENCE (it
    cannot show a mover or certify its absence); one with some pairs missing reads
    PARTIAL. A missing pair or within-arm diff prints '—' instead of n/a."""
    shots = [r['shot'] for r in runs]
    failed = sum(1 for s in shots if s is None)
    col = lambda k: ','.join(r[k] for r in runs)  # noqa: E731
    pct = lambda v: '—' if v is None else '%.2f%%' % v  # noqa: E731
    kind, n = evidence(runs)
    rest = ('failed: %d  secs A,B,A,B %s  script records A,B,A,B %s'
            '  elapsed_ms %s  build_ms %s  script_errors %s  script_ms %s') % (
        failed, col('secs'), col('scripts'), col('elapsed'), col('build'), col('errs'), col('sms'))
    if kind == 'none':
        return '%-12s NO EVIDENCE (0 of 4 cross pairs: %s)  %s' % (sid, failure_note(runs, timeout_ms), rest)
    head = '%-12s within A %s  within B %s  across %s' % (
        sid, pct(diff(shots[0], shots[2])), pct(diff(shots[1], shots[3])),
        ' '.join(pct(diff(shots[a], shots[b])) for a, b in PAIRS))
    if kind == 'partial':
        head += '  PARTIAL %d of 4 cross pairs (%s)' % (n, failure_note(runs, timeout_ms))
    return head + '  ' + rest


def footer(results, timeout_ms):
    """Last table line: every site with no evidence, so a reader of the tail sees it.
    `results` is [(site id, runs)]."""
    none = [(sid, runs) for sid, runs in results if evidence(runs)[0] == 'none']
    return 'no evidence (%d of %d sites): %s' % (
        len(none), len(results),
        '; '.join('%s (%s)' % (sid, failure_note(runs, timeout_ms)) for sid, runs in none) or 'none')


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
    results = []

    def emit(line):
        print(line, flush=True)
        with open(table, 'a') as f:
            f.write(line + '\n')

    for site in sites:
        runs = [capture(binary, site, i, frames, a.timeout_ms)
                for i, binary in enumerate([a.binary_a, a.binary_b, a.binary_a, a.binary_b])]
        results.append((site['id'], runs))
        emit(site_line(site['id'], runs, a.timeout_ms))
    emit(footer(results, a.timeout_ms))
    return 0


if __name__ == '__main__':
    sys.exit(main())
