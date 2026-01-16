# 🎯 Integration Test Implementation Plan

**Status:** Draft
**Created:** 2026-01-05
**Target Completion:** 6-week phased rollout

---

## 📋 Executive Summary

This plan transforms the current basic integration tests into a comprehensive test suite that validates real system behavior across all HiWave components. The plan addresses the critical gap identified in code review: current tests only verify struct creation, not actual functionality.

### Current State
- ❌ 8 basic tests that only verify struct construction
- ❌ No GPU initialization in tests
- ❌ No end-to-end rendering validation
- ❌ No IPC message testing
- ❌ No multi-view interaction testing
- ✅ Strong WPT-style unit test infrastructure exists (`rustkit-test`)
- ✅ Excellent visual regression test scripts
- ✅ Performance benchmarking framework

### Target State
- ✅ 100+ integration tests across 8 categories
- ✅ GPU-backed rendering validation
- ✅ End-to-end navigation and interaction tests
- ✅ IPC message integration coverage
- ✅ Multi-view coordination tests
- ✅ HTTP request/response integration
- ✅ JavaScript-to-DOM bridge validation

---

## 🏗️ Architecture Design

### Test Organization Structure

```
crates/hiwave-app/
├── tests/
│   ├── integration/                    # Main integration test suite
│   │   ├── mod.rs                     # Test harness and utilities
│   │   ├── engine/                    # Engine lifecycle tests
│   │   │   ├── initialization.rs
│   │   │   ├── view_management.rs
│   │   │   └── resource_cleanup.rs
│   │   ├── rendering/                 # Full rendering pipeline tests
│   │   │   ├── html_to_pixels.rs
│   │   │   ├── css_application.rs
│   │   │   ├── layout_computation.rs
│   │   │   └── multi_view.rs
│   │   ├── navigation/                # Navigation and history tests
│   │   │   ├── url_loading.rs
│   │   │   ├── history_stack.rs
│   │   │   └── back_forward.rs
│   │   ├── ipc/                       # IPC message integration
│   │   │   ├── tab_lifecycle.rs
│   │   │   ├── workspace_commands.rs
│   │   │   ├── settings_sync.rs
│   │   │   └── focus_mode.rs
│   │   ├── interaction/               # User interaction tests
│   │   │   ├── mouse_events.rs
│   │   │   ├── keyboard_input.rs
│   │   │   ├── scroll_handling.rs
│   │   │   └── form_submission.rs
│   │   ├── networking/                # HTTP and resource loading
│   │   │   ├── http_requests.rs
│   │   │   ├── resource_cache.rs
│   │   │   ├── subresource_loading.rs
│   │   │   └── shield_blocking.rs
│   │   ├── javascript/                # JS runtime integration
│   │   │   ├── dom_manipulation.rs
│   │   │   ├── event_dispatch.rs
│   │   │   ├── api_bindings.rs
│   │   │   └── console_logging.rs
│   │   └── performance/               # Performance regression tests
│   │       ├── render_timing.rs
│   │       ├── memory_usage.rs
│   │       └── startup_time.rs
│   └── support/                       # Test utilities and helpers
│       ├── test_server.rs             # Local HTTP test server
│       ├── test_engine.rs             # Headless engine wrapper
│       ├── frame_capture.rs           # Screenshot utilities
│       ├── assertions.rs              # Custom test assertions
│       └── fixtures.rs                # Test HTML fixtures
```

---

## 🧪 Test Categories

### Category 1: Engine Lifecycle Tests
**Priority:** P0 (Critical)
**Estimated Tests:** 15
**Dependencies:** None

Tests the RustKit engine initialization, view management, and cleanup.

#### Test Scenarios:

1. **Engine Initialization**
   - ✅ Engine creates with default config
   - ✅ Engine creates with custom user agent
   - ✅ Engine creates with JS enabled/disabled
   - ✅ Multiple engines can coexist
   - ✅ Engine initializes compositor correctly

2. **View Management**
   - ✅ Single view creation and destruction
   - ✅ Multiple views (up to 10) creation
   - ✅ View resizing updates bounds correctly
   - ✅ View destruction releases GPU resources
   - ✅ View ID reuse after destruction

