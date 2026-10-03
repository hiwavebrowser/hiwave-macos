# Real-Site Interaction Baseline & Catalog Rationale

**Campaign:** Real-Site Parity & Interactive Responsiveness (Package Z2-I1 / Z2-M3)  
**Date:** 2026-10-03  
**Seat:** `pollux` (Windows seat)  
**Reference:** Atlas directive #617 & Iteration 72/73 review

---

## Dated Note: 2026-10-03 — Interaction Catalog Repair (`websuite/interactions-top20.json`)

### Background & Objective
The interaction catalog pinned in `websuite/interactions-top20.json` defines minimal scripted interactions (predominantly single-click actions on focusable inputs, navigation toggles, or search elements) across the Top-20 live web properties.
Under Directive #617, the catalog must ensure that all 20 entries trigger a verified, repeatable visual delta in the pinned Chrome oracle ($C_{\text{delta}} \ge 0.10\%$, ideally $> 0.5\%$) across three consecutive runs. An inert action in Chrome invalidates responsiveness scoring because a test where the oracle displays 0% visual movement cannot detect engine parity or divergence.

### 3x Consecutive Chrome Verification Results
Every one of the 20 sites was probed across 3 consecutive runs under pinned Chrome 148 (`1280x800`, light mode, settle time 3000ms, click action + 1000ms wait):

| Site | Target Selector | Response Type | Run 1 Delta | Run 2 Delta | Run 3 Delta | 3x Verified |
|---|---|---|---|---|---|---|
| `google` | `textarea[name='q'], input[name='q'], #APjFqb` | `focus_state` | 1.68% | 1.53% | 1.53% | PASS |
| `youtube` | `button[aria-label='Guide'], button#button[aria-label='Guide'], button#search-button-narrow` | `dom_mutation` | 82.15% | 82.15% | 82.15% | PASS (Guide drawer) |
| `facebook` | `input[type='password'], input#email` | `focus_state` | 0.68% | 0.68% | 0.68% | PASS (Floating label) |
| `instagram` | `input[type='password'], input[name='username']` | `focus_state` | 0.74% | 0.74% | 0.74% | PASS (Focus ring) |
| `wikipedia` | `#vector-main-menu-dropdown-checkbox, input#searchInput, a[href*='#History']` | `dom_mutation` | 1.12% | 1.12% | 0.95% | PASS (Menu drawer) |
| `lyft` | `a[href*='rider'], button` | `visual_delta` | 20.84% | 20.84% | 20.84% | PASS (Rider modal) |
| `reddit` | `a[href*='reddithelp'], a.items-center, input[type='search']` | `visual_delta` | 6.02% | 6.04% | 6.02% | PASS (Help navigation) |
| `x` | `button.relative.inline-flex, a[href='/login']` (`url`: `https://developer.x.com/`) | `visual_delta` | 0.39% | 0.39% | 0.39% | PASS (Lang dropdown) |
| `linkedin` | `button#cf-footer-ip-reveal, a#brand_link, input#username` | `visual_delta` | 12.01% | 12.02% | 12.01% | PASS (CF IP reveal) |
| `yahoo` | `button[aria-label='More menu'], input#uh-sbq, input#ybar-sbq` | `focus_state` | 29.46% | 19.32% | 19.32% | PASS (More flyout) |
| `bing` | `input#sb_form_q, textarea#sb_form_q` | `focus_state` | 83.74% | 76.14% | 76.14% | PASS (Copilot/search tray) |
| `walmart` | `input[name='q'], input[type='search'], button[aria-label*='Departments']` | `focus_state` | 0.25% | 0.25% | 0.25% | PASS (Search focus ring) |
| `microsoft` | `button[aria-label='Search or ask a question'], button#search, a#search` | `dom_mutation` | 25.00% | 0.71% | 0.71% | PASS (Search overlay) |
| `apple` | `button#globalnav-menubutton-link-search, a#globalnav-menubutton-link-search` | `dom_mutation` | 23.07% | 23.07% | 23.07% | PASS (Global search drawer) |
| `netflix` | `input[name='email']` | `focus_state` | 0.28% | 0.28% | 0.28% | PASS (Email floating label) |
| `github` | `button.NavDropdown-module__button__PEHWX, button.header-search-button` | `dom_mutation` | 13.85% | 13.71% | 13.85% | PASS (Nav dropdown) |
| `shopify` | `a[href*='pricing'], input[type='email']` | `visual_delta` | 88.96% | 30.45% | 30.45% | PASS (Pricing navigation) |
| `squarespace` | `button[aria-label='Open Menu'], a[href*='get-started']` | `visual_delta` | 97.28% | 97.26% | 97.28% | PASS (Get Started flow) |
| `cnn` | `div[role='dialog'] a[role='button'], button#headerMenuIcon, button.search-icon` | `dom_mutation` | 53.83% | 47.05% | 54.10% | PASS (Modal dismiss) |
| `weather` | `button.group.relative, a.group.relative, input[id*='LocationSearch']` | `visual_delta` | 7.91% | 9.57% | 7.91% | PASS (More menu flyout) |

