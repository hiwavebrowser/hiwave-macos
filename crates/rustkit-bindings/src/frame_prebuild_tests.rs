//! SPARE-COST (#1489): portable accept table for `__rustkit_frame_prebuild`.
//!
//! Engine integration pins cover "unread frame leaves the spare unused"; the
//! host gate itself (blank + no sandbox + not already prebuilt) had no unit
//! callers on default Linux `--lib`.

use super::{frame_prebuild_accepted, FrameRequest};

fn req(url: Option<&str>, sandbox: Option<&str>, prebuilt: bool) -> FrameRequest {
    FrameRequest {
        node: 1,
        url: url.map(str::to_string),
        sandbox: sandbox.map(str::to_string),
        initial: true,
        quiet: false,
        prebuilt,
    }
}

#[test]
fn frame_prebuild_accepts_only_pending_unsandboxed_blank() {
    assert!(frame_prebuild_accepted(&req(Some("about:blank"), None, false)));

    assert!(
        !frame_prebuild_accepted(&req(Some("https://example.test/"), None, false)),
        "a URL frame must attach at the drain, not via the spare"
    );
    assert!(
        !frame_prebuild_accepted(&req(None, None, false)),
        "a request without a URL is not a blank spare candidate"
    );
    assert!(
        !frame_prebuild_accepted(&req(Some("about:blank"), Some(""), false)),
        "a sandboxed blank must not take the spare"
    );
    assert!(
        !frame_prebuild_accepted(&req(Some("about:blank"), None, true)),
        "an already-prebuilt request must not be accepted again"
    );
}
