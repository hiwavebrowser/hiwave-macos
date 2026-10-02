"""Write bgsh3.html: the shorthand's layer forms with a data: url image (the only url image the engine
paints today) and gradients, so position, size and repeat show without a fetch."""
import base64
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1002b'
dot = 'url(data:image/png;base64,' + base64.b64encode(open(f'{D}/dot.png', 'rb').read()).decode() + ')'
G = 'linear-gradient(blue, blue)'
cases = [
    ('f0', f'background-image: {dot}; background-repeat: no-repeat; background-position: center;'),
    ('f1', f'background: {dot} no-repeat;'),
    ('f2', f'background: {dot} no-repeat center;'),
    ('f3', f'background: {dot} no-repeat right bottom;'),
    ('f4', f'background: {dot} repeat-x;'),
    ('f5', f'background: {dot} no-repeat 10px 20px / 40px 30px;'),
    ('f6', f'background: #ff0 {dot} no-repeat center;'),
    ('f7', f'background: {G} no-repeat 20px 10px / 30px 15px;'),
    ('f8', f'background: {dot} center / cover no-repeat;'),
    ('f9', f'background: {dot} center / contain no-repeat #eee;'),
    ('f10', f'background: {dot};'),
    ('f11', f'background: no-repeat center {dot};'),
    ('f12', f'background: {dot} 50% 50% no-repeat, linear-gradient(#cfc, #cfc);'),
    ('f13', f'background: {G} no-repeat bottom / 100% 4px;'),
    ('f14', f'background: {G} repeat-y right / 6px 12px;'),
    ('f15', f'background: {G} no-repeat top right / 30px 15px;'),
    ('f16', f'background: {G} no-repeat left 10px top 20px / 30px 15px;'),
    ('f17', f'background: {G} no-repeat; background-size: 50% 50%; background-position: bottom;'),
    ('f18', f'background: #cfc {G} no-repeat center/50% 50%;'),
    ('f19', f'background: {G} 0 0/20px 100% no-repeat, linear-gradient(red, red) 100% 0/20px 100% no-repeat, #eee;'),
]
css = '\n'.join(f'#{k} {{ {v} }}' for k, v in cases)
divs = ''.join(f'<div id="{k}"></div>' for k, _ in cases)
open(f'{D}/bgsh3.html', 'w').write(f'''<!doctype html>
<html><head><meta charset="utf-8">
<style>
body {{ margin: 10px; background: #fff; }}
div {{ float: left; width: 120px; height: 60px; margin: 0 8px 8px 0; }}
{css}
</style></head><body>
{divs}
</body></html>
''')
print(len(cases), 'cases')
