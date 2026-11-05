# CH3.A3 composition: the RustKit chrome over the page

Flag: `HIWAVE_RUSTKIT_CHROME=1` only. Flag off (the WebKit chrome) is not
touched: no call below runs for `ChromeView::WebKit`.

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

## What is transparent: the chrome's painted boxes, not the frame's alpha

The chrome is opaque inside its *region* and absent outside it. The region
is a list of rects in view coordinates (top-left origin):

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

Why not the alpha of the painted frame: the renderer clears to opaque
white and the chrome's `<body>` paints `--bg-primary` over the whole
window, so alpha transparency needs a transparent body plus a non-opaque
layer and a premultiplied surface (engine and compositor changes that
would reach the page view too); and `hitTest:` runs on every mouse move on
the main thread, where reading the frame's alpha means a GPU read-back.

Limits, by construction (the surface is opaque, the mask is binary): a
popup's `box-shadow` outside its border box is not shown over the page
(revealing it would show a block of the body's colour, so shadows are left
out of the region); a rounded popup's corners show the body's colour in
the corner pixels; a full-window translucent overlay (the find modal's
dimming layer) covers the hole and is a backdrop, so it does not dim the
page; a popup the engine draws that is not in the chrome's display list (a
`<select>`'s native drop-down, CH2.E6) is only visible inside the region.
All of these need the alpha route above.

### Panels

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
