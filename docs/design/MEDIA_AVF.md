# Media on AVFoundation (MEDIA-AVF): design, plan first

Status: **approved by Pete on 2026-11-18** (#1331). Written 2026-11-18 against develop
`ecf18ff4`.

**Amended 2026-11-19 (§3.1, §3.2, §3.3): `AVPlayer` and `AVPlayerItem` are main-thread
only, so the players live on the main thread.** The amended §3.2 supersedes the original
design, where the engine's content thread owned the players. The decision is the
coordinator's (engine design is Atlas's lane).

Pete's media plan (2026-11-14) sets the frame:

- decode with the OS's decoders;
- write our own `<video>` element and controls;
- then Media Source Extensions;
- leave DRM (EME, FairPlay, Widevine) out of scope.

This note turns that plan into two milestones. Each has red-first tests and Chrome
for Testing 148.0.7778.216 pins, and each lands as reviewed PRs through the batch
gate.

The order of work: §1 current state, §2 the dependency, §3 Milestone 1, §4 Milestone 2,
§5 tests and pins, §6 what Pete decides.

## 1. Current state (read from the code at `ecf18ff4`)

### 1.1 `rustkit-media` (`crates/rustkit-media/src/lib.rs`, 919 lines, 13 unit tests)

`rustkit-media` is a workspace member, but no other crate depends on it. Nothing in the
engine, the bindings or the app calls it.

What is in it:

- **`HTMLMediaElement`:** a Rust state model of the element. It holds:
  - `NetworkState` and `ReadyState`, with the spec's values;
  - `Preload`;
  - `TimeRanges` (`add` merges ranges);
  - text-track structs;
  - a `MediaEvent` enum (`LoadStart`, `Play`, `Playing`, `TimeUpdate`, ...) sent on a
    tokio `mpsc` channel.

  Its methods (`set_src`, `load`, `play`, `pause`, `seek`, volume, muted, rate and
  `update_time`) move that state and send events. `can_play_type` answers from a fixed
  table: `"video/mp4"` gives `"maybe"`, the audio types give `"probably"`.
- **`AudioPlayer` and `VideoPlayer`:** stubs.
  - `load()` sleeps 100 ms and then sets fake metadata: 180 s of audio, or a 120 s
    1920x1080 video.
  - No byte is fetched and nothing is decoded.
  - `VideoFrame` (RGBA `Vec<u8>`) and `current_frame` exist, but nothing fills them.
- **`MediaManager`:** maps `MediaId` to the players, and has an `update(delta)` tick.
- **The `rodio` dependency** (wav, mp3, vorbis, flac) is declared but not used. The audio
  sink is a `TODO` comment.

What to keep: the state enums, `TimeRanges` and the event list. They are the right
shapes and will be checked against the HTML spec's algorithms as they are wired.

What goes: the simulated `load()`s and the `rodio` dependency. AVPlayer outputs its own
audio (§3).

### 1.2 The bindings (`crates/rustkit-bindings/src/dom.rs`, around line 3427)

`HTMLMediaElement`, `HTMLAudioElement` and `HTMLVideoElement` exist as interfaces, so
`instanceof` checks work. Their prototype is a deliberate "no playback" stub:

| member | today |
|---|---|
| `load()`, `pause()` | no-op |
| `canPlayType()` | `""` for everything |
| `play()` | rejects with `NotSupportedError` ("The element has no supported sources.") |
| `paused`, `ended`, `readyState`, `currentTime`, `duration`, `error` | plain writable data properties: `true`, `false`, `0`, `0`, `NaN`, `null` |
| `HAVE_*` constants | present |
| `MediaError` | an interface with its code constants; constructor illegal |

The stub was added so pages that call `video.load()` stop throwing (github.com). There
are no events, no `src` handling, and no `videoWidth` / `videoHeight`.

### 1.3 The engine and layout

- **UA style:** `video` and `audio` get `display: inline` (`lib.rs` around line 17314).
  Chrome's UA sheet makes `audio:not([controls])` `display: none`. Milestone 1 pins this.
- **Selectors:** `:paused` matches every `audio` and `video` (`lib.rs` around line
  24971). `:playing` never matches.
