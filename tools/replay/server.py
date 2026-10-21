#!/usr/bin/env python3
"""Deterministic HAR replay for HiWave and Chrome.

Matches a request by method plus a normalised URL (see normalize.py). The
nth request for a key gets the nth recorded response, then the last one
again. Recorded response headers are sent, hop-by-hop headers are not, and
Content-Length is the body actually written. The HAR entry's `time`
(milliseconds) is slept, multiplied by --time-scale (0 disables the wait).

HiWave reaches this server through HIWAVE_REPLAY (an HTTP proxy URL). It
sends X-Original-Url. Chrome reaches it with --proxy-server: absolute-form
HTTP requests, and CONNECT for HTTPS, which this server answers with a
one-off certificate (--ignore-certificate-errors on the Chrome side).

A miss is logged as `MISS METHOD url` and answered 404. Cookie and
Authorization values are never written to that log.

redditstatic ``js/concat`` URLs are built at runtime, so the chunk list
often matches no recorded entry. Those requests are answered by splitting
recorded concat bodies into ``SML.dm`` chunk registrations (see concat.py)
or, when that split is not exact, by the smallest set of recorded bodies
that covers the requested chunks. The strategy is logged as ``CONCAT split``
or ``CONCAT cover``. A chunk that is still absent is logged as
``CONCAT missing <id>``. Cookie values are not part of that log.

No recorded HAR belongs in the repo. Use fixtures/synthetic.har for tests.
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import ssl
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Dict, List, Optional, Tuple
from urllib.parse import urlparse

from concat import ConcatBody, assemble, chunk_ids, split_body
from normalize import normalize_url

def _entry_text(entry: dict) -> str:
    response = entry.get("response") or {}
    content = response.get("content") or {}
    encoding = str(content.get("encoding") or "").lower()
    text = content.get("text") or ""
    if encoding == "base64" and isinstance(text, str) and text:
        try:
            return base64.b64decode(text).decode("utf-8", errors="replace")
        except Exception:
            return text
    if isinstance(text, str):
        return text
    return bytes(text).decode("utf-8", errors="replace")


HOP_BY_HOP = {
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
    "content-length",
}

class HarArchive:
    def __init__(self, data: dict):
        self.by_key: Dict[Tuple[str, str], List[dict]] = {}
        self.cursor: Dict[Tuple[str, str], int] = {}
        self.misses: List[str] = []
        self._lock = threading.Lock()
        self._notes: List[str] = []
        self.concat_slices: Dict[str, str] = {}
        self.concat_bodies: List[ConcatBody] = []
        log = data.get("log", data)
        for entry in log.get("entries", []):
            request = entry.get("request") or {}
            method = str(request.get("method") or "GET").upper()
            url = str(request.get("url") or "")
            key = (method, normalize_url(url))
            self.by_key.setdefault(key, []).append(entry)
            if method == "GET":
                self._index_concat(url, entry)

    def _index_concat(self, url: str, entry: dict) -> None:
        ids = chunk_ids(url)
        if not ids:
            return
        text = _entry_text(entry)
        body = ConcatBody(
            ids=ids,
            text=text,
            time_ms=float(entry.get("time") or 0),
            entry=entry,
        )
        self.concat_bodies.append(body)
        pieces = split_body(text, ids)
        if pieces is None:
            return
        for chunk_id, piece in pieces.items():
            self.concat_slices.setdefault(chunk_id, piece)

    @classmethod
    def from_file(cls, path: str | Path) -> "HarArchive":
        with open(path, "r", encoding="utf-8", errors="replace") as handle:
            return cls(json.load(handle))

    def take(self, method: str, url: str) -> Tuple[Optional[dict], List[str]]:
        """Return the HAR entry (or a concat assembly) and log lines for this request.

        Log lines never include cookie or authorization values.
        """
        key = (method.upper(), normalize_url(url))
        with self._lock:
            entries = self.by_key.get(key)
            if entries:
                index = self.cursor.get(key, 0)
                self.cursor[key] = index + 1
                return entries[min(index, len(entries) - 1)], []
            assembled, notes = self._assemble_concat(url)
            if assembled is not None:
                self._notes.extend(notes)
                return assembled, notes
            self.misses.append(f"MISS {key[0]} {key[1]}")
            self._notes.extend(notes)
            return None, notes

    def _assemble_concat(self, url: str) -> Tuple[Optional[dict], List[str]]:
        requested = chunk_ids(url)
        if not requested:
            return None, []
        found = assemble(requested, self.concat_slices, self.concat_bodies)
        if found is None:
            notes = [f"CONCAT missing {chunk_id}" for chunk_id in requested]
            return None, notes
        text, strategy, missing, sources = found
        if strategy == "split":
            notes = [f"CONCAT split chunks={len(requested)}"]
        else:
            notes = [f"CONCAT cover bodies={len(sources)}"]
        notes.extend(f"CONCAT missing {chunk_id}" for chunk_id in missing)
        time_ms = max((source.time_ms for source in sources), default=0)
        base = sources[0].entry if sources else {}
        response = dict(base.get("response") or {})
        content = dict(response.get("content") or {})
        content["text"] = text
        content["encoding"] = ""
        content["mimeType"] = content.get("mimeType") or "application/javascript"
        content["size"] = len(text.encode("utf-8"))
        response["status"] = 200
        response["statusText"] = response.get("statusText") or "OK"
        response["content"] = content
        return {"time": time_ms, "response": response}, notes


class ReplayServer(ThreadingHTTPServer):
    allow_reuse_address = True
    daemon_threads = True

    def __init__(self, address: Tuple[str, int], archive: HarArchive, time_scale: float, miss_log: Optional[Path], tls: Optional[ssl.SSLContext]):
        self.archive = archive
        self.time_scale = time_scale
        self.miss_log = miss_log
        self.tls = tls
        self._miss_lock = threading.Lock()
        super().__init__(address, ReplayHandler)

    def log_miss(self, line: str) -> None:
        with self._miss_lock:
            sys.stderr.write(line + "\n")
            sys.stderr.flush()
            if self.miss_log is not None:
                self.miss_log.parent.mkdir(parents=True, exist_ok=True)
                with self.miss_log.open("a", encoding="utf-8") as handle:
                    handle.write(line + "\n")


class ReplayHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server: ReplayServer
    inner_tls: bool

    def setup(self) -> None:
        super().setup()
        self.inner_tls = False

    def log_message(self, fmt: str, *args) -> None:
        if os.environ.get("REPLAY_VERBOSE") == "1":
            sys.stderr.write("[replay] " + (fmt % args) + "\n")

    def do_GET(self) -> None:
        self._serve(True)

    def do_HEAD(self) -> None:
        self._serve(False)

    def do_POST(self) -> None:
        self._serve(True)

    def do_PUT(self) -> None:
        self._serve(True)

    def do_DELETE(self) -> None:
        self._serve(True)

    def do_OPTIONS(self) -> None:
        self._serve(True)

    def do_PATCH(self) -> None:
        self._serve(True)

    def do_CONNECT(self) -> None:
        if self.server.tls is None:
            body = b"CONNECT is unavailable (no certificate)\n"
            self.send_response(501, "CONNECT unavailable")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_response(200, "Connection Established")
        self.send_header("Connection", "close")
        self.end_headers()
        try:
            self.connection = self.server.tls.wrap_socket(self.connection, server_side=True)
            self.rfile = self.connection.makefile("rb", buffering=0)
            self.wfile = self.connection.makefile("wb", buffering=0)
        except Exception as exc:
            sys.stderr.write(f"replay CONNECT failed: {exc}\n")
            return
        self.inner_tls = True
        self.close_connection = True
        self.handle_one_request()

    def _target_url(self) -> str:
        original = self.headers.get("X-Original-Url") or self.headers.get("x-original-url")
        if original:
            return original
        if self.path.startswith("http://") or self.path.startswith("https://"):
            return self.path
        host = (self.headers.get("Host") or "").strip()
        scheme = "https" if self.inner_tls else "http"
        path = self.path if self.path.startswith("/") else "/" + self.path
        if host:
            return f"{scheme}://{host}{path}"
        return self.path

    def _serve(self, send_body: bool) -> None:
        # Drain a body so a keep-alive client is not left mid-request. The
        # match key is method + URL; the body is not part of it.
        length = int(self.headers.get("Content-Length") or "0")
        if length > 0:
            remaining = length
            while remaining > 0:
                chunk = self.rfile.read(min(remaining, 65536))
                if not chunk:
                    break
                remaining -= len(chunk)
        target = self._target_url()
        entry, notes = self.server.archive.take(self.command, target)
        for line in notes:
            self.server.log_miss(line)
        if entry is None:
            normalized = normalize_url(target)
            # The miss line is method + URL only. Header values (cookies)
            # are not appended, even if the URL itself was built from Host.
            self.server.log_miss(f"MISS {self.command.upper()} {normalized}")
            body = f"HAR entry not found: {self.command.upper()} {normalized}\n".encode("utf-8")
            self._send(404, "Not Found", [(b"Content-Type", b"text/plain; charset=utf-8")], body, send_body, 0)
            return
        delay = float(entry.get("time") or 0) * float(self.server.time_scale)
        if delay > 0:
            time.sleep(delay / 1000.0)
        response = entry.get("response") or {}
        status = int(response.get("status") or 200)
        reason = str(response.get("statusText") or "OK") or "OK"
        content = response.get("content") or {}
        encoding = str(content.get("encoding") or "").lower()
        text = content.get("text") or ""
        if encoding == "base64" and text:
            try:
                body = base64.b64decode(text)
            except Exception:
                body = text.encode("utf-8", errors="replace")
        else:
            body = text.encode("utf-8", errors="replace") if isinstance(text, str) else bytes(text)
        headers: List[Tuple[bytes, bytes]] = []
        saw_type = False
        for header in response.get("headers") or []:
            name = str(header.get("name") or "").strip()
            value = str(header.get("value") or "")
            if not name or name.lower() in HOP_BY_HOP:
                continue
            # HAR bodies are stored decoded (base64 is only how binary is
            # kept in JSON). The recorded Content-Encoding (br, gzip)
            # described the wire bytes, not this body, so it is not sent.
            if name.lower() == "content-encoding":
                continue
            if name.lower() == "content-type":
                saw_type = True
            headers.append((name.encode("latin-1", errors="replace"), value.encode("latin-1", errors="replace")))
        if not saw_type:
            mime = str(content.get("mimeType") or "application/octet-stream")
            headers.append((b"Content-Type", mime.encode("latin-1", errors="replace")))
        self._send(status, reason, headers, body, send_body, delay)

    def _send(self, status: int, reason: str, headers: List[Tuple[bytes, bytes]], body: bytes, send_body: bool, _delay: float) -> None:
        self.send_response(status, reason)
        for name, value in headers:
            self.send_header(name.decode("latin-1", errors="replace"), value.decode("latin-1", errors="replace"))
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        if send_body and body:
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass


def make_tls_context() -> Optional[ssl.SSLContext]:
    openssl = "openssl"
    try:
        directory = Path(tempfile.mkdtemp(prefix="hiwave-replay-"))
        cert = directory / "cert.pem"
        key = directory / "key.pem"
        subprocess.run(
            [
                openssl, "req", "-x509", "-newkey", "rsa:2048",
                "-keyout", str(key), "-out", str(cert),
                "-days", "1", "-nodes", "-subj", "/CN=replay",
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(cert, key)
        try:
            context.set_alpn_protocols(["http/1.1"])
        except Exception:
            pass
        return context
    except Exception as exc:
        sys.stderr.write(f"replay: HTTPS/CONNECT disabled ({exc})\n")
        return None


def serve(har: str | Path, host: str = "127.0.0.1", port: int = 0, time_scale: float = 1.0, miss_log: Optional[Path] = None, tls: bool = True) -> ReplayServer:
    archive = HarArchive.from_file(har)
    context = make_tls_context() if tls else None
    return ReplayServer((host, port), archive, time_scale, miss_log, context)


def main() -> None:
    parser = argparse.ArgumentParser(description="Serve a HAR to HiWave and Chrome")
    parser.add_argument("har", help="HAR file with response bodies. Do not commit recorded HARs.")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--time-scale", type=float, default=1.0, help="Multiply recorded entry time. 0 sends at once.")
    parser.add_argument("--miss-log", default="", help="Append MISS lines here as well as stderr.")
    parser.add_argument("--no-tls", action="store_true", help="Do not answer CONNECT. HiWave's HTTP replay proxy does not need it.")
    args = parser.parse_args()
    if not os.path.exists(args.har):
        sys.stderr.write(f"HAR file not found: {args.har}\n")
        sys.exit(1)
    miss = Path(args.miss_log) if args.miss_log else None
    server = serve(args.har, args.host, args.port, args.time_scale, miss, tls=not args.no_tls)
    host, port = server.server_address[:2]
    print(f"REPLAY_SERVER_READY http://{host}:{port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