3. **Resource Cleanup**
   - ✅ Engine drop releases all GPU resources
   - ✅ View destruction removes from compositor
   - ✅ No memory leaks after 1000 view creates/destroys
   - ✅ Proper cleanup on engine error states

**Code Example:**
```rust
#[test]
fn test_engine_creates_with_gpu() {
    let engine = EngineBuilder::new()
        .user_agent("Test/1.0")
        .build()
        .expect("Engine should build successfully");

    // Verify compositor was initialized
    assert!(engine.compositor().is_some());
}

#[test]
fn test_view_lifecycle_releases_resources() {
    let mut engine = create_test_engine();
    let window_handle = create_test_window();

    // Create view
    let view_id = engine.create_view(
        window_handle,
        Bounds::new(0, 0, 800, 600)
    ).unwrap();

    // Verify view exists
    assert!(engine.get_view(view_id).is_some());

    // Destroy view
    engine.destroy_view(view_id).unwrap();

    // Verify view no longer exists
    assert!(engine.get_view(view_id).is_none());
}
```

---

### Category 2: Rendering Pipeline Tests
**Priority:** P0 (Critical)
**Estimated Tests:** 25
**Dependencies:** Engine Lifecycle Tests

Tests the complete HTML → Pixels rendering pipeline.

#### Test Scenarios:

1. **HTML to DOM**
   - ✅ Simple HTML parses and renders
   - ✅ Complex nested structures render correctly
   - ✅ Malformed HTML recovers gracefully
   - ✅ Large documents (10,000+ elements) render

2. **CSS Application**
   - ✅ Inline styles apply correctly
   - ✅ External stylesheets load and apply
   - ✅ CSS cascade resolves correctly
   - ✅ Specificity rules apply in correct order
   - ✅ Pseudo-classes compute correctly

3. **Layout Computation**
   - ✅ Block layout calculates correct dimensions
   - ✅ Flexbox layout positions items correctly
   - ✅ Grid layout creates correct grid areas
   - ✅ Text wrapping calculates line breaks
   - ✅ Scrollable content respects overflow

4. **Display List Generation**
   - ✅ Display list contains expected commands
   - ✅ Z-ordering respects stacking contexts
   - ✅ Clip regions apply correctly
   - ✅ Transform matrices compute correctly

5. **GPU Rendering**
   - ✅ Renders produce non-blank frames
   - ✅ Background colors render correctly
   - ✅ Text renders with correct metrics
   - ✅ Images decode and render
   - ✅ Border radius clips correctly

**Code Example:**
```rust
#[test]
fn test_simple_html_renders_to_frame() {
    let mut test_engine = TestEngine::new();

    let html = r#"<!DOCTYPE html>
        <html>
        <head><style>
            body { background: #ff0000; }
            h1 { color: #00ff00; }
        </style></head>
        <body><h1>Test</h1></body>
        </html>"#;

    // Load HTML
    test_engine.load_html(html).unwrap();

    // Render and capture frame
    let frame = test_engine.render_and_capture().unwrap();

    // Verify frame is not blank
    assert!(!frame.is_blank());

    // Verify background color (red)
    let bg_color = frame.sample_pixel(10, 10);
    assert_color_near(bg_color, RGB(255, 0, 0), tolerance: 5);

    // Verify text rendered (green)
    let text_color = frame.sample_pixel(100, 50);
    assert_color_near(text_color, RGB(0, 255, 0), tolerance: 5);
}

#[test]
fn test_flexbox_layout_positions_correctly() {
    let html = r#"<!DOCTYPE html>
        <html><body>
        <div style="display: flex; width: 300px;">
            <div id="item1" style="flex: 1;">Item 1</div>
            <div id="item2" style="flex: 2;">Item 2</div>
        </div>
        </body></html>"#;

    let mut test_engine = TestEngine::new();
    test_engine.load_html(html).unwrap();
    test_engine.layout().unwrap();

    // Get computed layout boxes
    let item1 = test_engine.get_element_bounds("#item1").unwrap();
    let item2 = test_engine.get_element_bounds("#item2").unwrap();

    // Verify flex sizing (1:2 ratio)
    assert_eq!(item1.width, 100); // 1/3 of 300
    assert_eq!(item2.width, 200); // 2/3 of 300

    // Verify positioning
    assert_eq!(item1.x, 0);
    assert_eq!(item2.x, 100);
}
```

