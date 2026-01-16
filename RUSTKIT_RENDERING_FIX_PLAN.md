# RustKit Rendering Fix Plan

## Problem Statement

The HiWave browser opens but shows a blank white content area. The RustKit engine infrastructure is in place, but rendering is not working on macOS.

## Current State Analysis

### What's Working ✅
1. **Engine initialization** - `Engine::new()` succeeds
2. **ViewHost creation** - NSView is created and added to window
3. **Compositor initialization** - wgpu device/queue created
4. **View creation** - `create_view()` returns successfully
5. **Event loop integration** - `render_all_views()` is being called

### What's NOT Working ❌
1. **Surface creation** - `create_surface_for_raw_handle()` may be failing silently
2. **HTML loading** - `load_html_internal()` may not be parsing/rendering
3. **Display list generation** - Layout may not be producing visible content
4. **GPU rendering** - Surface may not be properly connected to the NSView

## Root Cause Investigation

### Issue 1: Surface Creation
The compositor's `create_surface_for_raw_handle()` on macOS needs to create a wgpu surface from the NSView's CAMetalLayer.

**Current code path:**
```
RustKitView::new() 
  → Engine::create_view() 
    → ViewHost::create_view() [creates NSView with CAMetalLayer]
    → Compositor::create_surface_for_raw_handle() [creates wgpu surface]
```

**Potential issues:**
- The raw window handle passed may not be correct
- The CAMetalLayer may not be properly configured
- The wgpu surface may not be connected to the layer

### Issue 2: HTML Loading Pipeline
When `load_html_internal()` is called, it should:
1. Parse HTML → DOM tree
2. Parse CSS → Style tree  
3. Layout → LayoutBox tree
4. Build display list
5. Render display list to surface

**Current state:** The HTML may be loaded but layout/rendering may be stubbed.

### Issue 3: Initial Rendering
The engine calls `render_solid_color()` for initial background, but this may fail if the surface isn't properly created.

## Diagnostic Steps

### Step 1: Add Logging
Add tracing to identify where the pipeline breaks:
- [ ] Log surface creation success/failure in compositor
- [ ] Log HTML parsing success/failure
- [ ] Log layout generation 
- [ ] Log display list command count
- [ ] Log render pass execution

### Step 2: Verify Surface Creation
Check if wgpu surface is properly created from NSView:
- [ ] Verify CAMetalLayer is attached to NSView
- [ ] Verify raw window handle extraction is correct
- [ ] Verify wgpu can create surface from handle

### Step 3: Test Solid Color Rendering
Simplest test - can we render a solid color?
- [ ] Call `render_solid_color()` explicitly
- [ ] Verify the color appears on screen

### Step 4: Test HTML Pipeline
- [ ] Verify HTML parsing produces DOM
- [ ] Verify CSS produces styles
- [ ] Verify layout produces boxes
- [ ] Verify display list has commands

## Implementation Plan

### Phase 1: Diagnostics (1-2 hours)
Add comprehensive logging to identify the exact failure point.

```rust
// In rustkit-engine/src/lib.rs create_view()
info!("Creating view - step 1: viewhost");
let viewhost_id = ...;
info!(?viewhost_id, "Created viewhost view");

info!("Creating view - step 2: raw handle");
let raw_handle = ...;
info!("Got raw handle: {:?}", raw_handle);

info!("Creating view - step 3: surface");
let result = self.compositor.create_surface_for_raw_handle(...);
info!(?result, "Surface creation result");

info!("Creating view - step 4: initial render");
let result = self.compositor.render_solid_color(...);
info!(?result, "Initial render result");
```

### Phase 2: Fix Surface Creation (2-4 hours)
Ensure wgpu surface is properly created from NSView.

**Key files:**
- `crates/rustkit-compositor/src/lib.rs` - `create_surface_for_raw_handle()`
- `crates/rustkit-viewhost/src/macos.rs` - `get_raw_window_handle()`

**Tasks:**
1. Verify CAMetalLayer is properly set on NSView
2. Verify raw-window-handle extraction returns correct AppKitWindowHandle
3. Verify wgpu can create surface from the handle
4. Test with a simple solid color render

