import os
import json,sys,os,collections
import numpy as np
from PIL import Image
import os
REPO=os.path.abspath(os.path.join(os.path.dirname(__file__),'..','..','..'))
R=os.environ.get('RS_RUNS', os.path.join(REPO,'trench','realsite','runs'))
OUT=os.environ.get('RS_OUT', '.')

run=sys.argv[1]; sites=sys.argv[2:]
def load(p):
    im=Image.open(p).convert('RGB'); a=np.asarray(im).astype(np.int16)
    return a[:800,:1280]
def dominant(a):
    q=(a//16).reshape(-1,3); v,c=np.unique(q,axis=0,return_counts=True); return v[c.argmax()]*16+8
out={}
for s in sites:
    d=f'{R}/{run}/{s}'
    ch=load(f'{d}/chrome-a.png'); rk=load(f'{d}/rustkit.ppm')
    h=min(ch.shape[0],rk.shape[0]); w=min(ch.shape[1],rk.shape[1]); ch=ch[:h,:w]; rk=rk[:h,:w]
    diff=(np.abs(ch-rk).max(axis=2)>40)
    bgc=dominant(ch); bgr=dominant(rk)
    chc=(np.abs(ch-bgc).max(axis=2)>30); rkc=(np.abs(rk-bgr).max(axis=2)>30)  # content masks
    # tiles 8x5
    TY,TX=5,8; th,tw=h//TY,w//TX; cls=collections.Counter()
    for y in range(TY):
        for x in range(TX):
            sl=(slice(y*th,(y+1)*th),slice(x*tw,(x+1)*tw))
            dfr=diff[sl].mean(); cf=chc[sl].mean(); rf=rkc[sl].mean()
            if dfr<0.05: cls['match']+=1
            elif cf>0.03 and rf<0.01: cls['missing (chrome content, rustkit empty)']+=1
            elif rf>0.03 and cf<0.01: cls['extra (rustkit content, chrome empty)']+=1
            elif abs(cf-rf)<0.5*max(cf,rf,1e-6): cls['both content, differs (offset/font/style)']+=1
            else: cls['density mismatch']+=1
    # vertical offset estimate via row content profiles
    pc=chc.mean(axis=1); pr=rkc.mean(axis=1); best=(0,-1)
    for off in range(-300,301,4):
        a=pc[max(0,off):h+min(0,off)]; b=pr[max(0,-off):h-max(0,off)]
        if len(a)>100 and a.std()>0 and b.std()>0:
            c=np.corrcoef(a,b)[0,1]
            if c>best[1]: best=(off,c)
    dl=json.load(open(f'{d}/rustkit-display-list.json'))['commands']
    ops=collections.Counter(c.get('op') for c in dl)
    full=[c for c in dl if c.get('op')=='solid_color' and c['rect']['width']>=1200 and c['rect']['height']>=700]
    j=json.load(open(f'{R}/{run}/{s}.json'))
    out[s]={'diff%':round(diff.mean()*100,1),'board_diff':j['looks_right'].get('diff'),'readable':j['readable'].get('ratio'),
        'bg_chrome':bgc.tolist(),'bg_rk':bgr.tolist(),'content_chrome%':round(chc.mean()*100,1),'content_rk%':round(rkc.mean()*100,1),
        'tiles':dict(cls),'vshift_px(rk vs chrome)':best[0],'vshift_corr':round(best[1],2),'ops':dict(ops.most_common(10)),
        'fullviewport_fills':[(c['color']['r'],c['color']['g'],c['color']['b'],round(c['color']['a'],2)) for c in full][:6],
        'missing_words':j['readable'].get('missing_sample',[])[:12]}
json.dump(out,open(os.path.join(OUT,f'anatomy-{run}.json'),'w'),indent=1)
for s,v in out.items(): print(s, json.dumps(v)[:900]); print()