---

### Category 3: Navigation Tests
**Priority:** P1 (High)
**Estimated Tests:** 18
**Dependencies:** Rendering Pipeline Tests

Tests URL loading, history management, and navigation flow.

#### Test Scenarios:

1. **URL Loading**
   - ✅ Load local file:// URLs
   - ✅ Load about: pages
   - ✅ Load data: URLs
   - ✅ Handle invalid URLs gracefully
   - ✅ Redirect handling (when HTTP works)

2. **History Stack**
   - ✅ Navigation creates history entry
   - ✅ Back/forward navigate through history
   - ✅ Replace state doesn't create entry
   - ✅ Max history limit (100 entries)
   - ✅ History persists across sessions

3. **Navigation Events**
   - ✅ NavigationStarted fires before load
   - ✅ PageLoaded fires after render
   - ✅ NavigationError fires on failure
   - ✅ Title updates propagate to app

**Code Example:**
```rust
#[test]
fn test_navigation_creates_history_entry() {
    let mut test_engine = TestEngine::new();
    let mut history = Vec::new();

    // Navigate to first page
    test_engine.load_url("file:///test/page1.html").unwrap();
    history.push("page1.html");

    // Navigate to second page
    test_engine.load_url("file:///test/page2.html").unwrap();
    history.push("page2.html");

    // Verify history length
    assert_eq!(test_engine.history_length(), 2);

    // Go back
    test_engine.go_back().unwrap();

    // Verify current page
    assert_eq!(test_engine.current_url(), "file:///test/page1.html");

    // Go forward
    test_engine.go_forward().unwrap();

    // Verify current page
    assert_eq!(test_engine.current_url(), "file:///test/page2.html");
}
```

---

### Category 4: IPC Integration Tests
**Priority:** P1 (High)
**Estimated Tests:** 30
**Dependencies:** Rendering Pipeline Tests

Tests all 314 IPC messages and their effects on application state.

#### Test Scenarios:

1. **Tab Lifecycle Messages**
   - ✅ CreateTab creates new tab in workspace
   - ✅ CloseTab removes tab and updates active
   - ✅ ActivateTab switches active tab
   - ✅ PinTab prevents auto-close
   - ✅ DuplicateTab creates identical tab

2. **Workspace Commands**
   - ✅ CreateWorkspace adds to shell
   - ✅ DeleteWorkspace removes and migrates tabs
   - ✅ SwitchWorkspace changes active workspace
   - ✅ RenameWorkspace updates metadata

3. **Settings Sync**
   - ✅ UpdateSettings persists to settings.json
   - ✅ ToggleDarkMode updates theme
   - ✅ SetHomePage validates URL
   - ✅ ChangeSearchEngine updates default

4. **Focus Mode**
   - ✅ EnterFocusMode hides chrome UI
   - ✅ ExitFocusMode restores chrome UI
   - ✅ Auto-trigger on scroll timeout
   - ✅ Blocklist prevents auto-trigger

**Code Example:**
```rust
#[test]
fn test_create_tab_ipc_message() {
    let mut app_state = create_test_app_state();
    let ipc_handler = IpcHandler::new(&mut app_state);

    // Send CreateTab message
    let msg = IpcMessage::CreateTab {
        workspace_id: "default".to_string(),
        url: Some("https://example.com".to_string()),
        activate: true,
    };

    let response = ipc_handler.handle(msg).unwrap();

    // Verify tab was created
    assert!(response.success);
    let tab_id = response.data["tab_id"].as_str().unwrap();

    // Verify tab exists in workspace
    let workspace = app_state.shell.get_workspace("default").unwrap();
    assert!(workspace.has_tab(tab_id));

    // Verify tab is active
    assert_eq!(workspace.active_tab_id(), Some(tab_id));
}

#[test]
fn test_ipc_message_validation() {
    let mut app_state = create_test_app_state();
    let ipc_handler = IpcHandler::new(&mut app_state);

    // Send invalid CreateWorkspace (empty name)
    let msg = IpcMessage::CreateWorkspace {
        name: "".to_string(),
    };

    let response = ipc_handler.handle(msg).unwrap();

    // Verify validation error
    assert!(!response.success);
    assert!(response.error.unwrap().contains("name cannot be empty"));
}
```

