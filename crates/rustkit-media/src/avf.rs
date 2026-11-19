//! AVFoundation glue for MEDIA-AVF (`docs/design/MEDIA_AVF.md`).
//!
//! Step 1 added the dependency and `is_playable_extended_mime_type`. Step 2 adds
//! [`Probe`]: an `AVURLAsset` on a private URL scheme whose every byte comes from a
//! [`MediaBytes`] the engine fetched through rustkit-net (and so through the shield),
//! answered by an `AVAssetResourceLoaderDelegate` on a serial dispatch queue of our own
//! (§3.4). The probe loads the asset's `playable`, `duration` and `tracks` and reports
//! them when they are in: what `loadedmetadata` needs. It never plays.
//!
//! Threading: `AVURLAsset` and its resource loader are not main-thread-only, so a probe
//! is created and polled on the engine's thread. Its delegate runs only on the probe's
//! own queue and reads only the immutable bytes. (`AVPlayer` and `AVPlayerItem` ARE
//! main-thread-only in the SDK, which steps 3 and 4 have to take into account.)

use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_av_foundation::{
    AVAssetResourceLoader, AVAssetResourceLoaderDelegate, AVAssetResourceLoadingRequest,
    AVAsynchronousKeyValueLoading, AVMediaTypeAudio, AVMediaTypeVideo, AVURLAsset,
};
use objc2_foundation::{NSArray, NSData, NSString, NSURL};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Whether AVFoundation can play `mime`, an extended MIME type (`video/mp4;
/// codecs="avc1.42E01E"`): `+[AVURLAsset isPlayableExtendedMIMEType:]`.
pub fn is_playable_extended_mime_type(mime: &str) -> bool {
    let mime = NSString::from_str(mime);
    unsafe { AVURLAsset::isPlayableExtendedMIMEType(&mime) }
}

/// A media resource's bytes, as the engine fetched them, with the response's
/// `Content-Type`. Immutable once made: step 2 fetches the whole resource first (ranged
/// fetches come with playback).
pub struct MediaBytes {
    pub content_type: String,
    pub data: Vec<u8>,
}

/// The Uniform Type Identifier AVFoundation wants for a response's `Content-Type` (its
/// essence, parameters ignored). Unknown types are offered as MPEG-4, which is what
/// AVFoundation would sniff most media as; a wrong guess fails as unplayable.
fn uti_for(content_type: &str) -> &'static str {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match essence.as_str() {
        "audio/mp4" | "audio/x-m4a" | "audio/aac" => "com.apple.m4a-audio",
        "audio/mpeg" | "audio/mp3" => "public.mp3",
        "audio/wav" | "audio/wave" | "audio/x-wav" => "com.microsoft.waveform-audio",
        "video/quicktime" => "com.apple.quicktime-movie",
        _ => "public.mpeg-4",
    }
}

struct LoaderIvars {
    bytes: Arc<MediaBytes>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; `RustKitMediaLoader` has no Drop.
    #[unsafe(super(NSObject))]
    #[name = "RustKitMediaLoader"]
    #[ivars = LoaderIvars]
    struct Loader;

    unsafe impl NSObjectProtocol for Loader {}

    unsafe impl AVAssetResourceLoaderDelegate for Loader {
        /// Every request is answered at once from the bytes in memory.
        #[unsafe(method(resourceLoader:shouldWaitForLoadingOfRequestedResource:))]
        fn should_wait(
            &self,
            _loader: &AVAssetResourceLoader,
            request: &AVAssetResourceLoadingRequest,
        ) -> bool {
            self.answer(request);
            true
        }
    }
);

impl Loader {
    fn new(bytes: Arc<MediaBytes>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(LoaderIvars { bytes });
        unsafe { msg_send![super(this), init] }
    }

    fn answer(&self, request: &AVAssetResourceLoadingRequest) {
        let bytes = &self.ivars().bytes;
        unsafe {
            if let Some(info) = request.contentInformationRequest() {
                info.setContentType(Some(&NSString::from_str(uti_for(&bytes.content_type))));
                info.setContentLength(bytes.data.len() as i64);
                info.setByteRangeAccessSupported(true);
            }
            if let Some(data) = request.dataRequest() {
                let len = bytes.data.len() as i64;
                let start = data.currentOffset().clamp(0, len);
                let end = if data.requestsAllDataToEndOfResource() {
                    len
                } else {
                    (data.requestedOffset() + data.requestedLength() as i64).clamp(start, len)
                };
                let chunk = NSData::with_bytes(&bytes.data[start as usize..end as usize]);
                data.respondWithData(&chunk);
            }
            request.finishLoading();
        }
    }
}

/// What a [`Probe`] knows.
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeState {
    /// The asset's values are still loading.
    Pending,
    /// The asset is playable. `video` is its first video track's display size; `None`
    /// for audio only.
    Ready {
        duration: f64,
        video: Option<(f64, f64)>,
        audio: bool,
    },
    /// Not media AVFoundation can play (or not media at all).
    Failed,
}

