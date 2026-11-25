"""The batch A/B table says when a site gave no evidence (T40).

cnn timed out at 30 s on all four captures for batches 45-52, and the table
showed `n/a` in every diff column and `failed: 4`: a site with no evidence
read like a quiet row. These tests drive ab.py with fake parity-capture
binaries (a frame, the hard-timeout JSON, a crash) and pin what the table says.
No network, no engine.
"""

import json
import os
import pathlib
import stat
import subprocess
import sys
import textwrap

import pytest

HERE = pathlib.Path(__file__).resolve().parent
AB = HERE.parent / 'ab.py'

# A fake parity-capture. Its behaviour comes from its own file name:
#   ok-<byte>    writes a 4x4 P6 frame of that grey level and prints status ok
#   timeout      prints the hard-timeout JSON (exit 3) after cnn-like stderr:
#                three inline scripts started, then 140 forced reads
#   urlstall     like timeout, but the last script is an external URL with a query
#   crash        exits 101 with nothing on stdout
#   cnnonly      timeout for a URL containing 'cnn', ok-255 for any other
FAKE = textwrap.dedent("""\
    #!{python}
    import json, pathlib, sys
    mode = pathlib.Path(sys.argv[0]).name
    args = sys.argv[1:]
    def arg(k):
        return args[args.index(k) + 1]
    tmo = int(arg('--timeout-ms'))
    if mode == 'cnnonly':
        mode = 'timeout' if 'cnn' in arg('--url') else 'ok-255'
    if mode.startswith('ok-'):
        level = int(mode[3:])
        pathlib.Path(arg('--dump-frame')).write_bytes(b'P6\\n4 4\\n255\\n' + bytes([level]) * 48)
        pathlib.Path(arg('--dump-scripts')).write_text(json.dumps({{'scripts': [{{'source': 'inline#1'}}]}}))
        print(json.dumps({{'status': 'ok', 'elapsed_ms': 1200, 'script_stats': {{'threw': 0, 'elapsed_ms': 300}}}}))
        sys.exit(0)
    if mode in ('timeout', 'urlstall'):
        esc = '\\x1b[2m'
        for n in range(1, 4):
            sys.stderr.write(esc + '2026-11-19T05:42:20Z\\x1b[0m INFO rustkit_engine: Running page script source=inline#%d bytes=10\\n' % n)
        if mode == 'urlstall':
            sys.stderr.write('2026-11-19T05:42:21Z INFO rustkit_engine: Running page script '
                             'source=https://cdn.example.test/a/b/bundle.min.js?user=secret&t=1 bytes=99\\n')
        else:
            sys.stderr.write('2026-11-19T05:42:21Z INFO rustkit_engine: Running page script source=inline#10 bytes=3274767\\n')
        for _ in range(140):
            sys.stderr.write('2026-11-19T05:42:22Z INFO relayout: rustkit_engine: Forced read builds the tree '
                             'id=EngineViewId(1) why=a write a layout can see is pending\\n')
        print(json.dumps({{'status': 'timeout', 'url': 'x', 'error': 'capture exceeded %d ms' % tmo}}))
        sys.exit(3)
    if mode == 'crash':
        sys.stderr.write('thread panicked\\n')
        sys.exit(101)
    raise SystemExit('unknown fake mode ' + mode)
    """)

SITES = [{'id': 'alpha', 'url': 'https://alpha.example.test/'},
         {'id': 'cnn', 'url': 'https://www.cnn.com/'}]


@pytest.fixture
def wt(tmp_path):
    (tmp_path / 'websuite').mkdir()
    (tmp_path / 'websuite' / 'realsite-top20.json').write_text(json.dumps({'sites': SITES}))
    bins = tmp_path / 'bins'
    bins.mkdir()
    for mode in ('ok-255', 'ok-0', 'timeout', 'urlstall', 'crash', 'cnnonly'):
        p = bins / mode
        p.write_text(FAKE.format(python=sys.executable))
        p.chmod(p.stat().st_mode | stat.S_IXUSR)
    return tmp_path


class Switch:
    """A binary that runs `first` on its first call and `rest` after (one capture fails, one arm)."""

    def __init__(self, wt, name, first, rest):
        self.path = wt / 'bins' / name
        self.path.write_text(textwrap.dedent("""\
            #!/bin/sh
            n=$(cat "{c}" 2>/dev/null || echo 0); echo $((n+1)) > "{c}"
            if [ "$n" = 0 ]; then exec "{first}" "$@"; else exec "{rest}" "$@"; fi
            """).format(c=self.path.with_suffix('.count'), first=wt / 'bins' / first, rest=wt / 'bins' / rest))
        self.path.chmod(0o755)


