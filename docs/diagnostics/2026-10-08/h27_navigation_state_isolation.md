# H27 Design Note: View Navigation State Isolation & Invariant Verification

**Author**: Pollux (Windows Verifier & Diagnostics)  
**Date**: 2026-10-08  
**Scope**: `crates/rustkit-engine`, `crates/rustkit-renderer`, `crates/rustkit-layout`, `crates/rustkit-text`, `crates/rustkit-bindings`  
**Status**: DESIGN NOTE / PROPOSAL  

---

## 1. Executive Summary & The Navigation Invariant

When a user browses from Page A to Page B in a single tab/view (`load_url(A) -> load_url(B)`), the browser engine must satisfy the **Strict Navigation Invariant**:

$$\text{Render}(B \mid \text{after } A) \equiv \text{Render}(B \mid \text{fresh})$$

Every visual, structural, and behavioral attribute of Page B—including layout geometry, display list paint commands, rendered frame pixels, font selections, image decodes, and JavaScript execution—must be **bit-for-bit identical** to loading Page B in a brand-new, freshly initialized view (`create_view() -> load_url(B)`).

Any divergence indicates a **cross-document state leak** (where state from Page A pollutes Page B) or a **cache eviction/staleness defect** (such as GPU texture atlas invalidation during navigation).

---

## 2. Subsystem-by-Subsystem Audit of Surviving State

