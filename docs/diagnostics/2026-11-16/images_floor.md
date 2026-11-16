# IMAGES-FLOOR: why MotionMark's Images test sits at 1.00

Measurement note, 2026-11-16. No engine change in this PR. Chrome is Chrome for Testing 148.0.7778.216.
Our engine is develop a8292900 with #1205 merged (a scratch release build; #1205 is what lets the
suite's frame load at all).

The full real-window run scored Images 1.00 at about 22 fps, with the complexity moving between 1 and 6.

## Summary

1. **The page never reaches its real work in our engine.** The test runs in a frame, and in a frame
   `drawImage` of a loaded image draws nothing. At top level it draws. Every canvas therefore reads back
   blank every frame and is drawn again every frame.
2. **Reading one 50x50 canvas back costs about 19 ms, and writing it about 7 ms.** Chrome: about 0.01 ms
   and 0.001 ms. The pixels cross between host and script as text and are converted one byte at a time
   in script. One canvas read back per frame is already more than a 60 fps frame. That is the floor.
3. Layout, paint and the image draw itself are not the floor. Moving and redrawing 64 canvases every
   frame holds 60 fps in the real window; the relayout for it takes 0.4 ms.

## What the page does each frame

From reading the suite (described, not copied) and from a probe on the running page. The stage holds
small canvases, 50x50, absolutely positioned. The complexity is the number of active canvases. For each
active canvas, each frame:

- read the whole canvas back (`getImageData`);
- in script, visit the 2,500 pixels: skip one that shows nothing, otherwise give it the colour of a near
  neighbour (itself most of the time), with two random numbers per pixel;
- if any pixel was shown, write the pixels back (`putImageData`);
- otherwise the canvas has gone blank: move it (`style.top`, `style.left`) and draw one of 13 SVG images
  (48x48 natural size) into it, scaled to 50x50.

In Chrome a drawn image lasts on the order of a hundred frames before it has eroded to nothing (174 steps
for the shape of our probe), so nearly every frame is read, loop, write.

## What happens on our side, on the real page

A probe script put in front of the suite's runner page (mutation records under the stage, wrapped canvas
calls), headless, 30 s test:

| per frame, per active canvas | ours |
|---|---|
| `getImageData` | 1 |
| `putImageData` | 0, never called in the whole run |
| `drawImage` | 1 |
| `style` writes | `top` and `left` |
| elements under the stage | 1 to 6 canvases, nothing else |

The first six `drawImage` calls were looked at one by one: the argument is an `img`, `complete`, natural
size 48x48, five arguments, the canvas 50x50 and connected; the canvas shows 0 pixels after each.

## The shape, pinned

Our own probe pages (this directory):

- `image_data_frame.html` + `image_data_inner.html`: what a canvas holds after an image is drawn into
  it, in a frame and in the same document loaded on its own.
- `image_data.html`: what a canvas holds after a fill, a PNG, an SVG and a `putImageData`; how many steps
  of the shape one drawn image lasts; what each call costs; what a frame costs with 1, 4, 16 and 64
  canvases, each stepped, each moved and redrawn, each only moved.

Records: `image_data_cft148.json`, `image_data_frame_cft148.json` (Chrome, `image_data_chrome.mjs`),
`image_data_engine.json` (ours, headless and real window).

### What the canvas holds

| shown pixels of 2,500 | Chrome | ours |
|---|---|---|
| top level, after the SVG image / the PNG image | 900 / 900 | 900 / 900 |
| **in a frame, after the SVG image / the PNG image** | **900 / 900** | **0 / 0** |
| in a frame, after `fillRect(0, 0, 5, 5)` | 25 | 25 |
| `image_data.html`: every value under `holds`, and the 174 steps | | identical to Chrome |

Also checked in our engine and all as expected (scratch pages, not kept): the suite's own SVG files draw
at top level; a canvas keeps its pixels across a frame, a move, `display: none` and back, insertion after
drawing, and a sibling being added.

Where it comes from, by reading: the engine gives a top-level page's bindings a canvas image loader
(`set_canvas_image_loader`, twice in `rustkit-engine/src/lib.rs`), and gives a frame's bindings the text
measurer only (`drive_frames`). A canvas with no loader finds no bitmap for the image and returns without
drawing.

### What each call costs

