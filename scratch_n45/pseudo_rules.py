#!/usr/bin/env python3
"""pseudo_rules.py HTML — print every CSS rule selector in <style> blocks that contains a pseudo-class."""
import re, sys
s = open(sys.argv[1], encoding='utf-8', errors='replace').read()
styles = '\n'.join(re.findall(r'<style[^>]*>(.*?)</style>', s, re.S))
styles = re.sub(r'/\*.*?\*/', '', styles, flags=re.S)
for m in re.finditer(r'([^{}]+)\{([^{}]*)\}', styles):
    sel = ' '.join(m.group(1).split())
    if re.search(r'(?<!:):[a-zA-Z-]', sel) and not sel.startswith('@'):
        print(sel, '{', ' '.join(m.group(2).split())[:90], '}')
