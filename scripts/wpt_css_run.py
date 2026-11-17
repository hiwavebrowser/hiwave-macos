"""Run every pinned CSS manifest URL over loopback and write local JSONL."""
import argparse
import concurrent.futures
import contextlib
import json
import logging
import uuid
import subprocess
import sys
from pathlib import Path

from wpt_chrome import HOST, page_command
from wpt_testharness import run_page


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("inventory", type=Path)
    parser.add_argument("host", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--jobs", type=int, default=4)
    args = parser.parse_args()
    sys.path.insert(0, str(args.root.resolve() / "tools"))
    import localpaths
    from serve.serve import ConfigBuilder, RoutesBuilder
    from wptserve.utils import get_port
    from wptserve import WebTestHttpd, handlers, stash

    subprocess.run(["node", HOST, "--verify"], check=True)
    reporter = Path(__file__).with_name("wpt_report.js").read_bytes()
    harness = (args.root / "resources/testharness.js").read_bytes() + b"\n;" + reporter
    routes = RoutesBuilder()
    routes.add_handler("GET", "/resources/testharness.js",
                       handlers.handler(lambda req, resp: ([(b"Content-Type", b"text/javascript")], harness)))
    port = get_port()
    builder = ConfigBuilder(logging.getLogger("wpt"), browser_host="127.0.0.1",
                            server_host="127.0.0.1", doc_root=str(args.root.resolve()))
    builder.ports = {**builder.ports, "http": [port, "auto"]}
    with builder as config:
        server = WebTestHttpd(port=port, doc_root=str(args.root.resolve()),
                              routes=routes.get_routes(), config=config)

    def measure(page):
        url = f"http://127.0.0.1:{server.port}{page}"
        chrome = run_page(page_command(url, 60), page, 65)
        hw = run_page([args.host, "--wpt", "--url", url, "--timeout-ms", "60000"], page, 65)
        return {"page": page, "chrome": chrome, "hiwave": hw}

    pages = json.loads(args.inventory.read_text())
    done = set()
    if args.output.exists():
        with args.output.open(encoding="utf8") as results:
            done.update(json.loads(line)["page"] for line in results)
    with stash.StashServer(("127.0.0.1", 0), authkey=str(uuid.uuid4())), contextlib.ExitStack() as cleanup:
        server.start()
        cleanup.callback(server.stop)
        with args.output.open("a", encoding="utf8", newline="\n") as out:
            with concurrent.futures.ThreadPoolExecutor(args.jobs) as workers:
                tasks = [workers.submit(measure, page) for page in pages if page not in done]
                for task in concurrent.futures.as_completed(tasks):
                    print(json.dumps(task.result()), file=out, flush=True)


if __name__ == "__main__":
    main()