- **Box build:** there is no `video` case. `canvas`, `iframe` and `svg` each build a
  replaced `BoxType::Image` under a key of their own (`canvas_paint::canvas_key`,
  `frame_key`, the inline-svg key). A `<video>` gets no replaced box, so it does not get
  the CSS 300x150 default size, and its fallback children lay out as ordinary content.
- **Layout:** `video` is listed as a replaced element only for margin collapse
  (`is_own_context_for_collapse`) and container queries.
- **Paint:** nothing. A video paints only its background and border.

### 1.4 The paths this design reuses

- **Replaced box under a key.** `BoxType::Image { url: key, natural_width,
  natural_height }` reaches the display list as `DisplayCommand::Image`. That command
  has `object-fit`, `object-position` and the content box, which is what `<video>`
  needs: its default `object-fit` is `contain`.
- **Present-time upload.** The presenter (`presenter.rs`, around lines 774-870) walks
  the display list's image keys at present time. It uploads canvas bitmaps with
  `Renderer::upload_premultiplied_image` and decoded images with `upload_image`, and
  drops keys the list no longer names with `retain_images`.
- **Frame loop.** `Engine::settle_animations` keeps the live loop turning while
  `Animations::active()`, and the LAT2 paint-only frame (`paint_animation_frame`)
  repaints without a build. Video frames need the same kind of "keep ticking, paint
  without layout" hook.

## 2. The dependency: `objc2-av-foundation`

### 2.1 Crates to add

- **`objc2-av-foundation` 0.3.2:** AVPlayer, AVPlayerItem, AVPlayerItemVideoOutput,
  AVURLAsset and AVAssetResourceLoader.
- **`objc2-core-video` 0.3.2:** CVPixelBuffer and its lock and plane access.
- **`objc2-core-media` 0.3.2:** CMTime.

These are generated bindings from the `objc2` project (Mads Marquart and contributors).
They are on `objc2` 0.6, which `Cargo.lock` already carries (0.6.3), next to the
`objc2` 0.5.2 that `rustkit-viewhost` uses. Milestone 2 adds
`objc2-video-toolbox` and `objc2-audio-toolbox` (§4).

Only the AVFoundation features we call are enabled (`default-features = false`, then
`AVPlayer`, `AVPlayerItem`, `AVPlayerItemOutput`, `AVAsset`, `AVAssetResourceLoader`,
`AVTime`). This keeps compile time down and the notice list short.

### 2.2 Licence

| crate | version | licence (crates.io, read 2026-11-18) |
|---|---|---|
| objc2-av-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-video | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-media | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-video-toolbox (Milestone 2) | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-audio-toolbox (Milestone 2) | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2 (already in the lock) | 0.6.3 | MIT |

All are permissive and compatible with AGPL-3.0-or-later. Where a crate is
offered under several licences, we take it under **MIT**, as the notices script already
does for other crates.

These are bindings only. The AVFoundation framework itself is part of macOS and is not
shipped by us, just as AppKit and Metal are not today.

### 2.3 Notices

The dependency PR (the first PR of Milestone 1) does three things:

1. Adds the crates to `rustkit-media/Cargo.toml`, and makes `rustkit-media` a dependency
   of `rustkit-engine`, behind the `media-avf` feature (§3.8).
2. Runs `python3 scripts/third_party_notices.py`, which regenerates
   `THIRD_PARTY_NOTICES.md` from `cargo metadata`, and commits the result. Afterwards
   `python3 scripts/third_party_notices.py --check` must pass. Pete approved this rule
   on 11-14: new dependencies add an entry.
3. Reads the new crates' published files before adding them, for anything the script
   classifies (`LICENSE*`, `NOTICE*`, `THIRD_PARTY*`). The security-framework precedent
   in section 5 of the notices shows why: Apple-derived documentation can carry its own
   licence. Generated framework bindings could carry header-derived text, and if they
   do, it gets a `carry` entry in the script.

## 3. Milestone 1: one `<video src="x.mp4">` playing through AVPlayer

**Done when:** a plain `<video src="x.mp4" autoplay muted>` or a `<video controls
src="x.mp4">` on a page plays in HiWave. We see moving frames composited in the page,
with `object-fit` honoured, audio out, and the element's script surface and events
matching the Chrome 148 pins in §5.1.

The PR order is in §3.8.

### 3.1 Shape

