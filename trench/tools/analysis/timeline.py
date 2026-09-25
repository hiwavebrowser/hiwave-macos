import os
import re,os,json,sys
from datetime import datetime
import os
REPO=os.path.abspath(os.path.join(os.path.dirname(__file__),'..','..','..'))
R=os.environ.get('RS_RUNS', os.path.join(REPO,'trench','realsite','runs'))
OUT=os.environ.get('RS_OUT', '.')

ANSI=re.compile(r'\x1b\[[0-9;]*m')
TS=re.compile(r'^(\d{4}-\d\d-\d\dT[\d:.]+)Z')
def phase(msg):
    if 'Loading URL' in msg: return 'net:doc'
    if 'Extracted stylesheets' in msg: return 'cascade'
    if 'Root box built' in msg: return 'layout'
    if 'external stylesheet' in msg or 'Loaded external stylesheets' in msg or 'parse external stylesheet' in msg: return 'net:css'
    if 'web font' in msg.lower() or 'Loaded web fonts' in msg: return 'net:font'
    if 'image' in msg.lower() or 'SVG' in msg: return 'net:img'
    if 'page script' in msg or 'lifecycle' in msg or 'JavaScript runtime' in msg: return 'script'
    if 'Capturing' in msg or 'Frame captured' in msg or 'Display list exported' in msg or 'Navigation finished' in msg: return 'capture'
    if 'Initializ' in msg or 'GPU' in msg or 'Headless' in msg or 'ResourceLoader' in msg or 'cache' in msg: return 'init'
    return 'other'
def parse(path, wall_ms, timed_out):
    ev=[]
    for line in open(path,errors='replace'):
        line=ANSI.sub('',line); m=TS.match(line)
        if not m: continue
        t=datetime.fromisoformat(m.group(1)).timestamp()
        ev.append((t,phase(line),line.strip()[28:200]))
    if not ev: return None
    t0=next((t for t,p,_ in ev if p=='net:doc'),ev[0][0])
    b={}
    for i,(t,p,_) in enumerate(ev):
        if t<t0: continue
        nt=ev[i+1][0] if i+1<len(ev) else (t0+wall_ms/1000 if timed_out else t)
        b[p]=b.get(p,0)+max(0,nt-t)
    last=ev[-1]
    return {'b':{k:round(v,2) for k,v in b.items()},'total':round((ev[-1][0]-t0) if not timed_out else wall_ms/1000,1),'last':last[1],'lastmsg':last[2][:140],
            'n_relayout':sum(1 for _,p,_ in ev if p=='cascade'),'n_scripts':sum(1 for _,_,m in ev if 'Running page script' in m)}
out={}
for run in sys.argv[1:]:
    out[run]={}
    for f in sorted(os.listdir(f'{R}/{run}')):
        if not f.endswith('.json') or f=='summary.json': continue
        j=json.load(open(f'{R}/{run}/{f}')); s=j['id']
        lp=f'{R}/{run}/{s}/rustkit-stderr.log'
        if not os.path.exists(lp): continue
        to='exceeded' in str(j.get('loads',{}).get('why'))
        wall=(j.get('rustkit') or {}).get('wall_ms') or 30000
        r=parse(lp, wall if not to else 30000, to)
        if r: r['timeout']=to; r['loads']=j['loads']['pass']; out[run][s]=r
json.dump(out,open(os.path.join(OUT,'timeline.json'),'w'),indent=1)
for run,d in out.items():
    print('==',run)
    for s,r in sorted(d.items(), key=lambda x:-x[1]['total']):
        b=r['b']; parts=' '.join(f"{k}={v:.1f}" for k,v in sorted(b.items(),key=lambda x:-x[1]) if v>=0.3)
        print(f"{s:10} {'TO' if r['timeout'] else ('ok' if r['loads'] else '--')} {r['total']:5.1f}s relayouts={r['n_relayout']:2} scripts={r['n_scripts']:3} | {parts}")
        if r['timeout']: print(f"{'':14}last: {r['last']} :: {r['lastmsg'][:120]}")
