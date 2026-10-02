"""Body for the background-image-fetch PR -> pr-bgf-final.md"""
D = '/Users/petecopeland/Repos/.worktrees/trench-realsite/scratch/s1002e'
head = open(f'{D}/pr-bgf-head.md').read()
failfirst = """thread 'background_image_fetch_tests::a_css_background_image_is_fetched' panicked at crates/rustkit-engine/src/lib.rs:28424:9:
assertion `left == right` failed: one request per background the shield allows, each with the policy's Referer; none for the blocked one, none for a box that is not rendered
  left: []
 right: [("/cross.png", Some("http://127.0.0.1:63390/")), ("/shown.png", Some("http://127.0.0.1:63390/page?q=1"))]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 377 filtered out"""
onbranch = "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 377 filtered out; finished in 6.26s"
fixture = """e0    develop   5.56%  fix   0.00%  |  background-image: url(dot.png); background-repeat: no-repeat; background-position: center
e1    develop   5.56%  fix   0.00%  |  background: url(dot.png) no-repeat
e2    develop   5.56%  fix   0.00%  |  background: url(dot.png) no-repeat center
e3    develop   5.56%  fix   0.00%  |  background: url(dot.png) no-repeat right bottom
e4    develop  33.33%  fix   0.00%  |  background: url(dot.png) repeat-x
e5    develop  16.67%  fix  29.17%  |  background: url(dot.png) no-repeat 10px 20px / 40px 30px
e6    develop   5.56%  fix   0.00%  |  background: #ff0 url(dot.png) no-repeat center
e7    develop   0.00%  fix   0.00%  |  background: linear-gradient(blue, blue) no-repeat 20px 10px / 30px 15px
e8    develop 100.00%  fix   0.00%  |  background: url(dot.png) center / cover no-repeat
e9    develop  50.00%  fix   0.00%  |  background: url(dot.png) center / contain no-repeat #eee
e10   develop 100.00%  fix   0.00%  |  background: url(dot.png)
e11   develop   5.56%  fix   0.00%  |  background: no-repeat center url(dot.png)
e12   develop   5.56%  fix   0.00%  |  background: url(dot.png) 50% 50% no-repeat, linear-gradient(#cfc, #cfc)
e13   develop   5.56%  fix   6.60%  |  background: url(dot.png) no-repeat; background-position: right 5px bottom 5px
e14   develop 100.00%  fix   0.00%  |  background-position: center; background-repeat: no-repeat; background: url(dot.png)
boxes more than 1% off Chrome: develop 14 of 15, fix 2 of 15"""
for k, v in (('FAILFIRST', failfirst), ('ONBRANCH', onbranch), ('FIXTURE', fixture)):
    assert k in head
    head = head.replace(k, v)
receipt = open(f'{D}/receipt-bgf.md').read()
ab = ''.join(l for l in open(f'{D}/ab-bgf.txt') if l.strip() != 'DONE')
ab2 = open(f'{D}/ab-bgf2.txt').read()
body = f"""{head}e5 and e13 are the length positions named above: the image is now there and sits at the corner, where develop painted nothing. e5 is worse than develop by that measure (16.67 -> 29.17%) until the length-position branch lands.

A second served page (`scratch/s1002e/ext/`: the rules in an external sheet one directory down, with `../img/` URLs) confirms what is and is not asked for: a relative URL resolves against the sheet, and a `var()` value, a `::before` box, a rule under `@media`, a layer beside a gradient, a root-relative URL and an inline `style` are all fetched. **`image-set()` and `-webkit-image-set()` are not**: no request, no image (not handled by the background parser; not in this PR).

## Campaign receipt

Arms: develop **f0d5fa2** (the branch's merge base) vs fix **91af4e9**, both release binaries built in this session through `cargo-serial` with every workspace source touched first. (The fix binary was built at the merge commit 42e9f7a plus the test-only change that 91af4e9 commits.) develop has since moved to e006c68 (#440, #441); not merged in.

{receipt}
The fix arm's first `all` run failed one capture under load (`chrome_rustkit`, NOT-MEASURED); the scope was re-run alone and is the row above. The campaign's pages use local files and `data:` images, so this says nothing else moved, not that the fetch is right; the test and the fixture are for that.

## Real sites: no board site moves, and why

RustKit frames, develop / fix / develop / fix on each of the board's 20 live URLs at 1280x800 (`scratch/s1001g/ab.py` on the hub branch), load 12 to 25 with the other lanes building. "Within" is a site's own variance between two captures of one arm; "across" is the four develop-fix pairs.

```
{ab}```

Second attempt at four of the sites that ran out the 30 s limit:

```
{ab2}```

- **Pixel-identical across arms on every pair captured:** google (A1 = B1 = B2; A2 is another page of its own), youtube, wikipedia, lyft, reddit, x, yahoo, bing (same), walmart, microsoft, weather, apple (second attempt), facebook and shopify (one pair each).
- **linkedin:** B2 is the site's other page variant ("Welcome to your professional community", 40.8% from the other three, which are within 2.96% of each other across arms). Its own variance; I looked at the frames.
- **netflix:** 2.17 to 2.29% across, 2.27% within the fix arm: its own variance.
- **Not captured inside the binary's 30 s on either arm:** instagram, github, squarespace, cnn (the heaviest pages; the machine was at load 12 to 25).
- Against the stored Chrome frames (`scratch/s1001g/vs_chrome.py`), every site captured scores the same on both arms.

**Why nothing moves** (measured, `scratch/s1002e/bg_census.py`: the `background_image` commands in each site's display list on the fix binary): on google, lyft, x, linkedin, weather, microsoft, reddit, youtube and walmart there is **no** `url()` background command at all; bing has one, a `data:` SVG; wikipedia has 99, all http SVG (external-link icons, none in the first viewport). So on the eleven board sites censused there is not one http raster background for this PR to fetch. Either those pages have none in Chrome either, or the cascade drops them before the display list (`image-set()` is one such form, shown above). Which, per site, needs Chrome's side of the count; that is the image census, next.

So this PR is a correctness fix with a measured null on the board, not the visible change the 05:25 digest expected. The weather page is pixel-identical across arms on all four pairs.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
"""
open(f'{D}/pr-bgf-final.md', 'w').write(body)
print(len(body))
