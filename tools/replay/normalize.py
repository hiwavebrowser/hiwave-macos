"""URL normalisation shared by the replay server and the trace diff.

Method is uppercased by callers. A URL loses its fragment and userinfo,
lowercases its scheme and host, drops a default port, and sorts its query
pairs. The path is kept as written, including a trailing slash.
"""

from urllib.parse import parse_qsl, urlencode, urlparse, urlunparse


def normalize_url(url: str) -> str:
    raw = (url or "").strip()
    if not raw:
        return raw
    parts = urlparse(raw)
    if not parts.scheme or not parts.hostname:
        return raw.split("#", 1)[0]
    scheme = parts.scheme.lower()
    host = parts.hostname.lower()
    port = parts.port
    if port and not ((scheme == "http" and port == 80) or (scheme == "https" and port == 443)):
        netloc = f"{host}:{port}"
    else:
        netloc = host
    path = parts.path or "/"
    pairs = parse_qsl(parts.query, keep_blank_values=True)
    pairs.sort()
    query = urlencode(pairs)
    return urlunparse((scheme, netloc, path, "", query, ""))
