"""Fetch linkedin's two CSS bundle variants (old hash URL, new assets/ URL) and summarise them."""
import collections, re, subprocess, sys
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/li0930'
UA = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36'
for name, url in (('new', 'https://static.licdn.com/aero-v1/sc/h/assets/Kf3QXGdB.css'),
                  ('old', 'https://static.licdn.com/aero-v1/sc/h/8bnxn2t783wxez9h12gkxfnlx')):
    r = subprocess.run(['curl', '-s', '--compressed', '-A', UA, '-o', f'{D}/{name}.css', '-w',
                        '%{http_code} %{size_download} %{content_type}', url], capture_output=True, text=True)
    css = open(f'{D}/{name}.css', errors='replace').read()
    ats = collections.Counter(re.findall(r'@[a-z-]+', css))
    print(name, r.stdout, len(css), 'rules~', css.count('{'), dict(ats.most_common(12)))
    print('   nesting-ish &:', css.count('&'), ' :where(', css.count(':where('), ' :is(', css.count(':is('),
          ' :has(', css.count(':has('), ' @layer', css.count('@layer'), ' @container', css.count('@container'))
    print('   head:', css[:300].replace('\n', ' '))