---

### Category 5: User Interaction Tests
**Priority:** P2 (Medium)
**Estimated Tests:** 20
**Dependencies:** Rendering Pipeline Tests

Tests mouse, keyboard, and scroll event handling.

#### Test Scenarios:

1. **Mouse Events**
   - ✅ Click dispatches to correct element
   - ✅ Hover updates :hover pseudo-class
   - ✅ Drag scrolls scrollable content
   - ✅ Double-click selects text

2. **Keyboard Input**
   - ✅ Typing inputs text in focused field
   - ✅ Tab navigates between focusable elements
   - ✅ Enter submits forms
   - ✅ Shortcuts trigger actions

3. **Scroll Handling**
   - ✅ Wheel scroll updates scroll position
   - ✅ Scroll dispatches scroll events
   - ✅ Smooth scroll animates correctly
   - ✅ Scroll snapping works

4. **Form Submission**
   - ✅ Form submit collects field values
   - ✅ Validation prevents invalid submit
   - ✅ File inputs trigger file picker
   - ✅ Checkbox/radio state toggles

**Code Example:**
```rust
#[test]
fn test_click_dispatches_to_element() {
    let html = r#"<!DOCTYPE html>
        <html><body>
        <button id="btn" style="position: absolute; left: 50px; top: 50px; width: 100px; height: 40px;">
            Click Me
        </button>
        <script>
            document.getElementById('btn').addEventListener('click', () => {
                document.body.style.background = 'red';
            });
        </script>
        </body></html>"#;

    let mut test_engine = TestEngine::new();
    test_engine.load_html(html).unwrap();

    // Simulate click at button center
    test_engine.send_mouse_click(100, 70).unwrap();

    // Verify background changed to red
    let frame = test_engine.render_and_capture().unwrap();
    let bg_color = frame.sample_pixel(10, 10);
    assert_color_near(bg_color, RGB(255, 0, 0), tolerance: 5);
}
```

---

### Category 6: Networking Tests
**Priority:** P2 (Medium)
**Estimated Tests:** 15
**Dependencies:** HTTP implementation (Phase 3)

Tests HTTP requests, resource loading, and caching.

#### Test Scenarios:

1. **HTTP Requests**
   - ✅ GET request loads HTML
   - ✅ POST request sends data
   - ✅ Request headers sent correctly
   - ✅ Response headers parsed
   - ✅ Timeout after configured duration

2. **Resource Loading**
   - ✅ Images load from URLs
   - ✅ Stylesheets load and apply
   - ✅ Scripts load and execute
   - ✅ Fonts load and render

3. **Shield Integration**
   - ✅ Blocked URLs don't load
   - ✅ Filter lists update
   - ✅ Exception rules work
   - ✅ Stats track blocked requests

**Code Example:**
```rust
#[test]
#[cfg(feature = "http")]
fn test_http_get_loads_html() {
    let mut test_server = TestHttpServer::start();
    test_server.serve("/test.html", r#"
        <!DOCTYPE html>
        <html><body><h1>Loaded via HTTP</h1></body></html>
    "#);

    let mut test_engine = TestEngine::new();
    test_engine.load_url(&test_server.url("/test.html")).unwrap();

    // Wait for load
    test_engine.wait_for_navigation().unwrap();

    // Verify content
    let h1_text = test_engine.query_selector("h1").unwrap().text_content();
    assert_eq!(h1_text, "Loaded via HTTP");
}

#[test]
fn test_shield_blocks_tracker() {
    let mut test_engine = TestEngine::new();
    let shield = AdBlocker::new().unwrap();
    shield.add_rule("||tracker.com^");
    test_engine.set_shield(shield);

    let html = r#"<!DOCTYPE html>
        <html><body>
        <img src="https://tracker.com/pixel.gif">
        </body></html>"#;

    test_engine.load_html(html).unwrap();

    // Verify image was blocked
    let stats = test_engine.get_shield_stats();
    assert_eq!(stats.blocked_count, 1);
    assert!(stats.blocked_urls.contains("tracker.com"));
}
```

