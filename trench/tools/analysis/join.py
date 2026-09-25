import json,collections,os,re,sys
REPO=os.path.abspath(os.path.join(os.path.dirname(__file__),'..','..','..'))
# usage: join.py <probe.json> [engine lib.rs]  (default: this checkout's rustkit-engine)
P=json.load(open(sys.argv[1]))['sites']
src=open(sys.argv[2] if len(sys.argv)>2 else os.path.join(REPO,'crates','rustkit-engine','src','lib.rs')).read()
i=src.find('fn apply_style_property'); j=src.find('\nfn ',i+10); body=src[i:j]
RK={p for a in re.findall(r'^\s*((?:"[a-z-]+"\s*\|\s*)*"[a-z-]+")\s*=>',body,re.M) for p in re.findall(r'"([a-z-]+)"',a)}
# properties handled outside apply_style_property or that are harmless to ignore for first paint
IGNORE={'cursor','pointer-events','user-select','-webkit-user-select','-webkit-tap-highlight-color','outline','outline-offset','outline-color','outline-style','outline-width',
        'will-change','content-visibility','speak','-webkit-font-smoothing','-moz-osx-font-smoothing','text-rendering','touch-action','scroll-behavior','resize','appearance','-webkit-appearance','-moz-appearance',
        'caret-color','accent-color','forced-color-adjust','print-color-adjust','-webkit-print-color-adjust','color-scheme','src','font-display','unicode-range','size-adjust','ascent-override','descent-override','line-gap-override',
        'syntax','inherits','initial-value','overscroll-behavior','overscroll-behavior-x','overscroll-behavior-y','-ms-overflow-style','scrollbar-width','scrollbar-color','-webkit-overflow-scrolling','zoom','-ms-text-size-adjust','-webkit-text-size-adjust','text-size-adjust',
        'animation','animation-name','animation-duration','animation-timing-function','animation-delay','animation-iteration-count','animation-direction','animation-fill-mode','animation-play-state','transition','transition-property','transition-duration','transition-timing-function','transition-delay','-webkit-transition','-webkit-animation'}
glob_unsupported=collections.Counter(); glob_sites=collections.defaultdict(set)
rows={}
for s,r in P.items():
    inv=r.get('inventory') or {}
    props=r.get('css_properties') or {}
    tot=sum(v for k,v in props.items()); 
    unsup={k:v for k,v in props.items() if k not in RK and k not in IGNORE and not k.startswith('-webkit-') and not k.startswith('-moz-') and not k.startswith('-ms-')}
    for k,v in unsup.items(): glob_unsupported[k]+=v; glob_sites[k].add(s)
    vd=inv.get('viewport_display',{}); d=inv.get('display',{})
    rows[s]={'nodes':inv.get('dom_nodes'),'vp_elems':inv.get('in_viewport_elems'),
      'vp_flex':sum(v for k,v in vd.items() if 'flex' in k),'vp_grid':sum(v for k,v in vd.items() if 'grid' in k),
      'vp_table':sum(v for k,v in vd.items() if 'table' in k),'contents':d.get('contents',0),'flow_root':d.get('flow-root',0),'list_item':d.get('list-item',0),
      'img':(inv.get('images') or {}).get('img'),'svg':(inv.get('images') or {}).get('inline_svg'),'lazy':(inv.get('images') or {}).get('lazy'),
      'img_types':r.get('img_types'),'js_files':r.get('js_files'),'js_MB':round((r.get('js_bytes') or 0)/1e6,2),'css_KB':round((r.get('css_bytes') or 0)/1e3),
      'decls':tot,'unsup_decl_share':round(sum(unsup.values())/tot,3) if tot else None,
      'server_html_share':r.get('server_html_share'),'vp_words':r.get('viewport_words_js'),
      'feat':inv.get('features'),'css_feat':r.get('css_features'),'doc_status':r.get('doc_status'),'load_ms':r.get('load_ms'),'shadow':(inv.get('features') or {}).get('shadow_roots'),'custom':(inv.get('features') or {}).get('custom_elements')}
json.dump(rows,open(os.environ.get('RS_OUT','.')+'/joined.json','w'),indent=1)
print('site       nodes vpEl flex grid tbl cont img svg lazy  jsMB cssKB decls unsup%  srvHTML vpWords')
for s,v in rows.items():
    print(f"{s:10}{v['nodes'] or 0:6}{v['vp_elems'] or 0:5}{v['vp_flex']:5}{v['vp_grid']:5}{v['vp_table']:4}{v['contents']:5}{v['img'] or 0:4}{v['svg'] or 0:4}{v['lazy'] or 0:5}{v['js_MB']:6}{v['css_KB']:6}{v['decls']:6} {('%5.1f'%(v['unsup_decl_share']*100)) if v['unsup_decl_share'] is not None else '   - '}  {v['server_html_share']}  {v['vp_words']}")
print()
print('top unsupported properties by declaration count (sites using)')
for k,v in glob_unsupported.most_common(45): print(f'  {k:28}{v:6}  sites={len(glob_sites[k]):2} {sorted(glob_sites[k])[:8]}')
