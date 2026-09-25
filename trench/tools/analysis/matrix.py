import os
import json
import os
REPO=os.path.abspath(os.path.join(os.path.dirname(__file__),'..','..','..'))
R=os.environ.get('RS_RUNS', os.path.join(REPO,'trench','realsite','runs'))
OUT=os.environ.get('RS_OUT', '.')

T=json.load(open(os.path.join(OUT,'trend.json')))
full=[(d,s) for d,s in T if len(s)>=20]
sites=sorted({k for d,s in full for k in s})
lab={d:d[4:8]+d[8:14] for d,_ in full}
print('site      ', ' '.join(f'{lab[d][:9]:>9}' for d,_ in full))
for k in sites:
    cells=[]
    for d,s in full:
        v=s.get(k)
        if not v: cells.append('    -    '); continue
        c=('L' if v['L'] else '.')+('R' if v['R'] else '.')+('V' if v['LR'] else '.')
        cells.append(f"{c}{(v['Rr'] or 0)*100:>3.0f}{(v['D'] if v['D'] is not None else -1):>3.0f}")
    print(f'{k:10}',' '.join(f'{c:>9}' for c in cells))
print()
# LOADS flips and elapsed
for k in sites:
    Ls=[s[k]['L'] for d,s in full if k in s]
    rk=[s[k]['rk'] for d,s in full if k in s and s[k]['rk']]
    ch=[min([c for c in s[k]['ch'] if c] or [0]) for d,s in full if k in s]
    why=set(str(s[k]['Lwhy'])[:40] for d,s in full if k in s and not s[k]['L'])
    print(f"{k:10} L {sum(bool(x) for x in Ls)}/{len(Ls)}  rk_ms min/med/max {min(rk) if rk else '-'} {sorted(rk)[len(rk)//2] if rk else '-'} {max(rk) if rk else '-'}  chrome_med {sorted(ch)[len(ch)//2]}  why={why}")