---

### Category 7: JavaScript Integration Tests
**Priority:** P2 (Medium)
**Estimated Tests:** 25
**Dependencies:** JS runtime implementation

Tests JavaScript-to-DOM bridge and Web APIs.

#### Test Scenarios:

1. **DOM Manipulation**
   - ✅ createElement creates elements
   - ✅ appendChild adds to tree
   - ✅ removeChild removes from tree
   - ✅ setAttribute updates attributes
   - ✅ Mutations trigger relayout

2. **Event Dispatch**
   - ✅ addEventListener registers handler
   - ✅ dispatchEvent triggers handlers
   - ✅ Event bubbling works
   - ✅ preventDefault stops default action

3. **API Bindings**
   - ✅ document.querySelector returns element
   - ✅ window.setTimeout schedules callback
   - ✅ fetch (when HTTP works) makes requests
   - ✅ localStorage persists data

**Code Example:**
```rust
#[test]
#[cfg(feature = "js")]
fn test_js_dom_manipulation() {
    let html = r#"<!DOCTYPE html>
        <html><body>
        <div id="container"></div>
        <script>
            const div = document.createElement('div');
            div.id = 'created';
            div.textContent = 'Created by JS';
            document.getElementById('container').appendChild(div);
        </script>
        </body></html>"#;

    let mut test_engine = TestEngine::new();
    test_engine.load_html(html).unwrap();

    // Verify element was created and added
    let created = test_engine.query_selector("#created").unwrap();
    assert_eq!(created.text_content(), "Created by JS");
    assert_eq!(created.parent_id(), Some("container"));
}
```

---

### Category 8: Performance Regression Tests
**Priority:** P3 (Low)
**Estimated Tests:** 12
**Dependencies:** All other categories

Tests that performance doesn't regress.

#### Test Scenarios:

1. **Render Timing**
   - ✅ Simple page renders < 50ms
   - ✅ Complex page renders < 200ms
   - ✅ Layout reflow < 16ms (60fps)
   - ✅ Paint commands < 10ms

2. **Memory Usage**
   - ✅ Engine < 100MB baseline
   - ✅ 1000 elements < 50MB
   - ✅ No leaks after 100 navigations

3. **Startup Time**
   - ✅ Engine init < 100ms
   - ✅ First view < 50ms
   - ✅ First render < 100ms

---

## 🛠️ Test Infrastructure

### TestEngine Helper

A headless test wrapper around RustKit engine:

```rust
pub struct TestEngine {
    engine: Engine,
    view_id: EngineViewId,
    event_queue: Vec<EngineEvent>,
    window_handle: TestWindow,
}

impl TestEngine {
    pub fn new() -> Self {
        Self::with_size(800, 600)
    }

    pub fn with_size(width: u32, height: u32) -> Self {
        let mut engine = EngineBuilder::new().build().unwrap();
        let window_handle = TestWindow::create();
        let bounds = Bounds::new(0, 0, width, height);

        let view_id = engine.create_view(
            window_handle.raw_handle(),
            bounds
        ).unwrap();

        Self {
            engine,
            view_id,
            event_queue: Vec::new(),
            window_handle,
        }
    }

    pub fn load_html(&mut self, html: &str) -> Result<()> {
        self.engine.load_html(self.view_id, html)
    }

    pub fn load_url(&mut self, url: &str) -> Result<()> {
        self.engine.load_url(self.view_id, url)
    }

    pub fn render_and_capture(&mut self) -> Result<TestFrame> {
        self.engine.render_view(self.view_id)?;

        let temp_path = format!("/tmp/test_frame_{}.ppm", self.view_id);
        self.engine.capture_frame(self.view_id, &temp_path)?;

        TestFrame::load(&temp_path)
    }

    pub fn query_selector(&self, selector: &str) -> Option<TestElement> {
        // Query DOM for element matching selector
        todo!()
    }

    pub fn get_element_bounds(&self, selector: &str) -> Option<Bounds> {
        // Get layout bounds for element
        todo!()
    }

    pub fn send_mouse_click(&mut self, x: i32, y: i32) -> Result<()> {
        let event = InputEvent::MouseDown {
            x, y, button: MouseButton::Left
        };
        self.engine.send_event(self.view_id, event)?;

        let event = InputEvent::MouseUp {
            x, y, button: MouseButton::Left
        };
        self.engine.send_event(self.view_id, event)
    }

    pub fn wait_for_navigation(&mut self) -> Result<()> {
        // Poll events until PageLoaded
        todo!()
    }
}
```