Milliseconds per call on one 50x50 canvas. Ours is the same within 3% across three runs (1-minute load
from 2.3 to 9), headless and real window. Chrome's clock is coarse at this scale.

| | Chrome | ours | 
|---|---|---|
| `getImageData` | 0.012 | 19.2 |
| `putImageData` | 0.001 | 6.7 |
| reading 2,500 alpha values in script | 0.005 | 0.80 |
| `drawImage`, PNG / SVG | 0.000 / 0.002 | 0.06 / 0.05 |
| `getContext("2d")` | 0.001 | 0.003 |
| one whole step (read, loop, write) | 0.024 | 27.2 |

Where the 19 ms and 7 ms go, by reading `web_canvas.js` and `web_canvas.rs`: the host returns the pixels
as one hex string and the script side turns it into a `Uint8ClampedArray` with a `substr` and a
`parseInt` per byte (10,000 of each for this canvas); on the way back the script side makes a two-digit
string per byte and joins them, and the host parses the text.

### What a frame costs

Mean milliseconds between frames over 40 frames. Chrome ran at 120 Hz here (8.3 ms) and held it in every
row.

| canvases | ours, each stepped (headless / real window) | ours, each moved and redrawn | ours, each moved |
|---|---|---|---|
| 1 | 27.0 / 31.3 | 16.7 | 16.7 |
| 4 | 107.5 / 107.5 | 16.7 | 16.7 |
| 16 | 425 / 427 | 16.7 | 16.7 |
| 64 | 1,695 / 1,718 | 16.7 | 16.7 |

The engine's own counters for those frames: a stepped frame of 64 canvases is 1,783 ms of script in a
1,785 ms turn; the relayout after 64 canvases moved takes 0.4 ms. (The first "moved and redrawn" row of
the real-window run had one 678 ms frame; the rest were 17 ms.)

## How the numbers explain the score

Today each active canvas costs a blank read-back (19.2 ms), a scan that finds nothing (0.8 ms), two style
writes and a draw that draws nothing: about 20 ms. One canvas does not fit a 60 fps frame, two do not fit
a 30 fps frame. The controller can only sit at the bottom: 1.00.

## Plan, ranked

1. **IMG-F1, a frame's canvas draws images** (correctness; nothing below is measured honestly without
   it). Give a frame's bindings what a top-level page's get for canvas images. To check while doing it:
   the origin-clean decision uses the frame's own origin, and an SVG image fetched for a frame's document
   reaches the store the loader reads. Test: `image_data_frame.html` end to end against
   `image_data_frame_cft148.json`. Expected effect on the score alone: none or slightly worse, because
   the page then does its real work at 27 ms a canvas.
2. **IMG-F2, pixels cross the host boundary as bytes.** `getImageData` and `putImageData` fill and read
   the typed array's buffer in the host, with no text in between, through host natives under the bindings'
   hardening rules. Expected: 19.2 ms and 6.7 ms each fall to well under 0.1 ms, leaving about 1.3 ms of
   script per canvas per frame (27.2 less 19.2 and 6.7), so on the order of ten canvases in a 60 fps
   frame. Every page that reads a canvas back gains, not only this test. Test: the `holds` values of
   `image_data.html` against the pin, and a count of host calls per `getImageData` rather than a time.
3. **IMG-F3, the loop itself.** After F2 the remaining cost is typed-array reads and writes and the
   random numbers in script: 0.80 ms against 0.005 ms for the bare read of 2,500 values. This is the
   interpreter row that the sort-with-comparator measurement already opened, not an Images row; measure
   again after F2 before deciding anything here.
4. **Measure again after F2:** painting many canvases that all changed in one frame. Today the stepped
   frames are all script, so the real window could only show that 64 canvases moved and redrawn from an
   image hold 60 fps. Whether 64 canvases written by `putImageData` do is not known yet.

Not on the list, because measured and small: layout for moved canvases, `drawImage`, `getContext`.

## Limits of this note

- Timings come from a shared machine and a scratch build; the ratios are large enough not to depend on
  that, the absolute values are not targets.
- The cause in "where it comes from" is read from the source, not proven by a fix; the fix PR proves it.
- No score is predicted for after F1 and F2.
- Tooling, seen on the way: the app log cuts a console line near 1,000 characters, so a probe should log
  short lines.