```text
 page script / DOM          content thread (engine, Rc state)        main thread                 off-main
 ─────────────────          ──────────────────────────────────       ───────────                 ────────
 <video src>  ─attr─▶  MediaElements (per view, media.rs)      ─cmd─▶  MediaHost                    AVFoundation's
 video.play() ─call─▶    HTMLMediaElement state machine                  ├─ AVPlayer (per element)    own threads
                         events in Chrome's order       ◀─snapshot─      ├─ AVPlayerItem
 events ◀── fired at frame ticks ┘                                       └─ AVPlayerItemVideoOutput ─┐
                                                                                                     │ pull
 presenter: key `rustkit-video:<node>` ── copyPixelBufferForItemTime ◀───────────────────────────────┘
 rustkit-net ◀── byte ranges ── resource-loader delegate (its own dispatch queue) ◀── AVAssetResourceLoader
```

### 3.2 Ownership and threading (amended 2026-11-19)

**This section supersedes the original §3.2.** That version had the engine's content
thread own the `AVPlayer`s, which is not allowed. The macOS SDK marks `AVPlayer` and
`AVPlayerItem` `NS_SWIFT_UI_ACTOR` (`AVPlayer.h`, `AVPlayerItem.h`), and `objc2-av-foundation`
0.3.2 makes both `MainThreadOnly`: creating an item needs a `MainThreadMarker`. Step 2's
metadata probe is not affected, because `AVURLAsset` and its resource loader are not
main-thread only and step 2 never creates a player.

- **Who owns what.**
  - A **`MediaHost`** on the main thread owns one `AVPlayer`, `AVPlayerItem` and
    `AVPlayerItemVideoOutput` per media element. It is small: it runs commands and
    reports state, and decides nothing about events.
  - The engine's content thread keeps everything else: each element's `HTMLMediaElement`
    state machine (step 2's `media.rs`), the event queue and the box.
- **Commands go in.** The engine sends the host commands over a channel:
  - `create`
  - `load`, with the resource-loader scheme URL (§3.4)
  - `play`, `pause`, `seek`, `rate`, `volume`
  - `destroy`

  Each command is dispatched onto the main queue (`dispatch2`'s
  `DispatchQueue::main().exec_async`), where the host runs it.
- **State snapshots come back.** After each command, and on a short main-thread timer
  while any player is potentially playing, the host reads each player and sends back a
  snapshot over a second channel:
  - readyState, currentTime, duration;
  - paused, ended, error;
  - natural size, loaded ranges.

  The host reads only the players' properties (`status`, `timeControlStatus`,
  `currentTime()`, `duration`, `presentationSize`, `loadedTimeRanges`). The end of the
  item is cross-checked with `AVPlayerItemDidPlayToEndTimeNotification`, which only sets
  a flag in the next snapshot.
- **No KVO and no callbacks into engine state.** Nothing on the main thread or on
  AVFoundation's threads touches the engine.
- **Events.** At each frame tick, the engine drains the snapshots. It turns each change
  into its element's state transitions and queues the events in Chrome 148's order
  (§5.1). Events fire after the tick's build, in queue order, as transition events do.
  One code path decides the order, so it is deterministic and testable.
- **Keeping the loop turning.** While a command is unanswered or a player is potentially
  playing, the view's media is active and the live loop keeps turning, as running
  transitions do. A paused video costs nothing per frame.
- **Frames are pulled off the main thread** (§3.3).
  - `AVPlayerItemOutput` and `AVPlayerItemVideoOutput` are `NS_SWIFT_SENDABLE` in
    `AVPlayerItemOutput.h`, and `objc2-av-foundation` makes them `Send + Sync`.
  - So the host hands each output, a cheap retained reference, to the present side when
    it creates the player.
  - `hasNewPixelBufferForItemTime` and `copyPixelBufferForItemTime` are called there,
    never on the main thread.
- **Tests.**
  - Event-order and state-machine tests run on the engine thread against a `FakeBackend`
    that stands in for the host and answers commands with scripted snapshots (§5).
  - The real `MediaHost` needs a main thread whose main queue drains. Its tests run in a
    test binary of their own (`harness = false`), whose `main` runs the CFRunLoop.
  - In the app, the main thread is AppKit's, whose run loop already drains the main queue.

### 3.3 Frames: from AVPlayerItemVideoOutput to the display list