### Phase 3: Fix HTML Rendering Pipeline (4-8 hours)
Ensure HTML → Display List pipeline works.

**Key files:**
- `crates/rustkit-engine/src/lib.rs` - `load_html()`, `relayout()`
- `crates/rustkit-html/src/lib.rs` - HTML parsing
- `crates/rustkit-layout/src/lib.rs` - Layout
- `crates/rustkit-renderer/src/lib.rs` - Display list execution

**Tasks:**
1. Verify HTML parsing produces valid DOM
2. Verify layout produces valid LayoutBox tree
3. Verify display list has render commands
4. Verify renderer executes commands to surface

### Phase 4: Fix Text Rendering (4-8 hours)
Text rendering requires Core Text integration.

**Key files:**
- `crates/rustkit-text/src/macos.rs` - Text shaping
- `crates/rustkit-renderer/src/lib.rs` - Glyph rendering

**Tasks:**
1. Implement proper Core Text font loading
2. Implement text shaping with CTLine
3. Implement glyph rendering to GPU texture
4. Integrate with display list rendering

## Quick Win: Solid Color Test

The fastest way to verify the rendering pipeline is to render a solid color:

```rust
// In webview_rustkit.rs - after creating the view
// Force a red background to verify rendering works
if let Some(view_id) = self.view_id {
    let mut engine = self.engine.borrow_mut();
    // This should make the content area red
    engine.set_background_color([1.0, 0.0, 0.0, 1.0]);
    engine.render_view(view_id);
}
```

If this works → Surface is fine, issue is in HTML pipeline
If this fails → Surface creation is broken

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         TAO Window                               │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                    NSWindow                              │    │
│  │  ┌─────────────────────────────────────────────────┐    │    │
│  │  │              NSView (contentView)                │    │    │
│  │  │                                                  │    │    │
│  │  │  ┌──────────────────────────────────────────┐   │    │    │
│  │  │  │     RustKit NSView (with CAMetalLayer)   │   │    │    │
│  │  │  │                                          │   │    │    │
│  │  │  │   wgpu Surface ──────────────────────┐   │   │    │    │
│  │  │  │                                      │   │   │    │    │
│  │  │  │   ┌──────────────────────────────┐  │   │   │    │    │
│  │  │  │   │     Rendered Content         │  │   │   │    │    │
│  │  │  │   │  (HTML → Layout → GPU)       │  │   │   │    │    │
│  │  │  │   └──────────────────────────────┘  │   │   │    │    │
│  │  │  │                                      │   │   │    │    │
│  │  │  └──────────────────────────────────────┘   │   │    │    │
│  │  │                                              │   │    │    │
│  │  └──────────────────────────────────────────────┘   │    │    │
│  │                                                      │    │    │
│  └──────────────────────────────────────────────────────┘    │    │
│                                                               │    │
└───────────────────────────────────────────────────────────────┘    │
```

## Files to Modify

### Priority 1 - Surface/Rendering
1. `crates/rustkit-viewhost/src/macos.rs` - NSView + CAMetalLayer setup
2. `crates/rustkit-compositor/src/lib.rs` - Surface creation from handle
3. `crates/rustkit-engine/src/lib.rs` - View creation and rendering

### Priority 2 - HTML Pipeline  
4. `crates/rustkit-engine/src/lib.rs` - load_html, relayout
5. `crates/rustkit-renderer/src/lib.rs` - Display list execution

### Priority 3 - Text
6. `crates/rustkit-text/src/macos.rs` - Core Text integration

## Success Criteria

1. ✅ Solid color renders to screen
2. ✅ Simple HTML (`<h1>Hello</h1>`) renders
3. ✅ ABOUT_HTML page renders with text
4. ✅ Links are clickable
5. ✅ Navigation works

## Estimated Time

- **Phase 1 (Diagnostics):** 1-2 hours
- **Phase 2 (Surface):** 2-4 hours  
- **Phase 3 (HTML Pipeline):** 4-8 hours
- **Phase 4 (Text):** 4-8 hours

**Total: 11-22 hours** (2-3 days of focused work)

## Next Steps

1. Run the app with `RUST_LOG=debug` to see current logging
2. Add diagnostic logging to identify failure point
3. Fix issues in order of the pipeline (surface → HTML → text)

