"""Failing-first helper for revert-layer: make reverted_layer_properties return nothing (off) or
restore it (on).  usage: python3 toggle_revert.py off|on"""
import sys
P = '/Users/petecopeland/Repos/.worktrees/rs-light-dark/crates/rustkit-engine/src/lib.rs'
ON = '    let mut winners: Vec<((u32, &\'a str), bool)> = Vec::new();\n    for rule in rules {'
OFF = '    let mut winners: Vec<((u32, &\'a str), bool)> = Vec::new();\n    for rule in rules.take(0) /*TOGGLE*/ {'
s = open(P).read()
a, b = (ON, OFF) if sys.argv[1] == 'off' else (OFF, ON)
assert s.count(a) == 1, s.count(a)
open(P, 'w').write(s.replace(a, b))
print('revert-layer is', sys.argv[1])