**Output.** Each player has an `AVPlayerItemVideoOutput` with these pixel-buffer
attributes:

- `kCVPixelBufferPixelFormatTypeKey = kCVPixelFormatType_32BGRA`;
- `kCVPixelBufferMetalCompatibilityKey = true`;
- `kCVPixelBufferIOSurfacePropertiesKey = {}`.

So every frame is an IOSurface-backed BGRA buffer.

**Per present, for each playing video, on the present side (off the main thread, §3.2):**

1. `itemTimeForHostTime(CACurrentMediaTime())` gives the item time to show now.
2. If `hasNewPixelBufferForItemTime(t)`, take `copyPixelBufferForItemTime(t,
   itemTimeForDisplay)` and put the `CVPixelBuffer` in the player's single frame slot.
   It replaces the previous frame, so only the latest is kept and no queue grows.
3. Mark the element's frame dirty.

**Layout.** The box build gets a `video` case, like `canvas`: a replaced
`BoxType::Image` under the stable key `rustkit-video:<node>`.

- Its natural size is the video's `presentationSize` once metadata is known.
- Before that it is 300x150, or the `width` / `height` attributes.
- The UA default `object-fit` is `contain`.
- A size change (a metadata arrival, or a resolution switch) is a relayout. A new frame
  never is.

**Paint and upload, without a build.** A new frame does not change the display list:
the key is stable. The present that follows uploads the frame slot's buffer into the
texture already under that key.

- **Step 1a, the copy (first PR):** the presenter locks the `CVPixelBuffer`, swizzles
  BGRA to RGBA and calls a new `Renderer::update_image(key, w, h, rgba)`. That call
  `write_texture`s into the existing texture when the size matches, and only creates a
  new texture when the size differs. One 1080p frame is about 8 MB per present, which
  is acceptable to get correctness and pins first. Its cost is measured (§5.1, P-cost).
- **Step 1b, zero copy (its own PR, after 1a is pinned):**
  1. Wrap the pixel buffer's IOSurface with `CVMetalTextureCacheCreateTextureFromImage`.
  2. Hand it to wgpu as a `Texture` through `wgpu::hal::metal` (`texture_from_raw`) and
     `Device::create_texture_from_hal`.
  3. Sample it in place as `Bgra8Unorm`.
  4. Keep the `CVMetalTexture` alive until the frame's command buffer completes.

  This is the only unsafe interop in the design, and it is isolated in one
  presenter-side module.

**A video-only frame:** the tick finds a new buffer and the display list is unchanged,
so the frame is a present-only frame, with no build and no layout. It needs a "textures
changed" flag on the view that `paint_animation_frame` (LAT2) honours, so a page whose
only motion is video never relayouts.

### 3.4 Bytes: every request goes through rustkit-net

AVPlayer fetches a plain `https:` URL itself, with Apple's URL loading. That would
bypass:

- HiWave Shield (blocking);
- our cookie jar and partitioning;
- `Referer` policy;
- CORS for `crossorigin` media;
- the request log the privacy pillar depends on.

So the asset is created with a private URL scheme (`rustkit-media://<view>/<player>`),
which AVURLAsset cannot load by itself. An `AVAssetResourceLoaderDelegate`, written in
Rust with `objc2` `define_class!` and running on a serial dispatch queue we create,
answers each `AVAssetResourceLoadingRequest`:

- **Content information:** content type (UTI), content length and byte-range support
  come from a `HEAD` request, or from the first ranged `GET`.
- **Data requests:** each one becomes a ranged `GET` (`Range: bytes=a-b`) through
  `rustkit-net`, on the engine's tokio runtime. Chunks go back through
  `dataRequest.respondWithData`, then `finishLoading` or `finishLoadingWithError`.
  Cancellation (`didCancelLoadingRequest`) aborts the fetch.
- **Shield:** a request the shield blocks finishes with an error, and the element moves
  to `MEDIA_ERR_SRC_NOT_SUPPORTED` (or `NETWORK`, per the pin).

The delegate thread touches no engine state. It only holds an `Arc` to a small
request-forwarding channel. `data:` and `blob:` sources go through the same delegate:
their bytes come from memory instead of the network.

### 3.5 The element and its script surface (Milestone 1 scope)