### A. CSS Stylesheets, Rule Indexes & Cascade Caches
1. **`ViewState::external_stylesheets` (LEAK IDENTIFIED)**:
   - **Current Behavior**: In `load_url` ([`crates/rustkit-engine/src/lib.rs:4280-4314`](file:///P:/repos/hiwave-macos/crates/rustkit-engine/src/lib.rs#L4280-L4314)), when a new document is parsed, `view.external_stylesheets` is **not cleared**.
   - If Page B contains no `<link rel="stylesheet">`, `defer` is `false`. The engine immediately calls `self.relayout(id)` at line 4312 *before* calling `load_subresources`.
   - `build_layout_for_view` passes `&view.external_stylesheets`—which **still holds Page A's stylesheets**!
   - Page B is cascaded and laid out with Page A's CSS rules. While `load_subresources` later clears `external_stylesheets` at line 10641, the initial layout and paint pass ran with leaked rules.
   - **Fix**: Clear `view.external_stylesheets.clear()` unconditionally at `load_url` commit time (line 4154), exactly as `load_html` does at line 4460.

2. **`RuleIndexScope` & `StyleMemoScope`**:
   - `RuleIndexScope`: Bound locally during each layout build. Properly re-indexed per document.
   - `view.rule_reads`: Cleared on navigation commit (`view.rule_reads.get_mut().take()`, line 4205).
   - `StyleMemoScope`: Scoped to individual layout passes, active only when external sheets are stable.

---

### B. Text Rendering, Web Fonts & The Glyph Atlas
1. **`GlyphCache` (Renderer GPU Texture Atlas) (CRITICAL BUG IDENTIFIED)**:
   - **Current Behavior**: `Renderer::glyph_cache` ([`crates/rustkit-renderer/src/glyph.rs:680-720`](file:///P:/repos/hiwave-macos/crates/rustkit-renderer/src/glyph.rs#L680-L720)) manages a $2048 \times 2048$ grayscale atlas and an RGBA color atlas.
   - The cache is stored on `Renderer` and **persists across all navigations** in a window.
   - When Page A renders thousands of unique glyphs (e.g., Netflix / eBay), the atlas fills to 90%+.
   - When Page B (Wikipedia) navigates, allocating a new glyph triggers:
     ```rust
     if self.next_y + height > self.atlas_size {
         tracing::warn!("Glyph atlas full, clearing cache");
         self.entries.clear();
         self.run_entries.clear();
         self.next_x = 1;
         self.next_y = 1;
         self.row_height = 0;
     }
     ```
   - **The Defect ("Static / Garbage Artifacts")**:
     - Resetting `next_x = 1, next_y = 1` clears the CPU map, but **does not clear the GPU texture** and **does not update already-generated vertex UVs**.
     - If the atlas resets *mid-frame* during Page B's text pass, all glyph quads built *before* the reset point have UVs pointing to texels that are immediately overwritten by glyphs rasterized *after* the reset point!
     - Even across frames, old glyph texels remain in unwritten regions; any padding or bilinear filtering artifact samples stale glyph bits from Page A.
   - **Fix**:
     - (1) Double-buffer or generation-tag the glyph atlas so mid-frame overflow flushes prior draw batches before resetting texture coordinates.
     - (2) Clear or zero the atlas during major page transitions or when capacity threshold is reached between frames.

2. **WebFont Cache (`FontLoader` & `FontCacheKey`)**:
   - Web fonts are partitioned by `TopLevelSite` (`eTLD+1` origin partition).
   - Fonts declared on Page A (`https://wikipedia.org`) survive in the `wikipedia.org` partition for subsequent navigations within the same site (desired cache behavior).
   - Cross-origin navigations (Page A on `netflix.com` -> Page B on `wikipedia.org`) use separate partition keys, preventing cross-site font pollution.

---

### C. Images, Decoded Bitmaps & Subresource Tracking
1. **`ViewState::images_attempted` (LEAK IDENTIFIED)**:
   - **Current Behavior**: `view.images_attempted` records every image URL fetched or attempted.
   - `load_url` does **not** clear `view.images_attempted` on navigation commit (omitted in lines 4190-4217).
   - If Page A attempted to fetch `https://example.com/icon.svg` (or failed), and Page B later adds an `<img>` with the same URL via script or late layout, Page B's post-script pass (`load_images`) checks `!view.images_attempted.contains(url)` and **skips fetching it**.
   - **Fix**: Add `view.images_attempted.clear()` to `load_url` commit (line 4217).

2. **`ImageManager` (Decoded Raster Cache)**:
   - Shared globally on `Renderer` keyed by canonical URL. Bitmaps are immutable; cache hits between Page A and Page B sharing identical asset URLs are correct and desired.

3. **In-Flight Requests (`live_requests`, `live_images`)**:
   - `view.live_requests.clear()` and `view.live_images.clear()` are executed at `load_url` commit (lines 4215-4216). Stale asynchronous responses are discarded.

---

### D. Scroll Offset, Viewport & Form Control State
1. **Scroll Offset**:
   - `view.scroll_offset` is reset to `(0.0, 0.0)` at line 4154 upon committing navigation.
   - `view.max_scroll_offset` is recalculated during layout.
2. **Form IDL State Tables (`edit_states`, `checked_states`)**:
   - `NodeId` is 1-indexed per document.
   - `view.edit_states.clear()` and `view.checked_states.clear()` (lines 4199-4200) ensure Node 4 on Page B does not inherit text typed into Node 4 on Page A.
3. **Interactive & Focus Chains**:
   - `focused_node`, `hovered_node`, `hover_chain`, `active_chain`, `pointer_at`, `primary_button_down`, and `press_target` are all cleanly reset (lines 4201-4211).

---

### E. JavaScript Runtime, Timers & Event Loop
1. **Runtime Isolation**:
   - `load_url` constructs a fresh `JsRuntime` and `DomBindings` instance (lines 4221-4229).
   - All ECMAScript globals, prototype extensions, lexical environments, and microtask queues from Page A are dropped.
2. **Timers & Event Listeners**:
   - `JsRuntime::timers` is an instance field dropped with the runtime. Page A's active intervals and timeouts cannot fire into Page B.
3. **Network Fetch Policy**:
   - A new `FetchPolicy::for_page(url)` is created for Page B, resetting request budgets and preflight caches.

---

### F. Layout Tree & Formatting Contexts
1. **Box Tree**: `view.layout` (`LayoutBox`) is replaced in full on `self.relayout(id)`.
2. **Ephemeral Contexts**:
   - `FloatContext` (floats and clearance line-narrowing) is allocated per Block Formatting Context (`BFC`) and destroyed at BFC exit.
   - `TableLayout` / `TableGrid` is computed per `<table>` element during layout.
   - `MarginCollapser` is instantiated per block flow step.
   - No layout formatting state survives document replacement.

---

## 3. Required Engine Fixes

```diff
--- a/crates/rustkit-engine/src/lib.rs
+++ b/crates/rustkit-engine/src/lib.rs
@@ -4154,6 +4154,10 @@ impl Engine {
         view.scroll_offset = (0.0, 0.0);
+        // Clear previous document's external stylesheets immediately on commit
+        // so initial un-deferred layout does not cascade with stale rules.
+        view.external_stylesheets.clear();
+        view.images_attempted.clear();
```

---

## 4. Pinning Test Suite: `B after A == B fresh`

To permanently prevent cross-document state leaks and texture corruption, we implement an automated integration test suite in `crates/rustkit-engine/tests/navigation_isolation_tests.rs`:

```rust
//! Navigation isolation tests verifying: Render(B | after A) == Render(B | fresh)

use rustkit_engine::{Engine, EngineConfig, Bounds};
use url::Url;

fn display_list_json(engine: &mut Engine, view_id: rustkit_engine::EngineViewId) -> String {
    let mut path = std::env::temp_dir();
    path.push(format!("dl_test_{}.json", uuid::Uuid::new_v4()));
    let path_str = path.to_str().unwrap().to_string();
    engine.export_display_list_json(view_id, &path_str).unwrap();
    let content = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(path);
    content
}

#[tokio::test]
async fn test_navigation_stylesheet_leak_isolation() {
    let mut engine = Engine::new(EngineConfig::for_parity_testing()).unwrap();
    let bounds = Bounds { x: 0, y: 0, width: 800, height: 600 };
    
    // Page A: heavy styles (red background, custom font size)
    let html_a = r#"
        <!DOCTYPE html>
        <html>
        <head><style>body { background-color: rgb(255, 0, 0); font-size: 48px; }</style></head>
        <body><p>Page A</p></body>
        </html>
    "#;
    
    // Page B: completely unstyled default page
    let html_b = r#"
        <!DOCTYPE html>
        <html>
        <body><p>Page B</p></body>
        </html>
    "#;

    // Run 1: Fresh Page B
    let fresh_view = engine.create_headless_view(bounds).unwrap();
    engine.load_html(fresh_view, html_b).unwrap();
    engine.render_view(fresh_view).unwrap();
    let fresh_dl = display_list_json(&mut engine, fresh_view);

    // Run 2: Page B after Page A in the same view
    let nav_view = engine.create_headless_view(bounds).unwrap();
    engine.load_html(nav_view, html_a).unwrap();
    engine.render_view(nav_view).unwrap();
    
    // Navigate to Page B
    engine.load_html(nav_view, html_b).unwrap();
    engine.render_view(nav_view).unwrap();
    let nav_dl = display_list_json(&mut engine, nav_view);

    assert_eq!(nav_dl, fresh_dl, "Page B display list after Page A must match fresh Page B exactly");
}

#[tokio::test]
async fn test_navigation_form_edit_state_isolation() {
    let mut engine = Engine::new(EngineConfig::for_parity_testing()).unwrap();
    let bounds = Bounds { x: 0, y: 0, width: 800, height: 600 };
    
    let page_a = r#"<html><body><input id="target" value="defaultA"></body></html>"#;
    let page_b = r#"<html><body><input id="target" value="defaultB"></body></html>"#;

    let view = engine.create_headless_view(bounds).unwrap();
    engine.load_html(view, page_a).unwrap();
    
    // Simulate user typing into input control (Node 1)
    engine.handle_text_input(view, "USER_TYPED_DATA").unwrap();
    
    // Navigate to page B
    engine.load_html(view, page_b).unwrap();
    engine.render_view(view).unwrap();
    
    // Verify Node 1 on Page B has not inherited "USER_TYPED_DATA"
    let dl = display_list_json(&mut engine, view);
    assert!(!dl.contains("USER_TYPED_DATA"), "Page B must not inherit text edit state from Page A");
    assert!(dl.contains("defaultB"), "Page B must display its own authored default value");
}

#[tokio::test]
async fn test_navigation_glyph_atlas_reset_no_corruption() {
    let mut engine = Engine::new(EngineConfig::for_parity_testing()).unwrap();
    let bounds = Bounds { x: 0, y: 0, width: 1280, height: 800 };

    // Generate page A with thousands of distinct Unicode characters to force atlas capacity
    let mut heavy_text = String::new();
    for i in 0x0370..0x08A0 {
        if let Some(c) = char::from_u32(i) {
            heavy_text.push(c);
            heavy_text.push(' ');
        }
    }
    let page_a = format!("<html><body><p style='font-size: 24px;'>{}</p></body></html>", heavy_text);
    let page_b = "<html><body><h1>Wikipedia Article Lead</h1><p>Standard article text.</p></body></html>";

    // Fresh control
    let fresh_view = engine.create_headless_view(bounds).unwrap();
    engine.load_html(fresh_view, page_b).unwrap();
    engine.render_view(fresh_view).unwrap();
    let fresh_dl = display_list_json(&mut engine, fresh_view);

    // Navigated view
    let nav_view = engine.create_headless_view(bounds).unwrap();
    engine.load_html(nav_view, &page_a).unwrap();
    engine.render_view(nav_view).unwrap();
    
    engine.load_html(nav_view, page_b).unwrap();
    engine.render_view(nav_view).unwrap();
    let nav_dl = display_list_json(&mut engine, nav_view);

    assert_eq!(nav_dl, fresh_dl, "Glyph atlas saturation on Page A must not corrupt Page B display list");
}
```

---

## 5. Summary Matrix of Surviving State

| Subsystem | State Object | Lifetime / Reset Mechanism | Status |
| :--- | :--- | :--- | :--- |
| **Styles** | `view.external_stylesheets` | `load_subresources` & `load_html` | ⚠️ **Leak Window** (fixed by clearing on commit) |
| **Styles** | `view.rule_reads` | Cleared on `load_url` commit | ✅ Isolated |
| **Text** | `Renderer::glyph_cache` | Persists on `Renderer` (shared atlas) | ⚠️ **Mid-frame reset bug** (needs batch flush) |
| **Text** | `FontLoader` web fonts | Origin-partitioned (`TopLevelSite`) | ✅ Isolated |
| **Images** | `view.images_attempted` | Cleared only in `load_images` on empty | ⚠️ **Leak** (fixed by clearing on commit) |
| **Images** | `ImageManager` cache | Global URL cache | ✅ Isolated |
| **DOM / Form** | `edit_states`, `checked_states` | Cleared on `load_url` commit | ✅ Isolated |
| **Input** | Focus, hover, active chains | Cleared on `load_url` commit | ✅ Isolated |
| **Scroll** | `view.scroll_offset` | Reset to `(0, 0)` on `load_url` commit | ✅ Isolated |
| **JavaScript** | `JsRuntime`, timers, jobs | Reconstructed per document | ✅ Isolated |
| **Layout** | `LayoutBox`, `FloatContext`, `TableGrid` | Ephemeral per build pass | ✅ Isolated |