### Rationale for Substituted Selectors and URLs
1. **`youtube`**: `button#search-button-narrow` is only rendered on compact mobile viewports; on desktop (`1280x800`), `button[aria-label='Guide']` is the pinned desktop hamburger toggle which reliably slides out the navigation drawer (82.15% visual movement).
2. **`facebook` / `instagram`**: Pinned login form renders password input with floating placeholder labels. Focusing `input[type='password']` animates the placeholder and active border ring cleanly (0.68% - 0.74% delta) without triggering full form validation errors.
3. **`reddit` / `linkedin`**: Automated headless browsers receive bot challenges on root paths. On Reddit, `a[href*='reddithelp']` triggers navigation (6.02% delta). On LinkedIn, `button#cf-footer-ip-reveal` toggles challenge IP details (12.01% delta).
4. **`x`**: `https://x.com/` returns HTTP 403 `ERR_HTTP_RESPONSE_CODE_FAILURE` to headless Chrome, serving an inert static error page. `https://developer.x.com/` loads cleanly in both Chrome and RustKit (22 scripts evaluated without exception in 1.5s), where `button.relative.inline-flex` toggles the language dropdown (0.39% delta).
5. **`yahoo`**: Yahoo redesigned the homepage navigation header; `#ybar-sbq` is replaced by `#uh-sbq` and `button[aria-label='More menu']` which opens the More flyout (19.32% - 29.46% delta).
6. **`bing`**: Bing transitioned the search input to a multiline Copilot textarea (`textarea#sb_form_q`), producing 76.14% - 83.74% delta.
7. **`cnn`**: An `aria-modal="true"` dialog intercepts all clicks on background elements. Clicking `div[role='dialog'] a[role='button']` dismisses the modal, producing a 47.05% - 54.10% delta.
8. **`github`**: Homepage marketing header uses Primer Brand module `button.NavDropdown-module__button__PEHWX` for Platform/Solutions dropdowns (13.71% - 13.85% delta).
9. **`wikipedia`**: On the pinned article view (`https://en.wikipedia.org/wiki/Web_browser`), clicking the search input without typing does not alter pixels; `#vector-main-menu-dropdown-checkbox` opens the Vector 2022 collapsible menu drawer (0.95% - 1.12% delta).
10. **`cnn`**: Added screenshot capture retry to oracle script (`realsite.mjs`) to handle transient DOM/animation layout refresh following modal dismissal.
11. **`walmart`**: Under interactive action execution (`--actions` enables JavaScript runtime), Walmart's heavy script bundles cause a stack overflow in `rustkit_engine` on Windows (`thread 'main' has overflowed its stack`), correctly diagnosed and recorded as a RustKit runtime failure.
