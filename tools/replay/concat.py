"""Reassemble redditstatic ``js/concat`` responses from a HAR.

Reddit builds the chunk list at runtime, so a request's ``chunks=`` value
often does not match any recorded URL. The same chunks are usually present
across other recorded concat entries.

A recorded body splits cleanly when it is module-factory registrations
separated by ``/* @sml-chunk */`` and each piece has one ``SML.dm("id"``.
Webpack ``.push([["id"]`` registrations split the same way. Anything else
is left whole and served by covering the request with the smallest set of
those bodies.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Dict, List, Optional, Sequence
from urllib.parse import parse_qsl, unquote, urlparse

# Locale and cache-busters are not chunk ids.
_IGNORE_KEYS = {"prefix", "locale", "lang", "v", "m", "version"}
_LIST_KEYS = {"chunks", "chunk", "chunkids", "ids", "c", "id"}

_MARKER = re.compile(r"/\*\s*@sml-chunk\s*\*/")
_DM = re.compile(r"""SML\.dm\(\s*["']([^"']+)["']""")
_PUSH = re.compile(r"""\.push\(\s*\[\s*\[([^\]]*)\]""")
_JOIN = "\n/* @sml-chunk */\n"


@dataclass
class ConcatBody:
    ids: List[str]
    text: str
    time_ms: float
    entry: dict


def is_concat_url(url: str) -> bool:
    path = urlparse(url).path.lower()
    return "/js/concat" in path or path.rstrip("/").endswith("/concat")


def chunk_ids(url: str) -> List[str]:
    """Chunk ids in the order the URL lists them. Empty when this is not a concat URL."""
    if not is_concat_url(url):
        return []
    parsed = urlparse(url)
    found: List[str] = []
    bracket = re.search(r"concat\[([^\]]*)\]", parsed.path, re.IGNORECASE)
    if bracket:
        found.extend(_split_ids(bracket.group(1)))
    for key, value in parse_qsl(parsed.query, keep_blank_values=True):
        lowered = key.lower()
        if lowered in _IGNORE_KEYS:
            continue
        if lowered in _LIST_KEYS:
            found.extend(_split_ids(value))
            continue
        # Some builds put the chunk id in the query key (`?ChunkName` or `?ChunkName=1`).
        if value in ("", "1") and _token(key):
            found.append(key)
    return _dedupe(found)


def split_body(text: str, expected: Sequence[str]) -> Optional[Dict[str, str]]:
    """Map chunk id to its slice when every expected id is one clean registration.

    Returns None when the body does not split into exactly those ids. The
    caller then keeps the body whole.
    """
    if not expected or not text:
        return None
    parts = [part.strip() for part in _MARKER.split(text)]
    parts = [part for part in parts if part]
    mapped = _map_parts(parts)
    if mapped is None:
        mapped = _map_parts(_webpack_parts(text))
    if mapped is None:
        return None
    if set(mapped) != set(expected):
        return None
    return mapped


def assemble(
    requested: Sequence[str],
    slices: Dict[str, str],
    bodies: Sequence[ConcatBody],
) -> Optional[tuple]:
    """Return ``(text, strategy, missing, sources)`` or None when nothing matches.

    ``strategy`` is ``split`` when every requested chunk had its own slice,
    otherwise ``cover``. ``missing`` lists requested ids still absent.
    """
    if not requested:
        return None
    if all(chunk_id in slices for chunk_id in requested):
        ordered: List[str] = []
        seen = set()
        sources: List[ConcatBody] = []
        for chunk_id in requested:
            piece = slices[chunk_id]
            if piece in seen:
                continue
            seen.add(piece)
            ordered.append(piece)
        return _JOIN.join(ordered), "split", [], _sources_for(requested, bodies, sources)
    chosen = _smallest_cover(list(requested), list(bodies))
    if not chosen:
        return None
    covered = set()
    for body in chosen:
        covered.update(body.ids)
    missing = [chunk_id for chunk_id in requested if chunk_id not in covered]

    def rank(body: ConcatBody) -> int:
        for index, chunk_id in enumerate(requested):
            if chunk_id in body.ids:
                return index
        return len(requested)

    chosen.sort(key=rank)
    text = _JOIN.join(body.text for body in chosen)
    return text, "cover", missing, chosen


def _sources_for(requested: Sequence[str], bodies: Sequence[ConcatBody], into: List[ConcatBody]) -> List[ConcatBody]:
    seen = set()
    for chunk_id in requested:
        for body in bodies:
            if chunk_id in body.ids and id(body) not in seen:
                seen.add(id(body))
                into.append(body)
                break
    return into


def _map_parts(parts: Sequence[str]) -> Optional[Dict[str, str]]:
    if not parts:
        return None
    mapped: Dict[str, str] = {}
    for part in parts:
        ids = _DM.findall(part)
        if len(ids) != 1:
            ids = _push_ids(part)
        if len(ids) != 1:
            return None
        chunk_id = ids[0]
        if not _token(chunk_id) or chunk_id in mapped:
            return None
        mapped[chunk_id] = part
    return mapped or None


def _webpack_parts(text: str) -> List[str]:
    starts = [match.start() for match in re.finditer(r"\.push\(\s*\[", text)]
    if len(starts) < 2:
        return [text] if text.strip() else []
    parts = []
    for index, start in enumerate(starts):
        end = starts[index + 1] if index + 1 < len(starts) else len(text)
        parts.append(text[start:end].strip())
    return parts


def _push_ids(part: str) -> List[str]:
    match = _PUSH.search(part)
    if not match:
        return []
    return [item for item in _split_ids(match.group(1).replace('"', "").replace("'", "")) if _token(item)]


def _smallest_cover(requested: Sequence[str], bodies: Sequence[ConcatBody]) -> List[ConcatBody]:
    need = set(requested)
    useful = [body for body in bodies if need.intersection(body.ids)]
    if not useful:
        return []
    if len(useful) > 16:
        return _greedy_cover(need, useful)
    best: Optional[List[ConcatBody]] = None
    best_count = -1
    for mask in range(1, 1 << len(useful)):
        chosen = [useful[index] for index in range(len(useful)) if mask & (1 << index)]
        covered = set()
        for body in chosen:
            covered.update(body.ids)
        count = len(need.intersection(covered))
        if count == 0:
            continue
        if best is None or count > best_count or (count == best_count and len(chosen) < len(best)):
            best = chosen
            best_count = count
    return best or []


def _greedy_cover(need: set, bodies: Sequence[ConcatBody]) -> List[ConcatBody]:
    remaining = set(need)
    chosen: List[ConcatBody] = []
    pool = list(bodies)
    while remaining and pool:
        pool.sort(key=lambda body: len(remaining.intersection(body.ids)), reverse=True)
        pick = pool.pop(0)
        gained = remaining.intersection(pick.ids)
        if not gained:
            break
        remaining -= gained
        chosen.append(pick)
    return chosen


def _split_ids(value: str) -> List[str]:
    found = []
    for part in unquote(value).replace(";", ",").split(","):
        part = part.strip().strip("'\"")
        if part:
            found.append(part)
    return found


def _dedupe(ids: Sequence[str]) -> List[str]:
    seen = set()
    ordered = []
    for chunk_id in ids:
        if chunk_id in seen:
            continue
        seen.add(chunk_id)
        ordered.append(chunk_id)
    return ordered


def _token(value: str) -> bool:
    if not value or len(value) > 128:
        return False
    return all(ch not in value for ch in " \t\r\n&?#")
