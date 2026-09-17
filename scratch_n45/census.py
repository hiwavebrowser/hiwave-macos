"""Census of pseudo-classes used by the board's cases (registry.json is the only truth)."""
import json, re, collections, os, sys

reg = json.load(open('cases/registry.json'))
cases = reg.get('cases', reg) if isinstance(reg, dict) else reg
files = []
if isinstance(cases, dict):
    for k, v in cases.items():
        if isinstance(v, dict):
            files.append((k, v.get('html') or v.get('path') or v.get('source') or ''))
        else:
            files.append((k, str(v)))
else:
    for v in cases:
        files.append((v.get('id') or v.get('name'), v.get('html') or v.get('path') or v.get('source') or ''))

if len(sys.argv) > 1 and sys.argv[1] == 'keys':
    print(json.dumps(cases if isinstance(cases, dict) else cases[:2], indent=1)[:1500])
    sys.exit()

cnt = collections.Counter(); per = {}
pat = re.compile(r'(?<!:):([a-zA-Z-]+)(\([^)]*\))?')
for k, f in files:
    if not f or not os.path.exists(f):
        print('MISS', k, f); continue
    s = open(f, encoding='utf-8', errors='replace').read()
    # only <style> blocks
    styles = '\n'.join(re.findall(r'<style[^>]*>(.*?)</style>', s, re.S))
    ps = set()
    for m in pat.finditer(styles):
        name = m.group(1)
        if name in ('root',): continue
        ps.add(name + ('()' if m.group(2) else ''))
    per[k] = ps; cnt.update(ps)
print(cnt.most_common())
for k, v in sorted(per.items()):
    print(k, sorted(v))
