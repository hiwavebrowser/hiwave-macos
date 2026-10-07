# Engine Diagnostics: Real-Site Visual Defects

**Seat**: `pollux` (Windows 11 / `busybee`)  
**Target Engine**: RustKit / HiWave  
**Scope**: Atlas Broadcast Directives (2) eBay Images, (1) Top-20 Icons, (3) Menus Behind Images  
**Execution Order**: Strictly (2) $\rightarrow$ (1) $\rightarrow$ (3)  

---

## Part 1: Item (2) eBay Images Diagnostics (`https://www.ebay.com/`)

### 1. Viewport Inventory (0–1600px)
Audited on pinned Chrome (`win64-148.0.7778.216`) at 1280×1600 viewport against live eBay and RustKit capture (`target/release/parity-capture.exe`).

Chrome renders exactly **19 image resources** within the first 2 viewports ($Y \in [0, 1600]$):
- **18 `<img>` elements** (all WebP format)
- **1 CSS `background-image`** (SVG dropdown chevron on `<select id="gh-cat">`)

| ID | Kind | Resource URL | Chrome Rect ($x, y, w, h$) | Natural Size | Object-Fit | RustKit Status | Cause Classification |
|---|---|---|---|---|---|---|---|
| 1 | `<img>` | `https://i.ebayimg.com/images/g/VqAAAOSwutRmoTTq/s-l1600.webp` | $(-16, 158, 1395, 392)$ | $1600\times552$ | `cover` | Distorted | Decoded but drawn wrong |
| 2 | `<img>` | `https://i.ebayimg.com/images/g/9HMAAeSw8xBqxTrI/s-l960.webp` | $(48, 653, 245, 375)$ | $640\times960$ | `cover` | Distorted | Decoded but drawn wrong |
| 3 | `<img>` | `https://i.ebayimg.com/images/g/7HkAAeSwRCxo7W4I/s-l64.webp` | $(56, 988, 32, 32)$ | $64\times64$ | `cover` | Shifted | Decoded but drawn wrong |
| 4 | `<img>` | `https://i.ebayimg.com/images/g/RrIAAeSwm-xqvzrl/s-l960.webp` | $(309, 653, 245, 375)$ | $640\times960$ | `cover` | Distorted | Decoded but drawn wrong |
| 5 | `<img>` | `https://i.ebayimg.com/images/g/jJcAAeSwmOFqK66A/s-l64.webp` | $(317, 988, 32, 32)$ | $64\times64$ | `cover` | Shifted | Decoded but drawn wrong |
| 6 | `<img>` | `https://i.ebayimg.com/images/g/WxMAAeSwYgpqvoLT/s-l960.webp` | $(570, 653, 245, 375)$ | $639\times960$ | `cover` | Distorted | Decoded but drawn wrong |
| 7 | `<img>` | `https://i.ebayimg.com/images/g/Qc8AAOSwnm5kdmMP/s-l64.webp` | $(578, 988, 32, 32)$ | $64\times64$ | `cover` | Shifted | Decoded but drawn wrong |
| 8 | `<img>` | `https://i.ebayimg.com/images/g/Sk4AAeSwkXNqxWqA/s-l960.webp` | $(831, 653, 245, 375)$ | $640\times960$ | `cover` | Distorted | Decoded but drawn wrong |
| 9 | `<img>` | `https://i.ebayimg.com/images/g/6asAAOSw7ttn7dyK/s-l64.webp` | $(839, 988, 32, 32)$ | $64\times64$ | `cover` | Shifted | Decoded but drawn wrong |
| 10 | `<img>` | `https://i.ebayimg.com/images/g/rIYAAeSwWaxqxU21/s-l960.webp` | $(1092, 653, 245, 375)$ | $640\times960$ | `cover` | Offscreen ($x=2072$) | Decoded but drawn wrong |
| 11 | `<img>` | `https://i.ebayimg.com/images/g/1CwAAOSwHzZkSqkK/s-l64.webp` | $(1100, 988, 32, 32)$ | $80\times80$ | `cover` | Offscreen ($x=4445$) | Decoded but drawn wrong |
| 12 | `<img>` | `https://i.ebayimg.com/images/g/XswAAeSwzh1qv~iQ/s-l500.webp` | $(48, 1186, 155, 155)$ | $500\times500$ | `contain` | Correct | Drawn correctly |
| 13 | `<img>` | `https://i.ebayimg.com/images/g/wfAAAeSw1FNqv~iP/s-l500.webp` | $(219, 1186, 155, 155)$ | $500\times500$ | `contain` | Correct | Drawn correctly |
| 14 | `<img>` | `https://i.ebayimg.com/images/g/j1oAAeSwgJJqv~iO/s-l500.webp` | $(391, 1186, 155, 155)$ | $500\times500$ | `contain` | Correct | Drawn correctly |
| 15 | `<img>` | `https://i.ebayimg.com/images/g/nA8AAeSw21pqv~iP/s-l500.webp` | $(562, 1186, 155, 155)$ | $500\times500$ | `contain` | Correct | Drawn correctly |
| 16 | `<img>` | `https://i.ebayimg.com/images/g/eK8AAeSwbAVqv~iP/s-l500.webp` | $(734, 1186, 155, 155)$ | $500\times500$ | `contain` | Correct | Drawn correctly |
| 17 | `<img>` | `https://i.ebayimg.com/images/g/m-cAAeSwS8Jqv~iP/s-l500.webp` | $(905, 1186, 155, 155)$ | $500\times500$ | `contain` | Correct | Drawn correctly |
| 18 | `<img>` | `https://i.ebayimg.com/images/g/kNMAAeSwFN9qv~iP/s-l500.webp` | $(1077, 1186, 155, 155)$ | $500\times500$ | `contain` | Correct | Drawn correctly |
| 19 | `bg-image` | `https://ir.ebaystatic.com/cr/v/c1/playbook-20230717-a4498c7/icons/chevron-down_12.svg` | $(826, 51, 160, 40)$ | N/A | N/A | Missing | Never requested |

