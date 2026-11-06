# CH3.A3 composition: the RustKit chrome over the page

The RustKit chrome only; it is the macOS default since CH3.A5a.
`HIWAVE_RUSTKIT_CHROME=0` (the WebKit chrome) is not touched: no call below
runs for `ChromeView::WebKit`.

## View hierarchy

Before (#907): the window's content view holds, in `addSubview:` order
(back to front): chrome (RustKitContentView, the whole window), page
(RustKitContentView, below the toolbar and right of the sidebar), shelf and
Inspector (WKWebViews). The page sits ON TOP of the chrome, so nothing the
chrome draws can reach the page's rect. A panel (downloads, history) asks
for `expand_chrome` and the app pushes the page down 460 px so the panel
has room in the strip.

After: same views, one move. Right after the page view is built, the
chrome's NSView is re-added `positioned:NSWindowAbove relativeTo:<page>`,
so the order is page, chrome, shelf, Inspector; since A3b the chrome is
also kept above the panels (see Panels), so the order is page, panels,
chrome. The chrome stays the whole window. `expand_chrome` no longer moves the page (the panel is drawn over
it); `expand_chrome_small` still does, because the tab-actions bar is in the
chrome's flow and pushes the toolbar down, as before.

## The region: the chrome's painted boxes (A3, A3b)

With the region-only route (A3, A3b; `HIWAVE_RUSTKIT_CHROME_ALPHA=0`
since A3c) the chrome is opaque inside its *region* and absent outside it;
with the alpha route (A3c, below) the region is its hit region only. The
region is a list of rects in view coordinates (top-left origin):

    region = window − hole + overlays

* `hole` is the page column the app already lays out: x from the left
  sidebar's width to the right sidebar, y from the chrome height to the
  bottom (page, Inspector and shelf). `apply_layout` computes it with the
  page's rect and hands it to the chrome every time it places the views.
* `overlays` (CH3.A3b) are the boxes the chrome's **own frame** paints over
  the hole, read from that frame's display list
  (`rustkit_engine::painted_boxes`): every command that puts colour on the
  surface (fills, borders, text, images, gradients, form controls, SVG
  shapes), where its transforms put it, cut by its clips; not inside a
  group faded to opacity 0, not a transparent fill, not a box shadow; and
  not a *backdrop*, a box covering 90% or more of the hole (the root/body
  background chrome.html paints over the whole window, the main column).
  Clipped to the hole; a box inside another (a menu's rows and text) is
  dropped, so an open menu is one rect. No popup list, no script: any
  popup, menu or tooltip the chrome paints over the page is in the region,
  whatever its class.

  Until A3b the overlays were reported by a script in the chrome document
  (`getBoundingClientRect()` of a selector list, posted on click/key and
  once a second). It is gone; only the outside-click helper is left in the
  chrome's tail script.

### The region and the frame change on the same turn

`RustKitView::present_latest` presents the chrome's newest `FrameCommit`
and, in the same call, derives that commit's overlays and applies the
region (`FrameRegion::present` in `rustkit-viewhost`, keyed by the
commit's generation, so an unchanged frame costs nothing). A layout
(`compose_region`) re-derives the frame on screen against the new hole at
once. So the mask and `hitTest:` never lag or lead the pixels by a message
round trip, as the reporter's did (up to a second, and 250 ms after a
click). What is left: the Metal drawable is presented by wgpu outside the
Core Animation transaction that carries the mask (wgpu-hal's
`present_with_transaction` is not reachable through wgpu's public API), so
on screen the two can still land one display refresh apart; the hit region
is exact from the turn the frame is presented.

Cost: one linear pass over the chrome's display list per new chrome frame
(bounds arithmetic only: no pixels, no GPU read-back); a 20,000-command
list is a unit test. At rest the chrome paints nothing over the hole, so
the region is the window less the hole and `chrome has overlays` is false.

The region is applied twice, from the same rects:

1. **Paint**: a `CAShapeLayer` mask on the view's backing layer (wgpu's
   `CAMetalLayer` is its sublayer, so it is clipped too). Pixels outside
   the region are not composited, and the page view below shows.
2. **Hit test**: `RustKitContentView` gets `hitTest:`. When its view has a
   region and the point is outside every rect, it returns `nil`, and AppKit
   continues to the next view down: the page. Views without a region (the
   page, every flag-off view) keep NSView's `hitTest:` unchanged.

Limits of the region alone (the surface opaque, the mask binary): a
popup's `box-shadow` outside its border box is not shown over the page; a
rounded popup's corners show the body's colour; a full-window translucent
overlay (the find modal's dimming layer) is a backdrop and does not dim the
page; and the Metal drawable and the mask can land a refresh apart. CH3.A3c
removes these with the alpha route below; `HIWAVE_RUSTKIT_CHROME_ALPHA=0`
keeps this region-only route (for comparison).

## The alpha route (CH3.A3c): pixels decide what covers the page

On by default with the RustKit chrome. The chrome's surface is
transparent, and the region above becomes its **hit region only**.

1. **Surface.** `RustKitView::set_transparent(true)` (main.rs calls it when
   it stacks the chrome over the page) does three things:
   * `WindowSurface::set_transparent` -> `Presenter::set_transparent`: the
     view's frames clear to (0, 0, 0, 0) instead of the canvas colour
     (`commit.canvas`, white for a transparent document), and the renderer's
     other first-pass clears (gradient, opacity-group, backdrop-filter
     paths, which cleared to white whatever the canvas) clear to
     transparent too (`Renderer::set_transparent` / `first_clear`).
   * `Compositor::set_surface_transparent`: the wgpu surface is reconfigured
     with a non-opaque composite alpha mode. Metal offers `Opaque` and
     `PostMultiplied` only; `PostMultiplied` there does exactly one thing,
     `CAMetalLayer.opaque = NO`. Core Animation always composites a
     layer's contents as premultiplied (`src + dst * (1 - src.a)`), so what
     matters is that the frame *is* premultiplied:
   * the renderer already leaves premultiplied colour in its target: the
     colour, glyph, image and gradient pipelines draw straight colour
     through `ALPHA_BLENDING`, whose colour factor is `SrcAlpha` and whose
     alpha factor is `One, OneMinusSrcAlpha`; starting from (0, 0, 0, 0)
     that gives `(c * a, a)`, and over a premultiplied pixel it is
     premultiplied source-over. Emoji, filters and opacity groups composite
     premultiplied (`PREMULTIPLIED_ALPHA_BLENDING`). No pipeline changed.
     The unit tests check every pixel of transparent frames (text, shadows,
     gradients, a faded group) for `r, g, b <= a`; a straight-alpha pixel
     would fail that and show as a fringe.
   * `set_raw_view_transparent` (viewhost): the view's backing layer is not
     opaque, and `set_raw_view_region` no longer installs a `CAShapeLayer`
     mask for it: the region is kept for `hitTest:` only.
2. **Document.** The app loads chrome.html into the RustKit chrome with
   `data-rustkit-alpha` on `<html>`; chrome.html's
   `html[data-rustkit-alpha], html[data-rustkit-alpha] body { background:
   transparent }` leaves the body unpainted. The tab strip, toolbar,
   sidebars, panels and popups keep their own backgrounds; `.chrome-main`
   paints nothing, so the page column paints nothing. The WebKit chrome
   (flag off) never gets the attribute.
3. **What now reaches the page**: a popup's shadow outside its box, the
   page in a rounded popup's corners, and the find modal's overlay
   (`rgba(0, 0, 0, 0.3)`, unchanged CSS) dims the page. The pixels are
   one Metal drawable, so the mask-vs-drawable refresh gap is gone for
   paint.
4. **Hit testing: the region from the display list (#925), not an alpha
   probe.** The frame lives only on the GPU: there is no CPU copy of the
   chrome's last frame in the engine, so an alpha probe means a read-back
   (a staging copy and a `map_async` wait) either per frame (a full-window
   copy on every chrome present) or per `hitTest:` (a GPU sync on the main
   thread on every mouse move). The region costs one bounds pass per new
   frame and is exact for boxes. What it gives up: a click on a popup's
   shadow or in a rounded corner's transparent pixels goes to the chrome
   (the box's bounds), not the page; a click on the find modal's dimming
   layer over the page goes to the page (the layer is a backdrop, as with
   the WebKit chrome, where the page view received it).
5. **A `<select>`'s drop-down (CH2.E6, CH2.E6b).** It is not native: the
   engine draws it as the last item of the chrome frame's display list (a
   stacking context at `i32::MAX`, outside every clip of the page, placed
   in view coordinates within the whole window), so it paints over the page
   column and its box is in the region like any chrome popup (pinned by
   `select_popup_overlay_tests`). What CH2.E6b changed: while it is open the
   frame says so (`FrameCommit::modal_popup`) and, on the alpha route, the
   chrome's hit region is the whole view, so a press on the page column
   reaches the chrome's engine, which closes the drop-down and drops the
   press (#899's outside-press rule; the macOS menu takes that press too).
   The page does not get it. With `HIWAVE_RUSTKIT_CHROME_ALPHA=0` the region
   is also the mask, so the whole-view region is not applied there: a page
   press with a drop-down open still reaches the page and leaves it open.

Cost: the chrome's layer is now composited over the whole window by the
window server (it was masked to its region); see the PR for the measured
idle CPU/RSS against #925.

## Panels

The chrome stays the frontmost of the window's views: a panel (shelf,
Inspector) built after it is added on top, so `apply_layout` re-raises the
chrome (`raise_raw_view_to_front`, first responder kept) when it is not
the last subview. The panels lie inside the hole, so they show and take
the pointer wherever the chrome paints nothing, and a chrome popup that
reaches into a panel's rect is drawn over it (#909 R1 nit 2).

## Focus

* A click goes to the view `hitTest:` picks; the dispatch already makes
  that view the first responder (`grab_keyboard`, #907). Over the hole
  that is the page, so a page click takes the keyboard from the address
  bar, as with the WebKit chrome.
* A page click while a chrome popup is open also tells the chrome
  (`document.body` gets a synthetic `click`), so the chrome's own
  outside-click handler closes it, as it did when the page was pushed
  down and the click landed in the chrome.
* Cmd shortcuts: unchanged from #907. AppKit gives key equivalents to the
  muda menu before any `keyDown:`, so Cmd+L / Cmd+F / Cmd+T reach the menu
  from either view and the menu's action focuses the chrome field and
  takes the keyboard. "The focused document's keydown table first, then
  the menu" needs the document's answer (`defaultPrevented`) before AppKit
  hands the event to the menu, and the document answers on the content
  thread; it is not done here (a follow-up: `performKeyEquivalent:` returns
  YES, the key goes to the document, and an unprevented key is replayed to
  `[NSApp mainMenu] performKeyEquivalent:`).

## Resize

Nothing new moves: `apply_layout` already places the chrome at the whole
window and the page at its rect on every resize; it now also recomputes the
hole and re-applies the region (the mask's frame follows the layer's
bounds, set with implicit animations off so the mask does not lag the
frame).

## The `a3_dropdown_over_page` flake (CH3.A3b)

Not the region: on a failing run the app logged no mouse event at all, not
even the move before the click, so the HID click landed in another app's
window lying over HiWave's at that point (the driver never brought HiWave
to the front, and `screencapture -l` captures a covered window, so the
pixel asserts still passed). `hwdrive` now refuses an event (exit 5,
nothing posted) unless the process is frontmost and the topmost window at
the point is its own, and `driver.py` activates the app first and stops
(NOT RUN) on a refusal or when the seat's HID idle time says a person is
using the Mac.

## Found while gating (2026-11-05)

* The downloads and history panels are `position: absolute; top: calc(100%
  + 6px)` with no positioned ancestor, so their containing block is the
  initial one and they open below the viewport (measured in the RustKit
  chrome: top 1116 in an 800 px window). That is chrome.html's CSS, shared
  with the WebKit chrome, so it is left alone here; with the panel moved
  into a positioned toolbar box it would be drawn over the page like the
  context menu. The real-window gate uses the page context menu instead
  (`a3_dropdown_over_page`).
* The `.toolbar-action-panel` the reporter lists does not exist in the
  document (the menu button toggles the right sidebar); harmless.