### TestFrame Helper

Utilities for frame capture and pixel verification:

```rust
pub struct TestFrame {
    width: u32,
    height: u32,
    pixels: Vec<RGB>,
}

impl TestFrame {
    pub fn load(path: &str) -> Result<Self> {
        // Load PPM file
        todo!()
    }

    pub fn sample_pixel(&self, x: u32, y: u32) -> RGB {
        let idx = (y * self.width + x) as usize;
        self.pixels[idx]
    }

    pub fn is_blank(&self) -> bool {
        // Check if all pixels are same color
        let first = self.pixels[0];
        self.pixels.iter().all(|p| *p == first)
    }

    pub fn compare(&self, other: &TestFrame) -> FrameDiff {
        // Pixel-by-pixel comparison
        todo!()
    }
}

pub fn assert_color_near(actual: RGB, expected: RGB, tolerance: u8) {
    let dr = (actual.r as i32 - expected.r as i32).abs();
    let dg = (actual.g as i32 - expected.g as i32).abs();
    let db = (actual.b as i32 - expected.b as i32).abs();

    assert!(
        dr <= tolerance as i32 && dg <= tolerance as i32 && db <= tolerance as i32,
        "Color mismatch: expected {:?}, got {:?} (tolerance: {})",
        expected, actual, tolerance
    );
}
```

### TestHttpServer

Mock HTTP server for network tests:

```rust
pub struct TestHttpServer {
    server: tiny_http::Server,
    routes: HashMap<String, String>,
}

impl TestHttpServer {
    pub fn start() -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        Self {
            server,
            routes: HashMap::new(),
        }
    }

    pub fn serve(&mut self, path: &str, content: &str) {
        self.routes.insert(path.to_string(), content.to_string());
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.server.server_addr().port(), path)
    }
}
```

---

## 📅 Implementation Roadmap

### Phase 1: Foundation (Week 1-2)
**Goal:** Set up test infrastructure and basic engine tests

**Tasks:**
1. Rename current `rustkit_integration.rs` → `rustkit_unit_tests.rs`
2. Create `tests/integration/` directory structure
3. Implement `TestEngine` helper
4. Implement `TestFrame` helper
5. Add required dev-dependencies to `Cargo.toml`
6. Write first 5 engine lifecycle tests
7. Verify tests run in CI

**Deliverables:**
- ✅ Test infrastructure in place
- ✅ 5 passing integration tests
- ✅ CI runs integration tests

**Estimated Effort:** 10 hours

---

### Phase 2: Rendering Pipeline (Week 3-4)
**Goal:** Comprehensive rendering validation

**Tasks:**
1. Implement DOM query utilities in `TestEngine`
2. Implement layout bounds extraction
3. Write 15 rendering pipeline tests (HTML → Pixels)
4. Write 10 CSS application tests
5. Create test fixtures for common layouts
6. Add pixel comparison assertions

**Deliverables:**
- ✅ 25 rendering tests passing
- ✅ Frame capture working reliably
- ✅ Pixel comparison utilities

**Estimated Effort:** 15 hours

---

### Phase 3: Navigation & IPC (Week 5)
**Goal:** Navigation flow and message handling

**Tasks:**
1. Implement navigation event tracking in `TestEngine`
2. Write 18 navigation tests
3. Implement `IpcTestHarness` for message testing
4. Write 30 IPC integration tests (prioritize top 30 messages)
5. Add state verification utilities

**Deliverables:**
- ✅ 18 navigation tests
- ✅ 30 IPC tests (covering ~10% of messages)
- ✅ State snapshot utilities

**Estimated Effort:** 12 hours

---

### Phase 4: Interactions (Week 6)
**Goal:** User interaction validation

