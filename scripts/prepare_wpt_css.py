"""Expand the pinned WPT source archive into a fresh directory for the CSS run."""
import argparse
import hashlib
import shutil
import tarfile
import tempfile
from pathlib import Path
from urllib.request import urlopen

from fetch_wpt_suite import PIN

SHA256 = "a638d730bda05b59e4914de3bb669d3800f1e692cdb6c9eb2eb9cd2015d76cbb"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--archive", type=Path, help="use the existing compressed download")
    args = parser.parse_args()
    if args.destination.exists():
        parser.error("destination must be a new directory")
    args.destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=args.destination.parent) as tmp:
        path = args.archive
        if path is None:
            path = Path(tmp, "wpt.tar.gz")
            url = f"https://codeload.github.com/web-platform-tests/wpt/tar.gz/{PIN}"
            with urlopen(url, timeout=120) as response, path.open("wb") as out:
                shutil.copyfileobj(response, out)
        with path.open("rb") as source:
            if hashlib.file_digest(source, "sha256").hexdigest() != SHA256:
                raise SystemExit("Unexpected pinned archive digest")
        with tarfile.open(path) as archive:
            for member in archive:
                # The archive links are outside the CSS suite and its support files.
                if member.isfile():
                    archive.extract(member, tmp, filter="data")
        Path(tmp, f"wpt-{PIN}").rename(args.destination)


if __name__ == "__main__":
    main()
