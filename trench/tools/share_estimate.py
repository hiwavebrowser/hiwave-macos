#!/usr/bin/env python3
"""Upper bound on what a style-sharing cache could skip on the pinned pages.

    share_estimate.py [site ...]

Reads trench/cascade/snapshots/<site>/index.html with Python's HTML parser (not
the engine's, so element counts differ a little from the engine's) and gives
every element under <body> a key: its parent's key, its tag, its class
attribute, and a signature of its other attributes. Two elements with the same
key have the same ancestors chain of (tag, class, attributes), so every
selector made of type, class, attribute, descendant and child parts matches
both or neither. An element that is not the first with its key could take the
first one's computed style instead of running the cascade.

Four signatures:
  strict      every attribute name and value
  loose       every attribute name; values only for id, style, type, role, lang, dir, hidden
  positional  loose, plus whether the element is its parent's first, last, only or a middle child
  anyid       loose, but an id counts only as present or absent (right only for
              ids that no rule in the sheets names; the sheets are not read)

Sibling combinators and :nth-child() are not modelled, so this is an upper
bound, not a hit rate.
"""
import collections
import html.parser
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SNAP = os.path.join(HERE, "..", "cascade", "snapshots")
VOID = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta",
        "param", "source", "track", "wbr"}
SKIP = {"head", "title", "meta", "link", "script", "style", "noscript"}
VALUE_ATTRS = {"id", "style", "type", "role", "lang", "dir", "hidden"}


class Tree(html.parser.HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.root = dict(tag="#root", attrs={}, kids=[], parent=None)
        self.cur = self.root

    def handle_starttag(self, tag, attrs):
        node = dict(tag=tag, attrs=dict(attrs), kids=[], parent=self.cur)
        self.cur["kids"].append(node)
        if tag not in VOID:
            self.cur = node

    def handle_startendtag(self, tag, attrs):
        self.cur["kids"].append(dict(tag=tag, attrs=dict(attrs), kids=[], parent=self.cur))

    def handle_endtag(self, tag):
        node = self.cur
        while node is not None and node["tag"] != tag:
            node = node["parent"]
        if node is not None and node["parent"] is not None:
            self.cur = node["parent"]


def signature(node, mode, position):
    attrs = node["attrs"]
    if mode == "strict":
        sig = tuple(sorted((k, v or "") for k, v in attrs.items() if k != "class"))
    else:
        keep = VALUE_ATTRS - {"id"} if mode == "anyid" else VALUE_ATTRS
        sig = tuple(sorted((k, (v or "") if k in keep else "")
                           for k, v in attrs.items() if k != "class"))
    cls = " ".join((attrs.get("class") or "").split())
    return (node["tag"], cls, sig, position if mode == "positional" else "")


def count(root, mode):
    seen = collections.Counter()
    total = 0
    stack = [(root, ())]
    while stack:
        node, parent_key = stack.pop()
        kids = [k for k in node["kids"] if k["tag"] not in SKIP]
        for i, kid in enumerate(kids):
            position = ("only" if len(kids) == 1 else "first" if i == 0
                        else "last" if i == len(kids) - 1 else "middle")
            key = hash((parent_key, signature(kid, mode, position)))
            seen[key] += 1
            total += 1
            stack.append((kid, key))
    return total, len(seen)


def find(node, tag):
    stack = [node]
    while stack:
        n = stack.pop()
        if n["tag"] == tag:
            return n
        stack.extend(n["kids"])
    return None


for site in sys.argv[1:] or ("cnn", "github", "wikipedia"):
    tree = Tree()
    with open(os.path.join(SNAP, site, "index.html"), errors="replace") as f:
        tree.feed(f.read())
    body = find(tree.root, "body") or tree.root
    out = [site]
    for mode in ("strict", "loose", "positional", "anyid"):
        total, distinct = count(body, mode)
        out.append(f"{mode}: {total - distinct} of {total} elements repeat a key "
                   f"({100 * (total - distinct) / total:.0f}%), {distinct} distinct")
    print(" | ".join(out))
