```
all:      develop 2026-10-02T10:05:03 26/26 avg 1.1069 | fix 2026-10-02T10:14:20 26/26 avg 1.0928 | 25/26 identical
builtins: develop 2026-10-02T10:01:11 5/5 avg 1.8139 | fix 2026-10-02T10:10:53 5/5 avg 1.8139 | 5/5 identical
micro:    develop 2026-10-02T10:00:33 13/13 avg 0.6548 | fix 2026-10-02T10:37:39 13/13 avg 0.6266 | 12/13 identical
```

`ratchet_local.py` (CI's Gate A / Gate B / ratchet): the lines that differ, develop then fix:
```
dev: form-controls: geo_fails=43 paint=0.95576 discrete=0
fix: form-controls: geo_fails=32 paint=0.95949 discrete=0
```

<details><summary>Per-case diff_pct (all 26), develop 60f7d39 vs fix 2f2e463</summary>

| case | develop | fix |  |
|---|---|---|---|
| about | 3.6742 | 3.6742 |  |
| article-typography | 4.7857 | 4.7857 |  |
| backgrounds | 1.2163 | 1.2163 |  |
| bg-pure | 0.0000 | 0.0000 |  |
| bg-solid | 0.2106 | 0.2106 |  |
| card-grid | 1.3034 | 1.3034 |  |
| chrome_rustkit | 1.1234 | 1.1234 |  |
| combinators | 0.6547 | 0.6547 |  |
| css-selectors | 1.3299 | 1.3299 |  |
| flex-positioning | 0.6327 | 0.6327 |  |
| form-controls | 3.2388 | 2.8712 | moved |
| form-elements | 0.9535 | 0.9535 |  |
| gpu-gradient-regression | 0.3903 | 0.3903 |  |
| gradient-backgrounds | 1.0133 | 1.0133 |  |
| gradient-no-radius | 0.5204 | 0.5204 |  |
| gradient-radius-only | 0.3429 | 0.3429 |  |
| gradients | 0.1412 | 0.1412 |  |
| image-gallery | 0.5217 | 0.5217 |  |
| images-intrinsic | 0.3267 | 0.3267 |  |
| new_tab | 1.1019 | 1.1019 |  |
| pseudo-classes | 0.4341 | 0.4341 |  |
| rounded-corners | 0.4354 | 0.4354 |  |
| settings | 2.0919 | 2.0919 |  |
| shelf | 1.0781 | 1.0781 |  |
| specificity | 0.6015 | 0.6015 |  |
| sticky-scroll | 0.6575 | 0.6575 |  |

</details>

<details><summary>Per-case diff_pct (builtins), develop 60f7d39 vs fix 2f2e463</summary>

| case | develop | fix |  |
|---|---|---|---|
| about | 3.6742 | 3.6742 |  |
| chrome_rustkit | 1.1234 | 1.1234 |  |
| new_tab | 1.1019 | 1.1019 |  |
| settings | 2.0919 | 2.0919 |  |
| shelf | 1.0781 | 1.0781 |  |

</details>

<details><summary>Per-case diff_pct (micro), develop 60f7d39 vs fix 2f2e463</summary>

| case | develop | fix |  |
|---|---|---|---|
| backgrounds | 1.2163 | 1.2163 |  |
| bg-pure | 0.0000 | 0.0000 |  |
| bg-solid | 0.2106 | 0.2106 |  |
| combinators | 0.6547 | 0.6547 |  |
| form-controls | 3.2388 | 2.8712 | moved |
| gpu-gradient-regression | 0.3903 | 0.3903 |  |
| gradient-no-radius | 0.5204 | 0.5204 |  |
| gradient-radius-only | 0.3429 | 0.3429 |  |
| gradients | 0.1412 | 0.1412 |  |
| images-intrinsic | 0.3267 | 0.3267 |  |
| pseudo-classes | 0.4341 | 0.4341 |  |
| rounded-corners | 0.4354 | 0.4354 |  |
| specificity | 0.6015 | 0.6015 |  |

</details>
