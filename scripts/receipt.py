#!/usr/bin/env python3
"""receipt.py — the provenance block a closure PR carries (Z phase, 2026-10-02).

Emits the fields the Z plan requires, from what the machine already knows
(git, cargo, the binary, the host). Nothing is typed by hand, so "same binary"
and "all tests green" are facts a reader can check, not prose.

    python3 scripts/receipt.py --package D0 --binary target/parity/parity-capture \
        [--base <sha>] [--campaign parity-baseline/parity_test_results.json] \
        [--runs <file>...] [--note "..."] [--markdown] [--host-paths]

The output goes into public PR bodies and comments, so it carries no path of
the machine that ran it: a file inside the repository prints repo-relative, any
other file prints as its basename, and absolute paths typed into --note get the
same treatment. --host-paths keeps the paths as given, for local use only.

Runs on macOS, Windows and Linux; every field it cannot determine is printed
as null rather than guessed.
"""
import argparse, datetime, hashlib, json, os, platform, re, shutil, subprocess, sys
from pathlib import Path


def sh(*cmd, cwd=None):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, cwd=cwd, timeout=60).stdout.strip() or None
    except Exception:
        return None


def sha256(path):
    try:
        h = hashlib.sha256()
        with open(path, "rb") as f:
            for chunk in iter(lambda: f.read(1 << 20), b""):
                h.update(chunk)
        return h.hexdigest()
    except Exception:
        return None


def public_path(path, root):
    """Repo-relative for a file inside the repository, the basename otherwise."""
    full = Path(os.path.abspath(str(path)))
    root = Path(root).resolve()
    for cand in (full, full.resolve()):
        try:
            return cand.relative_to(root).as_posix()
        except ValueError:
            pass
    return full.name


# An absolute path in free text: POSIX with at least one directory (so "/60" and
# "and/or" are left alone, and the "//host/..." of a URL is not a path), a "~/"
# path, or a Windows drive path. A path with a space in it is cut at the space.
ABS_PATH = re.compile(r"(?<![\w.:/\\~])~?/(?:[\w.@+~-]+/)+[\w.@+~-]*|(?<![A-Za-z])[A-Za-z]:[\\/](?:[\w.@+~-]+[\\/])*[\w.@+~-]*")


def public_text(text, root):
    def one(m):
        tok = m.group(0)
        body = tok.rstrip(".")
        if re.match(r"[A-Za-z]:", body):
            return re.split(r"[\\/]", body.rstrip("\\/"))[-1] + tok[len(body):]
        return public_path(os.path.expanduser(body), root) + tok[len(body):]
    return ABS_PATH.sub(one, text.replace("file://", "")) if text else text


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--package", required=True)
    ap.add_argument("--binary", help="the measured binary (parity-capture)")
    ap.add_argument("--base", help="base SHA the candidate is measured against")
    ap.add_argument("--campaign", help="parity_test results JSON for the candidate")
    ap.add_argument("--runs", nargs="*", default=[], help="raw run/A-B files to hash and list")
    ap.add_argument("--profile", help="cargo profile used (parity|release)")
    ap.add_argument("--features", default=None)
    ap.add_argument("--note", default=None)
    ap.add_argument("--markdown", action="store_true")
    ap.add_argument("--host-paths", action="store_true", help="keep paths as given (local use; never post this output)")
    a = ap.parse_args()

    root = Path(sh("git", "rev-parse", "--show-toplevel") or ".")
    head = sh("git", "rev-parse", "HEAD", cwd=root)
    dirty = bool(sh("git", "status", "--porcelain", "--untracked-files=no", cwd=root))
    toolchain = sh("rustc", "-V")
    cargo = sh("cargo", "-V")
    target = sh("rustc", "-vV")
    target = next((l.split(":", 1)[1].strip() for l in (target or "").splitlines() if l.startswith("host:")), None)
    lock_hash = sha256(root / "Cargo.lock")
    binary = Path(a.binary).resolve() if a.binary else None
    shown = (lambda p: str(p)) if a.host_paths else (lambda p: public_path(p, root))
    text = (lambda t: t) if a.host_paths else (lambda t: public_text(t, root))
    rec = {
        "work_package": a.package,
        "base_sha": a.base,
        "candidate_sha": head,
        "dirty_state": dirty,
        "repo": sh("git", "config", "--get", "remote.origin.url", cwd=root),
        "branch": sh("git", "rev-parse", "--abbrev-ref", "HEAD", cwd=root),
        "cargo_lock_sha256": lock_hash,
        "binary": shown(binary) if binary else None,
        "binary_sha256": sha256(binary) if binary else None,
        "binary_mtime_utc": datetime.datetime.utcfromtimestamp(binary.stat().st_mtime).isoformat() + "Z" if binary and binary.exists() else None,
        "toolchain": toolchain,
        "cargo": cargo,
        "target": target,
        "profile": a.profile or ("parity" if binary and "parity" in binary.parts else "release" if binary and "release" in binary.parts else None),
        "features": a.features,
        "rustflags": os.environ.get("RUSTFLAGS"),
        "effective_engine_flags": {k: v for k, v in os.environ.items() if k.startswith("RUSTKIT_")},
        "host": platform.node(),
        "os": platform.platform(),
        "cpu": platform.processor() or sh("sysctl", "-n", "machdep.cpu.brand_string"),
        "sccache": bool(shutil.which("sccache")),
        "chrome_version": None,
        "campaign": None,
        "raw_runs": [],
        "utc": datetime.datetime.utcnow().isoformat() + "Z",
        "note": text(a.note),
    }
    meta = root / "baselines" / "metadata.json"
    if meta.exists():
        try:
            m = json.load(open(meta))
            rec["chrome_version"] = m.get("chrome_version") or m.get("chrome") or m.get("version")
        except Exception:
            pass
    if a.campaign and Path(a.campaign).exists():
        try:
            c = json.load(open(a.campaign))
            r = c.get("results", [])
            vals = [x["pixel"]["diffPercent"] for x in r if x.get("pixel") and x["pixel"].get("diffPercent") is not None]
            rec["campaign"] = {"file": shown(a.campaign), "sha256": sha256(a.campaign), "cases": len(r),
                               "passed": c.get("passed"), "failed": c.get("failed"),
                               "mean_diff_pct": round(sum(vals) / len(vals), 4) if vals else None,
                               "timestamp": c.get("timestamp")}
        except Exception as e:
            rec["campaign"] = {"file": shown(a.campaign), "error": text(str(e))}
    for f in a.runs:
        p = Path(f)
        rec["raw_runs"].append({"file": shown(f), "sha256": sha256(p) if p.exists() else None, "bytes": p.stat().st_size if p.exists() else None})

    if a.markdown:
        print("## Receipt (`scripts/receipt.py`)\n")
        print("```json")
        print(json.dumps(rec, indent=1, sort_keys=True))
        print("```")
    else:
        print(json.dumps(rec, indent=1, sort_keys=True))


if __name__ == "__main__":
    main()