def run_ab(wt, a, b, *sites, timeout_ms=30000):
    out = wt / 'out'
    r = subprocess.run([sys.executable, str(AB), str(wt), 't40', str(wt / 'bins' / a), str(wt / 'bins' / b),
                        *sites, '--out', str(out), '--timeout-ms', str(timeout_ms)],
                       capture_output=True, text=True, timeout=120)
    assert r.returncode == 0, r.stderr
    table = (out / 'ab-t40.txt').read_text()
    assert r.stdout.strip() == table.strip()
    return table


def line_for(table, sid):
    rows = [l for l in table.splitlines() if l.split() and l.split()[0] == sid]
    assert len(rows) == 1, table
    return rows[0]


def test_a_site_that_times_out_on_both_arms_reads_no_evidence_not_na(wt):
    line = line_for(run_ab(wt, 'timeout', 'timeout', 'cnn'), 'cnn')
    assert 'NO EVIDENCE' in line
    assert 'n/a' not in line


def test_no_evidence_line_names_the_reason_count_and_limit(wt):
    line = line_for(run_ab(wt, 'timeout', 'timeout', 'cnn', timeout_ms=30000), 'cnn')
    assert 'A timeout 2/2' in line and 'B timeout 2/2' in line
    assert 'limit 30 s' in line


def test_no_evidence_line_says_where_the_capture_was_when_the_limit_hit(wt):
    line = line_for(run_ab(wt, 'timeout', 'timeout', 'cnn'), 'cnn')
    assert 'stalled in inline#10 after 140 forced layouts' in line


def test_a_stalled_external_script_is_named_by_host_and_file_without_its_query(wt):
    line = line_for(run_ab(wt, 'urlstall', 'urlstall', 'cnn'), 'cnn')
    assert 'stalled in cdn.example.test/…/bundle.min.js' in line
    assert 'secret' not in line and '?' not in line


def test_one_arm_failing_entirely_is_also_no_evidence(wt):
    line = line_for(run_ab(wt, 'ok-255', 'crash', 'cnn'), 'cnn')
    assert 'NO EVIDENCE' in line
    assert 'A ok 2/2' in line and 'B crash 2/2' in line
    assert 'n/a' not in line


def test_a_run_ends_with_a_no_evidence_footer_listing_each_such_site(wt):
    table = run_ab(wt, 'cnnonly', 'cnnonly')
    footer = table.strip().splitlines()[-1]
    assert footer.startswith('no evidence (1 of 2 sites):'), footer
    assert 'cnn' in footer and 'alpha' not in footer


def test_a_run_with_evidence_for_every_site_says_so_in_the_footer(wt):
    table = run_ab(wt, 'ok-255', 'ok-0')
    assert table.strip().splitlines()[-1] == 'no evidence (0 of 2 sites): none'


def test_a_full_site_line_keeps_its_numbers(wt):
    line = line_for(run_ab(wt, 'ok-255', 'ok-0', 'alpha'), 'alpha')
    assert 'within A 0.00%  within B 0.00%  across 100.00% 100.00% 100.00% 100.00%  failed: 0' in line
    assert 'NO EVIDENCE' not in line and 'PARTIAL' not in line


def test_one_failed_capture_is_partial_with_the_pairs_left(wt):
    Switch(wt, 'b-once', 'timeout', 'ok-0')
    line = line_for(run_ab(wt, 'ok-255', 'b-once', 'alpha'), 'alpha')
    assert 'PARTIAL 2 of 4 cross pairs' in line
    assert 'B timeout 1/2' in line
    # the pairs that exist keep their numbers; the missing ones say why, not n/a
    assert 'n/a' not in line
    assert '100.00%' in line


def test_runs_stalled_in_one_place_collapse_to_a_forced_layout_range():
    sys.path.insert(0, str(AB.parent))
    import ab
    runs = [{'shot': None, 'reason': 'timeout', 'hint': 'stalled in inline#10 after %d forced layouts' % n}
            for n in (100, 104, 96, 101)]
    note = ab.failure_note(runs, 30000)
    assert note == 'A timeout 2/2, B timeout 2/2, limit 30 s; stalled in inline#10 after 96–104 forced layouts'
    runs[3]['hint'] = 'stalled in inline#2'
    assert ab.failure_note(runs, 30000).endswith(
        'stalled in inline#10 after 96–104 forced layouts | stalled in inline#2')