The stubbed members in dom.rs become host calls into the `MediaHost`. They follow the
HTML spec's algorithms, and use the kept `HTMLMediaElement` model for state:

- **Attributes:** `src`, and `<source>` children (resource selection: first source with
  a type `canPlayType` does not reject), `currentSrc`, `preload` (default `metadata`),
  `autoplay`, `muted`, `loop`, `poster`.
- **Methods:** `load()`, `play()` (returns a Promise resolved at `playing` and rejected
  per the autoplay policy), `pause()`, `canPlayType()`.
- **Attributes, read and write:** `currentTime` (a write seeks with zero tolerance:
  `seekToTime:toleranceBefore:kCMTimeZero toleranceAfter:kCMTimeZero`), `duration`,
  `paused`, `ended`, `seeking`, `readyState`, `networkState`, `buffered`, `played`,
  `seekable`, `volume`, `muted`, `defaultMuted`, `playbackRate`, `error`,
  `videoWidth`, `videoHeight`.
- **Events, in the spec's order:**
  - `loadstart`, `durationchange`, `loadedmetadata`, `loadeddata`, `canplay`,
    `canplaythrough`;
  - `play`, `playing`, `timeupdate` (at the spec's 15-250 ms cadence), `pause`;
  - `seeking`, `seeked`, `ended`, `volumechange`, `ratechange`;
  - `progress`, `suspend`, `stalled`, `waiting`, `error`, `emptied`, `abort`.
- **`canPlayType`:** answered from AVFoundation, not from a table.
  - `AVURLAsset.isPlayableExtendedMIMEType` gives `"probably"` when codecs are named.
  - A container with no codecs is `"maybe"`.
  - Otherwise `""`.
  - Chrome's answers for the same strings are pinned (§5.1), and differences are
    listed.
- **Autoplay policy:** Chrome's default (muted autoplay allowed, unmuted needs a user
  activation) is pinned. It uses the user-activation state the engine already tracks
  for popups.
- **`:playing` / `:paused`:** these selectors follow the real state.

### 3.6 Our controls

`controls` draws a control bar that RustKit renders in the page: a closed
UA shadow tree of ordinary elements with play/pause, a scrubber, the time, mute,
volume and fullscreen (fullscreen is a stub until the Fullscreen API exists). It is
styled by a UA sheet and driven by the same host calls script uses. Being DOM, it is
hit-tested, focused and painted like page content, and it needs no new paint path.

Its look is pinned against Chrome only for size: the bar's height and the element's
default size with `controls`. Its design is ours.

This is the last PR of Milestone 1 (§3.8).

### 3.7 Out of Milestone 1

These are not in Milestone 1 and are not dropped either:

- text tracks (`<track>`, WebVTT);
- `requestVideoFrameCallback`;
- Picture-in-Picture;
- drawing a video onto a canvas (`drawImage(video)`), and video as a WebGL texture;
- `captureStream`;
- Media Session.

These are out of scope and stay out: DRM / EME (Pete, 11-14), and live HLS.

AVPlayer would play `.m3u8` natively; whether to allow it is a later decision.

### 3.8 PR order (Milestone 1)

Each step is its own PR, red first, behind the `media-avf` feature until step 6:

1. **Dependency and notices** (§2), plus the `<video>` replaced box: the 300x150
   default, the attribute sizes and the UA style pins. No playback yet.
2. **`MediaHost` and the polling tick.** Metadata and events, using a local fixture
   through the resource loader (rustkit-net serves the test server). `canPlayType`.
3. **Frames, step 1a** (copy upload, present-only frames). This PR gets the pixel pins.
4. **`play` / `pause` / seek / volume / rate / loop / autoplay policy**, with their
   event-order pins.
5. **Controls.**
6. **Feature on by default.** A real-window run plays a fixture in the app. Frames,
   step 1b (zero copy), follows as a performance PR with before and after numbers.

## 4. Milestone 2: MediaSource and SourceBuffer

**Done when:** a page that feeds fragmented MP4 (H.264/AAC first, then HEVC) through
`MediaSource` / `SourceBuffer.appendBuffer` plays, seeks and switches quality as in
Chrome 148, with the §5.2 pins green. This is the path that most streaming sites and
adaptive players (hls.js, dash.js, Shaka) use.

### 4.1 Why not AVPlayer

