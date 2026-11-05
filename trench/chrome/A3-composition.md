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
so the order is page, chrome, shelf, Inspector. The chrome stays the whole
window. `expand_chrome` no longer moves the page (the panel is drawn over
it); `expand_chrome_small` still does, because the tab-actions bar is in the
chrome's flow and pushes the toolbar down, as before.

## What is transparent: the chrome's boxes, not the frame's alpha

The chrome is opaque inside its *region* and absent outside it. The region
is a list of rects in view coordinates (top-left origin):

    region = window − hole + overlays

* `hole` is the page column the app already lays out: x from the left
  sidebar's width to the right sidebar, y from the chrome height to the
  bottom (page, Inspector and shelf). `apply_layout` computes it with the
  page's rect and hands it to the chrome every time it places the views.
* `overlays` are the boxes of the chrome's popups that are showing, which
  the chrome document reports itself: a small script appended to the
  RustKit chrome's bridge reads `getBoundingClientRect()` of a fixed
  selector list (downloads, history, toolbar-action and help panels, the
  context menu, the blocklist dropdown, the find bar) and posts
  `{"cmd":"chrome_overlays","rects":[[x,y,w,h],...]}` when the list
  changes: after any click or key in the chrome, and once a second. The
  app takes that message before `handle_chrome_ipc` (which never sees it).

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
Boxes are synchronous, cost nothing per event, and the chrome document
already knows them. The pure pieces (`region = window − hole + overlays`,
`point in region`) are in `rustkit-viewhost` with unit tests.

Limits, by construction: a popup's `box-shadow` outside its border box is
clipped; a popup the engine draws that is not a DOM box (a `<select>`'s
drop-down, CH2.E6) is only visible where it lies inside the region (the
Mode select is in the sidebar, which is in the region); a popup class
missing from the selector list is clipped at the hole's edge.

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
