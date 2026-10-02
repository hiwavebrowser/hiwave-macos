"""Replace rs-bg-fetch's background_image_fetch_tests module with ../s1002e_test.rs."""
import pathlib
p = pathlib.Path('/Users/petecopeland/Repos/.worktrees/rs-bg-fetch/crates/rustkit-engine/src/lib.rs')
s = p.read_bytes().decode()
a = s.index('#[cfg(all(test, feature = "headless"))]\nmod background_image_fetch_tests {')
b = s.index('#[cfg(test)]\nmod svg_image_tests {')
new = pathlib.Path('/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1002e_test.rs').read_text()
p.write_bytes((s[:a] + new + '\n' + s[b:]).encode())
print('replaced', b - a, 'bytes with', len(new))
