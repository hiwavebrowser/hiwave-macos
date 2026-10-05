#!/usr/bin/env python3
"""Shared summary for the counterbalanced A/B tools (ab2.py, ab_flag.py, ab2_summary.py).

A pair is {"order": "AB"|"BA", "ms": (a, b), "builds": (a, b)}; a value is None
when that load failed or logged nothing.

Until 2026-10-05 a pair whose two loads logged different build counts was
dropped before the summary. That is an exclusion on something the change under
test can cause: a change that makes a load take an extra layout build (or
skip one) has exactly its affected pairs removed and reads as neutral, and the
pairs that remain are no longer counterbalanced. So every complete pair now
counts in the headline, the equal-build subset is printed beside it with its
own AB/BA split, and the unequal pairs are listed by which arm built more.
"""
import statistics as st


def _ratios(pairs):
    return [p["ms"][1] / p["ms"][0] for p in pairs]


def _line(label, pairs):
    r = _ratios(pairs)
    ab = sum(p["order"] == "AB" for p in pairs)
    return (f"{label}: {len(pairs)} pairs ({ab} AB + {len(pairs) - ab} BA) | median B/A "
            f"{st.median(r):.3f} | below 1 in {sum(x < 1 for x in r)} | range "
            f"{min(r):.2f}-{max(r):.2f} | A med {st.median(p['ms'][0] for p in pairs)} "
            f"B med {st.median(p['ms'][1] for p in pairs)}")


def summarize(site, pairs, out=print):
    """Print the site's summary; returns the all-pairs median B/A (None without pairs)."""
    # Build counts come as ints from a live run and as text from a log; a log
    # line without them reads as equal.
    pairs = [dict(p, builds=tuple(int(b) if b not in (None, "") else 0 for b in p["builds"]))
             for p in pairs]
    complete = [p for p in pairs if all(v is not None and v > 0 for v in p["ms"])]
    failed = len(pairs) - len(complete)
    if not complete:
        out(f"{site}: no complete pairs ({failed} with a failed load)")
        return None
    out(_line(f"{site} all pairs", complete))
    equal = [p for p in complete if p["builds"][0] == p["builds"][1]]
    a_more = sum(p["builds"][0] > p["builds"][1] for p in complete)
    b_more = sum(p["builds"][0] < p["builds"][1] for p in complete)
    if len(equal) != len(complete):
        if equal:
            out(_line("   equal build counts only", equal))
        out(f"   build counts differ in {a_more + b_more} of {len(complete)} pairs: "
            f"A built more in {a_more}, B built more in {b_more}")
        if abs(a_more - b_more) >= 2:
            out("   ONE-SIDED: the difference between the arms may itself change the build "
                "count; the equal-build line leaves those loads out. Read the all-pairs line.")
    if failed:
        out(f"   {failed} pair(s) had a failed load and are in neither line")
    return st.median(_ratios(complete))
