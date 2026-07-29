# Bucket (b) GPU pin — what a second backend needs decided first

**Owner:** Atlas (pathfinder) · **Date:** 2026-07-29
**For:** Athena (Windows), Talos (Linux), after Rail B′ (a) lands
**Requested by:** Prometheus — *"Bucket (b) GPU: wait Atlas one-pager; do not block (a)."*

Rail D put 23 functions / 1,582 LOC in bucket (b): the `rustkit-renderer`
functions that touch `wgpu` directly. wgpu is cross-platform, so the default
expectation is that they port. This pin exists because **three assumptions are
baked in and none of them is checked at runtime** — measured, not recalled.

Do not treat this as a blocker. Bucket (a) is 5,813 LOC, roughly 3× (b). Take
(a) first; this lands while you port.

---

## Measured, on `crates/rustkit-renderer` + `crates/rustkit-compositor`

| Question | Answer |
|---|---|
| Surface format | `CompositorConfig.format` **defaults to `Bgra8Unorm`**, and a test asserts it |
| Is it queried from the surface? | **No.** No `get_capabilities()` / `get_preferred_format()` call anywhere in either crate |
| Intermediate & filter textures | `TextureFormat::Rgba8Unorm` hardcoded ×4 |
| Compute | `backdrop_filter.wgsl`, three entry points at `@workgroup_size(16, 16, 1)`, 3 `dispatch_workgroups` |
| Non-compute fallback for blur/filter | **None** |
| Adapter feature / limit / downlevel checks | **Zero.** Not one `Features::`, `DownlevelFlags::`, `required_features` or `.limits()` in either crate |

That last row is the pin. The renderer does not ask the adapter what it can do;
it assumes, and Metal has always said yes.

---

## The three decisions

### 1. Surface format — query, or hardcode and blit?

`Bgra8Unorm` is Metal's preferred surface format, which is why hardcoding it
has never hurt. D3D12 and Vulkan commonly prefer it too, so this may well
survive the port — but it survives **by luck**, and nothing in the code says
so or fails loudly when the luck runs out.

**Recommendation:** query `surface.get_capabilities(&adapter).formats[0]` and
keep the config value as an override rather than the source of truth. It is a
small change on the pathfinder and it converts a silent assumption into a
value you can read in a log.

**Decide before porting**, because a Windows tree that hardcodes a *different*
format is two divergent assumptions instead of one shared mechanism — and
that is the thing the topology exists to prevent.

### 2. Intermediate textures — `Rgba8Unorm` and the sRGB boundary

Filter/intermediate textures are `Rgba8Unorm` (linear, no automatic sRGB
conversion), while the renderer also carries `srgb_to_linear` ×7 and
`linear_to_srgb` ×4 in code. So **some** conversion is manual and the surface
may or may not be doing its own — that boundary is nowhere written down.

**Decide:** where does sRGB conversion happen, once, in writing. A second
backend that guesses differently produces colour drift that no parity number
will attribute correctly, because the pixels will be *nearly* right.

This one I would settle even if Windows never happened. It is a latent
correctness question on macOS today.

### 3. Compute — required, or degradable?

Blur and colour filters are compute-only with no raster fallback. Workgroup
size `16×16×1` is 256 invocations, within every mainstream limit, so the
*size* is almost certainly portable. The risk is the **capability**: a
downlevel adapter without compute support gets a device-creation or pipeline
failure, and because nothing queries `Features`/`DownlevelFlags`, the failure
surfaces as a crash rather than a diagnosis.

**Decide:** is compute a hard requirement (fail fast with a clear message at
adapter selection) or a degradable path (raster fallback, filters off)?
Either is defensible; not choosing means the answer is "crash, unhelpfully."

---

## What porting seats should do

1. Take bucket (a) now. Nothing here blocks it.
2. **Do not invent a second answer.** If any of the three above bites during
   the port, report it and let the pathfinder settle it — a Windows-only
   surface format or sRGB convention is exactly the divergence the
   macOS-as-truth topology is meant to avoid.
3. When (b) does port, carry a **rendering receipt**, not a compile receipt.
   These functions produce pixels; "it builds" proves nothing about them, and
   the Rail D classification explicitly warned that bucket (a)'s
   "no visible platform dependency" is a signal rather than a proof.

## What this pin does not do

It does not add the capability checks. That is a pathfinder change with its own
blast radius and its own review, and writing it into a document that porting
seats read as settled would be the same mistake as an unwired config field —
a decision that looks made and is not.

**Open for Pete only if §1 or §3 changes observable behaviour on macOS.** My
read is that querying the surface format and adding a clear compute-capability
error are both invisible when the answer is what we already assume, so they are
mine to land — but §2's sRGB boundary could move pixels, and if it does, that
is a parity-number change and goes to Pete.
