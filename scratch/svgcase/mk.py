"""Write lower/camel viewBox repro fixtures next to this file."""
import os
here = os.path.dirname(os.path.abspath(__file__))
p = 'M6,8c1.1,0 2,-0.9 2,-2s-0.9,-2 -2,-2 -2,0.9 -2,2 0.9,2 2,2zM12,20c1.1,0 2,-0.9 2,-2s-0.9,-2 -2,-2 -2,0.9 -2,2 0.9,2 2,2z'
for name, attr in [('lower', 'viewbox'), ('camel', 'viewBox')]:
    open(os.path.join(here, f'{name}.html'), 'w').write(f'''<!DOCTYPE html><html><body style="margin:0;background:#fff">
<style>.i{{width:48px;height:48px;display:block}}</style>
<div style="width:48px;height:48px;background:#eee"><svg class="i" {attr}="0 0 24 24"><path d="{p}"/></svg></div></body></html>''')