AVPlayer pulls from a URL. MSE has script push byte segments, so AVPlayer has no entry
point for it. The public building blocks for push playback are:

- **Video:** `VTDecompressionSession` (VideoToolbox) decodes `CMSampleBuffer`s into
  IOSurface-backed `CVPixelBuffer`s. These are the same buffers Milestone 1 already
  composites, so the frame path from §3.3 is reused unchanged.
  `AVSampleBufferDisplayLayer` is not used: it draws into a CALayer, outside our wgpu
  compositor, so `object-fit`, transforms, clipping and z-order would be wrong.
- **Audio:** `AVSampleBufferAudioRenderer` under an `AVSampleBufferRenderSynchronizer`.
  The synchronizer's timebase is the media clock, and video frames are chosen against
  it at each tick.

### 4.2 Pieces

- **Bindings:**
  - `MediaSource`: `readyState`, `duration`, `addSourceBuffer`, `removeSourceBuffer`,
    `endOfStream`, `setLiveSeekableRange`, `isTypeSupported`, and the
    `sourceopen` / `sourceended` / `sourceclose` events.
  - `SourceBuffer`: `appendBuffer`, `abort`, `remove`, `mode`, `timestampOffset`,
    `appendWindowStart` / `appendWindowEnd`, `updating`, `buffered`, and the
    `updatestart` / `update` / `updateend` / `error` / `abort` events.
  - `URL.createObjectURL(mediaSource)` attaching to the element.
  - The spec's coded-frame processing, removal and eviction algorithms in Rust, as a
    pure state machine with no OS calls, so they are unit-testable.
- **Demux:** our own ISO BMFF (fMP4) parser in `rustkit-media`:
  - initialization segments (`ftyp`, `moov`, `trak`, `avcC` / `hvcC` / `esds`);
  - media segments (`moof` / `mdat`, `tfdt`, `trun`).

  It turns these into coded frames with their timestamps. It is written in safe Rust
  with no recursion driven by page input (the same rule as the SVG work). It is fuzzed
  as part of its PR. WebM (Matroska) is a later step, because VP9 / AV1 decode on
  macOS depends on the OS version.
- **Decode:**
  - Video: coded frames become `CMSampleBuffer`s
    (`CMVideoFormatDescriptionCreateFromH264ParameterSets` or `...HEVC...`), which go
    to `VTDecompressionSession`.
  - Audio: AAC frames go to `AVSampleBufferAudioRenderer`.
  - Seeking flushes both and re-enqueues from the nearest preceding sync frame.
- **Dependencies:** `objc2-video-toolbox`, `objc2-core-media` (already in M1) and
  `objc2-av-foundation` (already in M1), and `objc2-audio-toolbox` if the audio path
  needs it. Each gets its notices entry through the same script step as §2.3. The
  licences are in the §2.2 table.

### 4.3 PR order (Milestone 2)

1. The `MediaSource` / `SourceBuffer` state machine and `isTypeSupported`, with no
   decode: `buffered` ranges and event order pinned.
2. The fMP4 demuxer, with its fuzz target.
3. VideoToolbox video decode into the M1 frame path.
4. Audio renderer and synchronizer clock, A/V sync.
5. Seeking, `remove`, eviction, quality switch (a new init segment mid-stream).
6. An hls.js or dash.js fixture page, served locally, plays end to end in a real-window
   run.

## 5. Tests and Chrome pins

All expected values come from Chrome for Testing 148.0.7778.216 through new
`tools/parity_oracle/media_*_log.mjs` scripts. Each writes a receipt under
`docs/diagnostics/<date>/`, and the tests read the receipt, as the
transitions work does.

**Nothing is pinned yet.** The first step of each PR is to run its oracle.

