"""New body for #429: the old body with the draft paragraph replaced, the receipt and real-site sections
rewritten for develop 60f7d39 vs the merged head 2f2e463."""
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1002d'
old = open(f'{D}/pr429-body-before.md').read()
a = old.index('## What')
b = old.index('## Campaign receipt')
middle = old[a:b]
middle = middle.replace(
    "On the branch: `cargo test -p rustkit-layout --lib` 602/602; `cargo test -p rustkit-engine --lib button_children_tests` 4/4. The whole engine suite was not run locally (other lanes were building; its GPU guard times out under load), so CI's `unit-suites` is its first full run.",
    "On the branch at eda63a8 (develop 97393a7 merged in): `cargo test -p rustkit-layout --lib` 605/605 and the engine's five button tests pass. Not re-run locally at 2f2e463, which adds only #435 and a design document from develop; CI's `unit-suites` is the full run at this head.")
assert 'eda63a8' in middle
top = """**Ready for review.** This was a draft because giving a button its default face through the cascade exposed three places where the engine dropped an author's reset of that face. All three are now on develop and merged into this branch (2f2e463, develop 60f7d39): #433 (the `background` shorthand clears the colour it does not name), #434 (the shorthand's layers) and #435 (`#0000`, the form minified sheets write `transparent` in). The two real-site regressions the draft showed are gone: weather.com's "More" button and squarespace's three navigation buttons no longer get a grey face (below). The receipt and the real-site table are re-run at this head; the fixture tables under **What** are from the first head and were not re-measured.

"""
receipt = open(f'{D}/receipt-btn.md').read()
tail = f"""## Campaign receipt

Arms: develop **60f7d39** vs fix **2f2e463** (this branch with develop 60f7d39 merged in), both release binaries built in this session with every workspace source touched first. The micro scope of the fix arm was run twice: the first run failed one capture (`images-intrinsic`) while another build had the machine at load 12, and the re-run measured 13/13.

{receipt}
The one mover is `form-controls`, 3.2388 -> 2.8712%, and its geometry gate goes from 43 failures to 32, as on the first head.

## Real sites

RustKit frames, develop / fix / develop / fix on each live URL at 1280x800 (`scratch/s1001g/ab.py` on the hub branch), load 10. "Across arms" is the share of pixels where any channel differs by more than 8. Each was then scored with the board's own diff against the Chrome frames stored by the 2026-10-01 14:54 quiet board.

```
site         within develop  within fix  across arms              against Chrome: develop -> fix
weather          0.00%         0.00%      0.02% (all four pairs)   30.5417% -> 30.5412%
squarespace      0.00%           -        0.00% (two pairs)        77.2910% -> 77.2910%
walmart          0.00%         0.00%      0.90% (all four pairs)   91.0489% -> 91.0375%
github             -             -        0.15% (one pair)         16.2405% -> 16.2756%
```

- **weather:** the "More" pill's grey face is gone (it was 0.83% across arms on the draft's head). What is left is 206 pixels at (575, 24) to (634, 43): a small button in the header that has a grey face on both arms and is 2px larger on the fix.
- **squarespace:** pixel-identical across arms (1.94% on the draft's head: the three navigation faces). One of the two fix captures and both github second captures did not finish inside the binary's 30 s.
- **walmart:** 9,206 pixels. Two buttons with element children take the default box: a 2px grey frame at the top left, and a grey pill face behind an icon button at the lower left. Slightly closer to Chrome's frame by the board's diff, on a page that is 91% off on both arms. I did not check which rule Chrome uses to clear that pill's face.
- **github:** 1,556 pixels. The "Sign up for GitHub" button's 2px frame is grey on the fix and dark on develop; 359 pixels further from Chrome's stored frame (0.035%). Not looked into further.
- The sixteen other board sites were not re-captured at this head. On the draft's merged head eda63a8 against develop 97393a7 (all 20 captured): 12 pixel-identical; google, linkedin, bing and netflix within their own variance.
- No scoring board was run; no board check is expected to change.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
"""
open(f'{D}/pr429-body-new.md', 'w').write(top + middle + tail)
print(len(top + middle + tail))
