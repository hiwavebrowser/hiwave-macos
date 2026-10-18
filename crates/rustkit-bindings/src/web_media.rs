//! `window.matchMedia` (CSSOM View) over the cascade's media evaluator.
//!
//! The host function is the same `media_query_list_matches_with` the
//! stylesheet cascade uses, so a query script asks and a query a sheet
//! asks have one answer. `web_media.js` is the `MediaQueryList`.

use rustkit_js::{JsError, JsRuntime, JsValue};

fn number(args: &[JsValue], index: usize) -> Option<f32> {
    match args.get(index) {
        Some(JsValue::Number(n)) if n.is_finite() => Some(*n as f32),
        _ => None,
    }
}

pub(crate) fn install(runtime: &mut JsRuntime) -> Result<(), JsError> {
    runtime.register_host_function(
        "__rustkit_media_matches",
        4,
        Box::new(|args| {
            let Some(JsValue::String(query)) = args.first() else {
                return JsValue::Boolean(false);
            };
            let width = number(args, 1).unwrap_or(0.0);
            let height = number(args, 2).unwrap_or(0.0);
            let dpr = number(args, 3).unwrap_or(1.0);
            JsValue::Boolean(rustkit_css::media_query_list_matches_with(
                query, width, height, dpr,
            ))
        }),
    )?;
    runtime
        .evaluate_script(include_str!("web_media.js"))
        .map(|_| ())
}
