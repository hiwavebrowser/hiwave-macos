# Replay harness

Serve a HAR to HiWave and to Chrome, then diff the two page traces. Recorded
HARs contain cookies. Keep them outside the repo. `fixtures/synthetic.har` is
the only archive that belongs here.

## Replay server

```bash
python3 tools/replay/server.py captured.har --port 8765 --time-scale 1
```

Requests match on method plus a normalised URL: lowercase scheme and host,
default port removed, fragment removed, query pairs sorted. The nth request
for a key replays the nth entry, then repeats the last. A miss is logged as
`MISS METHOD url` (no cookie or authorization values) and answered 404.

Recorded response headers are sent. Hop-by-hop headers are dropped and
`Content-Length` is the body that is written. The entry's `time` (milliseconds)
is slept, multiplied by `--time-scale` (`0` sends immediately).

HiWave sends its own requests when `HIWAVE_REPLAY` is set. It is off when the
variable is unset.

```bash
HIWAVE_REPLAY=http://127.0.0.1:8765 cargo run -p hiwave-app --features rustkit
```

Chrome uses the same process as an HTTP proxy. HTTPS entries are answered on
`CONNECT` with a temporary certificate, so Chrome needs
`--ignore-certificate-errors` (the comparison script sets that).

## Traces

`tools/replay/chrome_trace.mjs` loads one URL through the proxy and writes JSON:

```bash
node tools/replay/chrome_trace.mjs \
  --replay http://127.0.0.1:8765 \
  --url https://example.test/ \
  --out /tmp/chrome.json
```

HiWave writes the same schema for the load when `HIWAVE_TRACE_JSON=/tmp/hiwave.json`
or `HIWAVE_CONSOLE_LOG=1` (one `PAGE_TRACE` line on stderr). Both together
write the file and the line. The file is replaced as the load proceeds, and
again when the load returns.

Fields: `browser`, `url`, `ready_state` (`loading` / `interactive` /
`complete` with `t_ms` from navigation start), `dcl_ms`, `load_ms`, `scripts`
(`execute`, then `load` or `error`), `console` errors, `mutations` (one record
per subtree add or remove of more than 50 nodes), `max_js_stack`.

```bash
python3 tools/replay/diff_trace.py /tmp/hiwave.json /tmp/chrome.json
```

The diff prints the first structural mismatch and ignores timestamps. A `load`
event only one side recorded is skipped when both sides executed that script.
`max_js_stack` is compared last, and only when the depths differ by more than
16 frames and one is more than double the other.

## Tests

```bash
python3 tools/replay/test_replay.py
```
