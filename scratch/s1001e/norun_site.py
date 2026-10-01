"""Group a site's text ops by (family head, weight, style, has-run) with a sample.
usage: norun_site.py <dl.json>"""
import collections, json, sys
j = json.load(open(sys.argv[1]))
cmds = j.get('commands') if isinstance(j, dict) else j
groups = collections.defaultdict(list)
for c in cmds:
    if c.get('op') != 'text':
        continue
    run = c.get('run')
    key = (c['font_family'][:34], c['font_weight'], c['font_style'], run['face'] if run else None,
           c.get('advances') is not None)
    groups[key].append(c['text'])
for key, texts in sorted(groups.items(), key=lambda kv: -len(kv[1])):
    nonascii = sum(1 for t in texts if any(ord(ch) > 0x7e for ch in t))
    print(len(texts), key, 'non-ascii lines', nonascii, 'e.g.', repr(texts[0][:28]), repr(texts[-1][:28]))