**Tasks:**
1. Implement event simulation in `TestEngine`
2. Write 20 interaction tests (mouse, keyboard, scroll)
3. Add form submission tests
4. Create interactive test fixtures

**Deliverables:**
- ✅ 20 interaction tests
- ✅ Event simulation working

**Estimated Effort:** 8 hours

---

### Phase 5: Advanced Features (Future)
**Goal:** Network and JS integration

**Tasks:**
1. Implement `TestHttpServer` (when HTTP ready)
2. Write 15 networking tests
3. Implement JS execution utilities (when JS ready)
4. Write 25 JavaScript integration tests
5. Add 12 performance regression tests

**Deliverables:**
- ✅ 15 networking tests
- ✅ 25 JS tests
- ✅ 12 performance tests

**Estimated Effort:** 20 hours

---

## 🔧 Required Dependencies

Add to `crates/hiwave-app/Cargo.toml`:

```toml
[dev-dependencies]
http = "1.0"
tempfile = "3.13"         # Temporary test files
tiny-http = "0.12"        # Test HTTP server (Phase 5)
criterion = "0.5"         # Performance benchmarking
proptest = "1.5"          # Property-based testing
```

---

## ✅ Success Criteria

### Quantitative Metrics
- [ ] 100+ integration tests across 8 categories
- [ ] 95%+ test pass rate in CI
- [ ] < 5 minute total test suite execution
- [ ] Zero GPU resource leaks
- [ ] < 10ms average test overhead

### Qualitative Criteria
- [ ] Tests catch real regressions (validate with intentional breaks)
- [ ] Tests are maintainable and well-documented
- [ ] New contributors can add tests easily
- [ ] Test failures provide actionable debugging info

---

## 🚧 Known Challenges & Mitigations

### Challenge 1: GPU Initialization in CI
**Problem:** CI environments may not have GPU access
**Mitigation:**
- Use software rendering fallback (wgpu backends)
- Skip GPU tests with `#[cfg(feature = "gpu-tests")]`
- Run full GPU tests only on macOS runners

### Challenge 2: Test Flakiness
**Problem:** Timing-dependent tests may flake
**Mitigation:**
- Use deterministic event pumping
- Avoid real time delays (use virtual time)
- Retry flaky tests 3x before failing

### Challenge 3: HTTP Tests Before Implementation
**Problem:** Networking tests blocked on Phase 3
**Mitigation:**
- Mark as `#[ignore]` until HTTP ready
- Use mock responses for now
- Implement with `TestHttpServer` when ready

### Challenge 4: JavaScript Tests Before Runtime
**Problem:** JS tests blocked on runtime
**Mitigation:**
- Mark as `#[ignore]` until JS ready
- Focus on DOM/CSSOM tests first
- Implement incrementally as JS APIs added

---

## 📊 Tracking Progress

Create GitHub issues:
- [ ] #XXX: Setup integration test infrastructure (Phase 1)
- [ ] #XXX: Implement rendering pipeline tests (Phase 2)
- [ ] #XXX: Implement navigation & IPC tests (Phase 3)
- [ ] #XXX: Implement interaction tests (Phase 4)
- [ ] #XXX: Implement networking tests (Phase 5)
- [ ] #XXX: Implement JS integration tests (Phase 5)
- [ ] #XXX: Implement performance regression tests (Phase 5)

Weekly progress reviews:
- Monday: Plan week's test development
- Friday: Review pass rate and coverage
- Sunday: Update roadmap with blockers

---

## 🎓 Learning Resources

For contributors adding tests:

1. **RustKit Architecture:** See `docs/ARCHITECTURE.md`
2. **Test Writing Guide:** Create `docs/TESTING.md`
3. **Example Tests:** Reference existing parser corpus tests
4. **Debugging Failures:** Use `--nocapture` for println debugging

---

## 📝 Next Steps

**Immediate Actions (This Week):**
1. Review and approve this plan
2. Create GitHub tracking issues
3. Set up integration test directory structure
4. Implement `TestEngine` helper
5. Write first 5 engine lifecycle tests

**Owner:** TBD
**Reviewers:** TBD
**Estimated Start:** Week of 2026-01-06

---

*This document is a living plan and will be updated as implementation progresses.*