/// Each probe's URL is its own (`rustkit-media://probe/<n>`).
static NEXT_PROBE: AtomicU64 = AtomicU64::new(1);

/// One resource's metadata, loaded by AVFoundation from bytes we hold.
pub struct Probe {
    asset: Retained<AVURLAsset>,
    // Kept alive: the resource loader holds its delegate weakly.
    _loader: Retained<Loader>,
    _queue: DispatchRetained<DispatchQueue>,
    loaded: Arc<AtomicBool>,
    state: Option<ProbeState>,
}

impl Probe {
    /// Start loading `bytes`' metadata. Nothing blocks: [`Probe::poll`] reports it.
    pub fn start(bytes: Arc<MediaBytes>) -> Probe {
        let id = NEXT_PROBE.fetch_add(1, Ordering::Relaxed);
        // The scheme is ours alone, so AVFoundation cannot load it without the delegate.
        let url = NSURL::URLWithString(&NSString::from_str(&format!("rustkit-media://probe/{id}")))
            .expect("a valid URL");
        let asset = unsafe { AVURLAsset::URLAssetWithURL_options(&url, None) };
        let queue = DispatchQueue::new("org.hiwave.rustkit-media.loader", None);
        let loader = Loader::new(bytes);
        unsafe {
            asset
                .resourceLoader()
                .setDelegate_queue(Some(ProtocolObject::from_ref(&*loader)), Some(&queue));
        }
        let loaded = Arc::new(AtomicBool::new(false));
        let done = loaded.clone();
        let handler = block2::RcBlock::new(move || done.store(true, Ordering::Release));
        let keys = NSArray::from_retained_slice(&[
            NSString::from_str("playable"),
            NSString::from_str("duration"),
            NSString::from_str("tracks"),
        ]);
        unsafe { asset.loadValuesAsynchronouslyForKeys_completionHandler(&keys, Some(&handler)) };
        Probe {
            asset,
            _loader: loader,
            _queue: queue,
            loaded,
            state: None,
        }
    }

    /// The probe's state now. Once out of `Pending` it does not change.
    pub fn poll(&mut self) -> ProbeState {
        if let Some(state) = &self.state {
            return state.clone();
        }
        if !self.loaded.load(Ordering::Acquire) {
            return ProbeState::Pending;
        }
        // The keys are loaded (or failed): these reads do not block.
        #[allow(deprecated)]
        let state = unsafe {
            let duration = self.asset.duration().seconds();
            if !self.asset.isPlayable() || !duration.is_finite() {
                ProbeState::Failed
            } else {
                let track = |kind: Option<&'static objc2_av_foundation::AVMediaType>| {
                    kind.and_then(|kind| self.asset.tracksWithMediaType(kind).firstObject())
                };
                let video = track(AVMediaTypeVideo).map(|t| {
                    let size = t.naturalSize();
                    // A rotated track is displayed rotated (the preferred transform).
                    let tr = t.preferredTransform();
                    let w = (size.width * tr.a + size.height * tr.c).abs();
                    let h = (size.width * tr.b + size.height * tr.d).abs();
                    (w, h)
                });
                ProbeState::Ready {
                    duration,
                    video,
                    audio: track(AVMediaTypeAudio).is_some(),
                }
            }
        };
        self.state = Some(state.clone());
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The codecs of the MEDIA-AVF fixtures play; an unknown type does not.
    #[test]
    fn h264_and_aac_in_mp4_are_playable() {
        assert!(is_playable_extended_mime_type(
            "video/mp4; codecs=\"avc1.42E01E, mp4a.40.2\""
        ));
        assert!(is_playable_extended_mime_type(
            "audio/mp4; codecs=\"mp4a.40.2\""
        ));
        assert!(!is_playable_extended_mime_type("video/x-unknown"));
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../../fixtures/media/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("fixture")
    }

    /// Poll for up to 5 s of wall time: AVFoundation loads on its own threads.
    fn settle(probe: &mut Probe) -> ProbeState {
        for _ in 0..500 {
            match probe.poll() {
                ProbeState::Pending => std::thread::sleep(std::time::Duration::from_millis(10)),
                done => return done,
            }
        }
        ProbeState::Pending
    }

    /// Chrome 148 (media_load_cft148.json): duration 2, 64x48, and an audio track.
    #[test]
    fn a_probe_reads_the_fixtures_metadata_from_memory() {
        let mut probe = Probe::start(Arc::new(MediaBytes {
            content_type: "video/mp4".into(),
            data: fixture("color-steps.mp4"),
        }));
        match settle(&mut probe) {
            ProbeState::Ready {
                duration,
                video,
                audio,
            } => {
                assert!((duration - 2.0).abs() < 0.01, "duration {duration}");
                assert_eq!(video, Some((64.0, 48.0)));
                assert!(audio);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bytes_that_are_not_media_fail() {
        let mut probe = Probe::start(Arc::new(MediaBytes {
            content_type: "video/mp4".into(),
            data: b"this is not a media file ".repeat(40),
        }));
        assert_eq!(settle(&mut probe), ProbeState::Failed);
    }
}
