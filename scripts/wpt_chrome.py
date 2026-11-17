"""Run the pinned Chrome column using the WPT runner's server and JSONL protocol."""
import os
from pathlib import Path
import subprocess
from wpt_testharness import main

HOST = Path(__file__).resolve().parents[1] / "tools/parity_oracle/wpt_page.mjs"

if __name__ == "__main__":
    if not os.environ.get("PARITY_CHROME_PATH"):
        raise SystemExit("PARITY_CHROME_PATH is required")
    subprocess.run(["node", HOST, "--verify"], check=True)
    main(lambda url, timeout: ["node", HOST, url, str(timeout * 1000)])