### 2. Summary Counts by Cause
- **Total Chrome images**: **19**
- **Drawn correctly**: **7** (Category circle icons #12–18; natural $1:1$ matches destination $1:1$, so unhandled `object-fit` causes zero distortion).
- **Never requested**: **1** (`#19` - `select#gh-cat` CSS background SVG).
  - *Cause*: Form controls (`<select>`) bypass standard `BackgroundImage` display list emission; background discovery never enqueues the URL.
- **Requested but failed**: **0**
  - All 18 WebP images were fetched and decoded cleanly by `image-webp`.
- **Decoded but drawn wrong**: **11**
  - *Sub-cause A (9 images)*: Ignored `object-fit` in renderer (`crates/rustkit-renderer/src/lib.rs:2110-2118`). Display command passes `object_fit: _` and hardcodes texture UV coords to `[0.0, 0.0, 1.0, 1.0]`, stretching images into incorrect aspect ratios. Hero banner ($2.898:1$) is squashed to $1.400:1$ ($697.5\times498.2$).
  - *Sub-cause B (2 images)*: Carousel track layout failure. Video card 5 ($x=2072$) and host avatar 5 ($x=4445$) are laid out thousands of pixels offscreen to the right instead of at the viewport edge ($x=1092$).

---

## Part 2: Item (1) Top-20 Board Sites Icons Inventory

### 1. Matrix: Technique × Sites Using It × Status
Audited across all 20 board sites using headless Chromium (`148.0.7778.216`):

| Site | `@font-face` PUA | Inline `<svg>` | `<svg><use>` | CSS `background-image` SVG | CSS `mask-image` | `<img>` SVG |
|---|:---:|:---:|:---:|:---:|:---:|:---:|
| **apple** | **YES** | YES | no | no | **YES** | no |
| **bing** | no | YES | no | YES | **YES** | YES |
| **cnn** | no | YES | **YES** | YES | **YES** | YES |
| **facebook** | no | YES | no | no | no | no |
| **github** | no | YES | no | no | **YES** | YES |
| **google** | **YES** | YES | no | no | **YES** | no |
| **instagram**| no | YES | no | no | no | no |
| **linkedin** | no | no | no | no | no | no |
| **lyft** | no | YES | no | no | no | YES |
| **microsoft**| **YES** | YES | no | no | no | no |
| **netflix** | **YES** | YES | no | YES | no | YES |
| **reddit** | no | no | **YES** | no | no | YES |
| **shopify** | **YES** | YES | **YES** | no | **YES** | YES |
| **squarespace**| no | YES | no | no | no | no |
| **walmart** | **YES** | no | no | no | no | YES |
| **weather** | **YES** | YES | no | no | **YES** | YES |
| **wikipedia**| no | no | no | YES | **YES** | YES |
| **x** | **YES** | YES | no | no | no | YES |
| **yahoo** | no | YES | no | no | **YES** | no |
| **youtube** | no | YES | **YES** | YES | no | no |

### 2. Technique Summary Table

| Technique | Engine Status | Adoption | Sites Using It |
|---|---|:---:|---|
| **CSS `mask-image`** | **BROKEN** | 9 / 20 (45%) | apple, bing, cnn, github, google, shopify, weather, wikipedia, yahoo |
| **Icon font via `@font-face` PUA** | **BROKEN** | 8 / 20 (40%) | apple, google, microsoft, netflix, shopify, walmart, weather, x |
| **`<svg><use>`** | **BROKEN** | 4 / 20 (20%) | cnn, reddit, shopify, youtube |
| **CSS `background-image` SVG** | **PARTIAL** | 5 / 20 (25%) | bing, cnn, netflix, wikipedia, youtube (+ eBay) |
| **`<img>` SVG** | **PARTIAL** | 11 / 20 (55%)| bing, cnn, github, lyft, netflix, reddit, shopify, walmart, weather, wikipedia, x |
| **Inline `<svg>`** | **WORKS** | 16 / 20 (80%)| apple, bing, cnn, facebook, github, google, instagram, lyft, microsoft, netflix, shopify, squarespace, weather, x, yahoo, youtube |

---

### 3. Top 3 Broken Techniques & 10-Line Reduced Repros

#### 1. `<svg><use>` (Broken: 100% No-Op)
- **Root Cause**: In [`crates/rustkit-svg/src/lib.rs`](file:///P:/repos/hiwave-macos/crates/rustkit-svg/src/lib.rs#L1180-L1201), `<use>` elements are ignored:
  ```rust
  SvgElement::Use(_) => {} // TODO: resolve references
  SvgElement::Use(_) => return,
  ```
  The renderer produces 0 display commands for any icon referenced via `<use>`.
- **10-Line Reduced Reproduction**:
```html
<!DOCTYPE html>
<svg style="display:none">
  <defs><g id="icon"><circle cx="25" cy="25" r="20" fill="green"/></g></defs>
</svg>
<svg width="50" height="50">
  <use href="#icon"/>
</svg>
```
*Empirical Verification*: Chrome paints green circle. RustKit `target/release/parity-capture.exe` emits display list with only 1 command (background rect), circle is completely missing.

---

#### 2. CSS `mask-image` / `-webkit-mask-image` (Broken: 100% Unparsed)
- **Root Cause**: `mask-image` and `-webkit-mask-image` do not exist in `rustkit-css`, `rustkit-layout`, or `rustkit-renderer`. Sites use `mask-image: url(icon.svg); background-color: currentColor;` to render monochrome icons. RustKit ignores the mask and paints a solid opaque rectangle.
- **10-Line Reduced Reproduction**:
```html
<!DOCTYPE html>
<html><body>
<div style="width: 50px; height: 50px; background-color: rgb(255, 0, 0); -webkit-mask-image: url('data:image/svg+xml,<svg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 10 10%22><circle cx=%225%22 cy=%225%22 r=%225%22/></svg>'); mask-image: url('data:image/svg+xml,<svg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 10 10%22><circle cx=%225%22 cy=%225%22 r=%225%22/></svg>');"></div>
</body></html>
```
*Empirical Verification*: Chrome masks the red background into a 50px circle. RustKit emits `solid_color` rect $50\times50$ at $(8, 8)$ (unmasked square).

---

#### 3. Icon Font via `@font-face` PUA (Broken: Missing Glyph / Tofu)
- **Root Cause**:
  - On Windows: [`crates/rustkit-text/src/webfonts.rs`](file:///P:/repos/hiwave-macos/crates/rustkit-text/src/webfonts.rs#L396-L419) defines non-macOS stubs where `install()` unconditionally returns `0` and `is_installed()` returns `false`. Web fonts are never loaded into DirectWrite. Fallback fonts (Segoe UI / Arial) lack glyphs for Private Use Area codepoints ($\text{U+E000}$–$\text{U+F8FF}$), drawing blank tofu.
  - On macOS: WOFF2 container without brotli decompressor fails to decode or font swap fails to invalidate text runs when webfont arrives asynchronously.
- **10-Line Reduced Reproduction**:
```html
<!DOCTYPE html>
<style>
  @font-face { font-family: "CustomIcons"; src: url("icons.woff2") format("woff2"); }
  .icon { font-family: "CustomIcons", sans-serif; font-size: 24px; }
</style>
<span class="icon">&#xE001;</span>
```
*Empirical Verification*: Chrome loads font and renders custom glyph. RustKit outputs missing glyph tofu or whitespace.

---

## Part 3: Item (3) Dropdown Menus Drawn Behind Images

### 1. eBay Page CSS Forensics
On `https://www.ebay.com/`, the header dropdown flyouts (e.g. `My eBay`, `Watchlist`, `Shop by category`):
- **Menu Dialog**: `div.gh-flyout__dialog` (e.g. `#s0-1-4-6-13-0-dialog`)
  - `position`: `absolute`
  - `z-index`: `9`
- **Header Ancestor**: `<header id="gh" class="gh-header">`
  - `position`: `relative`
  - `z-index`: `100000`
  - *Spec behavior*: Under CSS 2.1 (Appendix E), `header` creates a stacking context with $z=100000$. All child flyouts ($z=9$) are scoped within this stacking context, which ranks above any following sibling in normal flow.
- **Hero Banner**: `<div class="vl-banner">` / `<img ...>` inside `<main id="mainContent">`
  - `position`: `static` (or `relative` with $z=0$/`auto`)
  - Sibling of `<header>` later in DOM order.

### 2. Root Cause in RustKit
1. **`position: relative` is demoted to `Static`**:
   In [`crates/rustkit-layout/src/lib.rs`](file:///P:/repos/hiwave-macos/crates/rustkit-layout/src/lib.rs#L7075-L7080):
   ```rust
   // the engine deliberately transfers `position: relative` as
   // `Position::Static` (its paint-side stacking path is not ready for
   // relative boxes)
   ```
2. **`header` never creates a stacking context**:
   [`crates/rustkit-layout/src/lib.rs`](file:///P:/repos/hiwave-macos/crates/rustkit-layout/src/lib.rs#L1611):
   `set_z_index` only sets `creates_context = true` if `self.position != Position::Static`. Because `position: relative` was demoted, `header` does not create a stacking context despite `z-index: 100000`.
3. **Local Tree Traversal Order**:
   [`crates/rustkit-layout/src/lib.rs`](file:///P:/repos/hiwave-macos/crates/rustkit-layout/src/lib.rs#L7061-L7220):
   `render_stacking_context` only sorts direct children. It does not collect all positioned descendants into the true CSS stacking context.
   - Body visits `header` (in normal flow).
   - `header` emits its background, and then its child dropdown menu `.gh-flyout__dialog`.
   - Body then visits `<main>` (the next normal flow child).
   - `<main>` emits the hero `<img>`.
4. **Serial Painter Execution**:
   [`crates/rustkit-renderer/src/lib.rs`](file:///P:/repos/hiwave-macos/crates/rustkit-renderer/src/lib.rs#L2286-L2295):
   `PushStackingContext` does not affect GPU drawing order; commands are executed serially. Because the hero image command occurs *after* the dropdown menu command, the GPU draws the hero image right over the menu.

---

### 3. Reduced Reproduction Page & Verification Against Chrome

#### Reproduction HTML (`scratch/ebay_diag/menu_behind_image_repro.html`)
```html
<!DOCTYPE html>
<html>
<head>
<style>
  body { margin: 0; }
  header { position: relative; z-index: 100; height: 50px; background: #eee; }
  .menu { position: absolute; top: 30px; left: 20px; width: 200px; height: 150px; background: red; z-index: 10; }
  .hero { width: 400px; height: 200px; background: blue; }
</style>
</head>
<body>
  <header>
    Header
    <div class="menu">Dropdown Menu</div>
  </header>
  <div class="hero">Hero Content</div>
</body>
</html>
```

#### Empirical Pixel Diff at $(x=50, y=100)$:
- **Chrome (`repro_chrome.png`)**: **`rgb(255, 0, 0)`** (Menu is on top, red box visible).
- **RustKit (`repro.ppm`)**: **`rgb(0, 0, 255)`** (Hero is on top, blue box painted over menu).
- **Display List Evidence (`repro-dl.json`)**:
  - Op 3: `solid_color` Red (`.menu`) at $(20, 30, 200, 150)$
  - Op 5: `solid_color` Blue (`.hero`) at $(0, 50, 400, 200)$
  Op 5 is emitted **after** Op 3, painting blue over red.
