import os
import json,glob,os,sys,statistics as st
import os
REPO=os.path.abspath(os.path.join(os.path.dirname(__file__),'..','..','..'))
R=os.environ.get('RS_RUNS', os.path.join(REPO,'trench','realsite','runs'))
OUT=os.environ.get('RS_OUT', '.')

rows=[]
for d in sorted(os.listdir(R)):
    sites={}
    for f in glob.glob(f'{R}/{d}/*.json'):
        if f.endswith('summary.json'): continue
        try: j=json.load(open(f))
        except Exception: continue
        sites[j['id']]=j
    rows.append((d,sites))
json.dump([(d,{k:{'L':v.get('loads',{}).get('pass'),'Lwhy':v.get('loads',{}).get('why'),
  'R':v.get('readable',{}).get('pass'),'Rr':v.get('readable',{}).get('ratio'),
  'LR':v.get('looks_right',{}).get('pass'),'D':v.get('looks_right',{}).get('diff'),'Dself':v.get('looks_right',{}).get('chrome_self_diff'),
  'pts':v.get('points'),'rk':(v.get('rustkit') or {}).get('elapsed_ms'),'rkwall':(v.get('rustkit') or {}).get('wall_ms'),'rkst':(v.get('rustkit') or {}).get('status'),'rkerr':str((v.get('rustkit') or {}).get('error'))[:80],
  'nbf':(v.get('rustkit') or {}).get('non_background_fraction'),
  'ch':[c.get('elapsed_ms') for c in v.get('chrome',[])],'chst':[c.get('status') for c in v.get('chrome',[])],
  'acc':v.get('access'),'ss':(v.get('rustkit') or {}).get('script_stats')} for k,v in s.items()}) for d,s in rows],open(os.path.join(OUT,'trend.json'),'w'))
for d,s in rows: print(d, len(s), sum((v.get('points') or 0) for v in s.values()))
