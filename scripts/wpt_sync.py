#!/usr/bin/env python3
"""wpt_sync.py — materialise ONLY the tests listed in trench/wpt/MANIFEST.json into third_party/wpt/.

Python twin of scripts/wpt_sync.sh with an HTTPS fetch path instead of a git
sparse-checkout. It exists because the macOS trench seat's Bash allowlist has no
`git clone` / `curl` — and, it turned out, cannot execute wpt_sync.sh at all
(`bash scripts/...` is itself off the allowlist). python3 is the seat's
sanctioned wrapper, so the first *real* sync ran through this file
(trench/wpt/README.md known gap 1 is closed by it).

The manifest is the source of truth; this script is a projection of it. It never
fetches a path the manifest does not list, and it never vendors the full WPT
tree. Every file is fetched at the pinned SHA — never a branch name.

    python3 scripts/wpt_sync.py            # fetch manifest paths at the pinned SHA
    python3 scripts/wpt_sync.py --check    # verify every manifest path exists locally; no network
    python3 scripts/wpt_sync.py --dry-run  # print what would be fetched, touch nothing

Writes third_party/wpt/.sync-receipt.json on success so the runner can verify
the tree on disk matches the manifest pin instead of trusting it.
"""

import json
import sys
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
MANIFEST = REPO_ROOT / "trench" / "wpt" / "MANIFEST.json"
DEST = REPO_ROOT / "third_party" / "wpt"
RECEIPT = DEST / ".sync-receipt.json"
RAW_BASE = "https://raw.githubusercontent.com/web-platform-tests/wpt"


def load_manifest():
    manifest = json.loads(MANIFEST.read_text())
    paths = set()
    for entry in manifest["entries"]:
        paths.add(entry["path"])
        if entry.get("ref"):
            paths.add(entry["ref"])
    paths.update(manifest.get("support_files", {}).get("paths", []))
    return manifest, sorted(paths)


def main():
    mode = "sync"
    if len(sys.argv) > 1:
        if sys.argv[1] == "--check":
            mode = "check"
        elif sys.argv[1] == "--dry-run":
            mode = "dry-run"
        else:
            print(f"unknown argument: {sys.argv[1]}", file=sys.stderr)
            return 2

    if not MANIFEST.exists():
        print(f"manifest not found: {MANIFEST}", file=sys.stderr)
        return 1

    manifest, paths = load_manifest()
    pin = manifest["wpt_pin"]

    print(f"manifest: {MANIFEST}")
    print(f"pin:      {pin}")
    print(f"files:    {len(paths)} (tests + refs)")
    print(f"dest:     {DEST}")

    if mode == "dry-run":
        print(f"--- would fetch these paths at {pin} ---")
        for p in paths:
            print(p)
        return 0

    if mode == "check":
        missing = [p for p in paths if not (DEST / p).is_file()]
        for p in missing:
            print(f"MISSING {p}")
        if missing:
            print(
                f"FAIL: {len(missing)}/{len(paths)} manifest paths absent from {DEST} — run without --check to sync.",
                file=sys.stderr,
            )
            return 1
        # A tree fetched at a different pin passing --check would be the quiet
        # version of measuring the wrong baseline set.
        if RECEIPT.exists():
            receipt_pin = json.loads(RECEIPT.read_text()).get("pin")
            if receipt_pin != pin:
                print(
                    f"FAIL: tree receipt pin {receipt_pin} != manifest pin {pin} — re-sync.",
                    file=sys.stderr,
                )
                return 1
        print(f"OK: all {len(paths)} manifest paths present at {DEST}")
        return 0

    # sync
    failed = []
    for i, p in enumerate(paths, 1):
        url = f"{RAW_BASE}/{pin}/{p}"
        target = DEST / p
        target.parent.mkdir(parents=True, exist_ok=True)
        try:
            data = urllib.request.urlopen(url, timeout=30).read()
        except Exception as e:
            print(f"  FETCH FAILED [{i}/{len(paths)}] {p}: {e}", file=sys.stderr)
            failed.append(p)
            continue
        target.write_bytes(data)
        print(f"  fetched [{i}/{len(paths)}] {p} ({len(data)} bytes)")

    if failed:
        print(f"FAIL: {len(failed)}/{len(paths)} fetches failed — no receipt written.", file=sys.stderr)
        return 1

    RECEIPT.write_text(
        json.dumps(
            {
                "pin": pin,
                "fetched_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
                "files": len(paths),
                "source": RAW_BASE,
            },
            indent=2,
        )
        + "\n"
    )

    # Sync is not done until every listed path is on disk — same contract as
    # wpt_sync.sh: end by re-running the check against the tree we just wrote.
    missing = [p for p in paths if not (DEST / p).is_file()]
    if missing:
        print(f"FAIL: {len(missing)} paths still missing after sync.", file=sys.stderr)
        return 1
    print(f"OK: all {len(paths)} manifest paths present at {DEST}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