**First check, before any pin:** whether this Chrome for Testing build decodes H.264 and
AAC. If it does not (if its codec set is Chromium's), the pins that need decoding
(frame pixels, `loadeddata` timing) are taken with the same content in an open codec
that Chrome plays (VP9 / Opus in WebM). That content is played by RustKit in MP4, and
the receipt says so. Timing and event order are codec-independent.

**Fixtures** are generated, not downloaded:

- A script in `tools/media_fixtures/` writes them with AVAssetWriter (no ffmpeg, no
  third-party encoder) and commits them.
- `color-steps.mp4`: 64x48, 2 s at 10 fps; frame *n* is a solid colour from a fixed
  table, so a pixel identifies the frame; with a 440 Hz sine on the audio track.
- A 4x3 one-frame clip, for the size pins.
- An fMP4 split of `color-steps` (init segment plus 1 s segments), for Milestone 2.

**Determinism.** Engine tests run headless. The clock tests drive a `MediaBackend`
trait with two implementations:

- `AvfBackend`, the real one;
- `FakeBackend`, which yields fixture frames and states on the page's virtual clock.

Event-order and state-machine tests run on `FakeBackend` and are exact. A smaller set
runs on `AvfBackend` and is marked as needing macOS media: it seeks paused, with zero
tolerance, so frame pixels are exact, and it never depends on wall-clock playback
speed.

### 5.1 Milestone 1 pins (what each red test asserts)

| id | case | measured in Chrome 148 |
|---|---|---|
| P-size | `<video>` with no src; with `width=` / `height=`; after metadata of the 64x48 fixture; with `controls` | box size, `videoWidth` / `videoHeight` |
| P-ua | `audio` with and without `controls`; `video` default `object-fit` | computed `display`, `object-fit` |
| P-events | `src` set, `preload=auto`, then play to the end | the full event sequence with `readyState` / `networkState` at each event |
| P-meta | after `loadedmetadata` | `duration`, `videoWidth`, `videoHeight`, `seekable`, `buffered` |
| P-seek | paused, `currentTime = 0.55` | `seeking`, `seeked` order; the painted centre pixel is frame 5's colour |
| P-poster | `poster=` before and after the first frame | painted pixel |
| P-fit | `object-fit: contain / cover / fill` on a 200x100 box | painted pixels at the letterbox edges |
| P-canplay | a list of MIME strings, with and without codecs (`video/mp4; codecs="avc1.42E01E, mp4a.40.2"`, `hvc1`, `video/webm; codecs="vp9"`, ...) | `canPlayType` answers |
| P-autoplay | `autoplay` muted / unmuted, `play()` without activation | Promise outcome, events |
| P-error | 404 src, an unsupported type, a shield-blocked src (blocked in the Chrome run by request interception) | `error.code`, events |
| P-net | (RustKit only) | every media byte request is visible to rustkit-net and the shield; no request leaves through Apple's loader |
| P-cost | (RustKit only) | per-frame cost of step 1a at 1080p30, debug and parity profiles; a page with one playing video does no relayout per frame |

### 5.2 Milestone 2 pins

| id | case | measured in Chrome 148 |
|---|---|---|
| M-type | `MediaSource.isTypeSupported` over the P-canplay strings | answers |
| M-open | `src = URL.createObjectURL(ms)` | `sourceopen` timing, `readyState` |
| M-append | init segment, then segments 1 and 2 | `updatestart` / `update` / `updateend` order, `buffered` after each |
| M-gap | segments 1 and 3 only | `buffered` ranges; playback stalls (`waiting`) at the gap |
| M-offset | `timestampOffset = 10` before segment 1 | `buffered` |
| M-remove | `remove(0.5, 1.5)` | `buffered`, events |
| M-end | `endOfStream()` | `duration`, `ended` at the end |
| M-seek | seek into buffered and unbuffered time | events, painted frame pixel |
| M-switch | new init segment (other resolution) mid-stream | `videoWidth` / `videoHeight` change, `resize` event |
| M-error | a corrupt segment | `error` on the SourceBuffer, `decode` error on the element |

## 6. What Pete decides

1. **This plan as a whole:** two milestones, AVPlayer for `src`, VideoToolbox for MSE.
2. **The dependency:** `objc2-av-foundation`, `objc2-core-video` and `objc2-core-media`
   (Milestone 1), then `objc2-video-toolbox` (Milestone 2). All are permissive,
   taken as MIT, with notices entries.
3. **All media bytes through rustkit-net via the resource loader (§3.4).** This costs
   work up front, but is what the privacy pillar requires. The alternative, letting
   AVPlayer fetch, is faster to build and would leave media traffic outside the shield.
4. **`.m3u8` (native HLS through AVPlayer):** allow it after Milestone 1, or not at all,
   leaving HLS to MSE players.
5. **Timing within the 36 h unfreeze:** none of this is code until approved. The first
   PR after approval is §3.8 step 1.
