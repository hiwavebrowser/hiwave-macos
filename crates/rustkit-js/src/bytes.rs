//! Byte work the embedder's scripts hand to the host: response bodies,
//! UTF-8, base64 and binary strings.
//!
//! These are natives on the host object (`__rk`, see
//! [`JsRuntime::evaluate_with_host`](crate::JsRuntime::evaluate_with_host)),
//! so only the embedder's own scripts can call them. Each does in one call
//! what the scripts used to do with a loop turn per byte and a string built
//! by `+=` (copied on every step): a 256 KB response took 3 s to reach the
//! page and 1 MB did not arrive inside the script budget.
//!
//! - `takeBytes(key)`: the bytes [`JsRuntime::stash_bytes`](crate::JsRuntime::stash_bytes)
//!   left under `key`, as an `ArrayBuffer`, once; `undefined` after that.
//! - `utf8Decode(bytes, flush)`: `[text, bytesUsed, sawError]`. Malformed
//!   input becomes U+FFFD, one per maximal subpart (the Encoding Standard's
//!   and `std`'s rule). Without `flush`, an incomplete sequence at the end
//!   is left unused for the next call.
//! - `utf8Encode(string)`: a `Uint8Array`; a lone surrogate is U+FFFD.
//! - `binaryDecode(bytes)`: the string with one code unit per byte.
//! - `binaryEncode(string)`: its code units as bytes, or `null` when one is
//!   above 255.
//! - `base64Decode(string)`: a `Uint8Array`, or `null` when `string` is not
//!   base64 (standard alphabet, padding optional, trailing bits dropped).
//! - `base64Encode(bytes)`: padded standard base64.
//!
//! `bytes` is an `ArrayBuffer` or a view on one.

use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};
use base64::engine::DecodePaddingMode;
use base64::Engine as _;
use boa_engine::object::builtins::{JsArray, JsArrayBuffer, JsTypedArray, JsUint8Array};
use boa_engine::object::ObjectInitializer;
use boa_engine::string::{JsStr, JsStrVariant};
use boa_engine::{js_string, Context, JsNativeError, JsResult, JsString, JsValue, NativeFunction};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Bytes waiting for script to take them, by key.
pub(crate) type Stash = Rc<RefCell<HashMap<u64, Vec<u8>>>>;

const BASE64: GeneralPurpose = GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_padding_mode(DecodePaddingMode::Indifferent)
        .with_decode_allow_trailing_bits(true),
);

/// Standard base64 (padding optional) as bytes; `None` when it is not.
pub(crate) fn base64_decode(ascii: &[u8]) -> Option<Vec<u8>> {
    BASE64.decode(ascii).ok()
}

/// Run `f` over the bytes of `value`: an `ArrayBuffer`, or the part of one
/// a typed array or `DataView` covers.
fn with_bytes<T>(
    value: Option<&JsValue>,
    context: &mut Context,
    f: impl FnOnce(&[u8]) -> T,
) -> JsResult<T> {
    let not_bytes =
        || JsNativeError::typ().with_message("expected an ArrayBuffer or a view on one");
    let object = value.and_then(JsValue::as_object).ok_or_else(not_bytes)?;
    let (buffer, start, length) = match JsArrayBuffer::from_object(object.clone()) {
        Ok(buffer) => {
            let length = buffer.byte_length();
            (buffer, 0, length)
        }
        Err(_) => {
            let view = JsTypedArray::from_object(object.clone()).map_err(|_| not_bytes())?;
            let buffer = view.buffer(context)?;
            let buffer = buffer.as_object().ok_or_else(not_bytes)?;
            let buffer = JsArrayBuffer::from_object(buffer.clone()).map_err(|_| not_bytes())?;
            (
                buffer,
                view.byte_offset(context)?,
                view.byte_length(context)?,
            )
        }
    };
    let data = buffer
        .data()
        .ok_or_else(|| JsNativeError::typ().with_message("the ArrayBuffer is detached"))?;
    let end = start.saturating_add(length).min(data.len());
    Ok(f(&data[start.min(end)..end]))
}

fn array_buffer(bytes: &[u8], context: &mut Context) -> JsResult<JsArrayBuffer> {
    let buffer = JsArrayBuffer::new(bytes.len(), context)?;
    if let Some(mut data) = buffer.data_mut() {
        data.copy_from_slice(bytes);
    }
    Ok(buffer)
}

fn uint8_array(bytes: &[u8], context: &mut Context) -> JsResult<JsValue> {
    let buffer = array_buffer(bytes, context)?;
    Ok(JsUint8Array::from_array_buffer(buffer, context)?.into())
}

/// Lossy UTF-8: `(text, bytes used, whether anything was malformed)`.
pub(crate) fn utf8_decode(bytes: &[u8], flush: bool) -> (String, usize, bool) {
    let mut text = String::with_capacity(bytes.len());
    let (mut at, mut bad) = (0, false);
    while at < bytes.len() {
        match std::str::from_utf8(&bytes[at..]) {
            Ok(rest) => {
                text.push_str(rest);
                at = bytes.len();
            }
            Err(error) => {
                let valid = error.valid_up_to();
                // The prefix is valid by `valid_up_to`'s contract.
                text.push_str(std::str::from_utf8(&bytes[at..at + valid]).unwrap_or_default());
                at += valid;
                match error.error_len() {
                    Some(len) => {
                        text.push('\u{FFFD}');
                        bad = true;
                        at += len;
                    }
                    // An incomplete sequence at the end of the input.
                    None if flush => {
                        text.push('\u{FFFD}');
                        bad = true;
                        at = bytes.len();
                    }
                    None => break,
                }
            }
        }
    }
    (text, at, bad)
}

/// UTF-8 of a string's code units; a lone surrogate is U+FFFD.
pub(crate) fn utf8_encode(string: JsStr<'_>) -> Vec<u8> {
    match string.variant() {
        JsStrVariant::Latin1(units) if units.is_ascii() => units.to_vec(),
        JsStrVariant::Latin1(units) => units
            .iter()
            .map(|&u| u as char)
            .collect::<String>()
            .into_bytes(),
        JsStrVariant::Utf16(units) => char::decode_utf16(units.iter().copied())
            .map(|c| c.unwrap_or('\u{FFFD}'))
            .collect::<String>()
            .into_bytes(),
    }
}

fn string_arg(args: &[JsValue]) -> JsResult<JsString> {
    args.first().and_then(JsValue::as_string).ok_or_else(|| {
        JsNativeError::typ()
            .with_message("expected a string")
            .into()
    })
}

/// Add the byte natives to the host object under construction.
pub(crate) fn install(host: &mut ObjectInitializer<'_>, stash: &Stash) {
    let taken = stash.clone();
    // SAFETY: the closure holds an `Rc` of plain bytes: nothing the
    // collector would need to trace.
    let take_bytes = unsafe {
        NativeFunction::from_closure(move |_this, args, context| {
            let key = args.first().and_then(JsValue::as_number).unwrap_or(-1.0);
            if key < 0.0 || key.fract() != 0.0 {
                return Ok(JsValue::undefined());
            }
            let bytes = taken.borrow_mut().remove(&(key as u64));
            match bytes {
                Some(bytes) => Ok(array_buffer(&bytes, context)?.into()),
                None => Ok(JsValue::undefined()),
            }
        })
    };
    let decode = NativeFunction::from_fn_ptr(|_this, args, context| {
        let flush = args.get(1).is_some_and(JsValue::to_boolean);
        let (text, used, bad) =
            with_bytes(args.first(), context, |bytes| utf8_decode(bytes, flush))?;
        let parts = [
            JsValue::from(JsString::from(text.as_str())),
            JsValue::from(used as f64),
            JsValue::from(bad),
        ];
        Ok(JsArray::from_iter(parts, context).into())
    });
    let encode = NativeFunction::from_fn_ptr(|_this, args, context| {
        let bytes = utf8_encode(string_arg(args)?.as_str());
        uint8_array(&bytes, context)
    });
    let binary_decode = NativeFunction::from_fn_ptr(|_this, args, context| {
        let text = with_bytes(args.first(), context, |bytes| {
            JsString::from(JsStr::latin1(bytes))
        })?;
        Ok(text.into())
    });
    let binary_encode = NativeFunction::from_fn_ptr(|_this, args, context| {
        let string = string_arg(args)?;
        let bytes: Option<Vec<u8>> = match string.as_str().variant() {
            JsStrVariant::Latin1(units) => Some(units.to_vec()),
            JsStrVariant::Utf16(units) => units.iter().map(|&u| u8::try_from(u).ok()).collect(),
        };
        match bytes {
            Some(bytes) => uint8_array(&bytes, context),
            None => Ok(JsValue::null()),
        }
    });
    let base64_decode = NativeFunction::from_fn_ptr(|_this, args, context| {
        let string = string_arg(args)?;
        let ascii: Option<Vec<u8>> = match string.as_str().variant() {
            JsStrVariant::Latin1(units) => Some(units.to_vec()),
            JsStrVariant::Utf16(units) => units.iter().map(|&u| u8::try_from(u).ok()).collect(),
        };
        match ascii.and_then(|ascii| base64_decode(&ascii)) {
            Some(bytes) => uint8_array(&bytes, context),
            None => Ok(JsValue::null()),
        }
    });
    let base64_encode = NativeFunction::from_fn_ptr(|_this, args, context| {
        let text = with_bytes(args.first(), context, |bytes| BASE64.encode(bytes))?;
        Ok(JsString::from(JsStr::latin1(text.as_bytes())).into())
    });
    host.function(take_bytes, js_string!("takeBytes"), 1)
        .function(decode, js_string!("utf8Decode"), 2)
        .function(encode, js_string!("utf8Encode"), 1)
        .function(binary_decode, js_string!("binaryDecode"), 1)
        .function(binary_encode, js_string!("binaryEncode"), 1)
        .function(base64_decode, js_string!("base64Decode"), 1)
        .function(base64_encode, js_string!("base64Encode"), 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_utf8_is_one_replacement_per_maximal_subpart() {
        // F0 9F 98 is a truncated four-byte sequence, then 'a'; C0 can
        // start nothing; E2 82 at the end is incomplete.
        assert_eq!(
            utf8_decode(b"\xF0\x9F\x98a", true),
            ("\u{FFFD}a".to_string(), 4, true)
        );
        assert_eq!(
            utf8_decode(b"\xC0\x80", true),
            ("\u{FFFD}\u{FFFD}".to_string(), 2, true)
        );
        assert_eq!(
            utf8_decode(b"ok\xE2\x82", true),
            ("ok\u{FFFD}".to_string(), 4, true)
        );
        assert_eq!(
            utf8_decode("é€😀".as_bytes(), true),
            ("é€😀".to_string(), 9, false)
        );
    }

    #[test]
    fn an_incomplete_tail_waits_for_the_next_chunk_when_streaming() {
        assert_eq!(
            utf8_decode(b"ok\xE2\x82", false),
            ("ok".to_string(), 2, false)
        );
        assert_eq!(
            utf8_decode(b"\xE2\x82\xAC", false),
            ("€".to_string(), 3, false)
        );
    }

    #[test]
    fn a_lone_surrogate_encodes_as_the_replacement_character() {
        let units = [0x61, 0xD800, 0x62];
        assert_eq!(utf8_encode(JsStr::utf16(&units)), b"a\xEF\xBF\xBDb");
        assert_eq!(utf8_encode(JsStr::latin1(b"\xE9")), "é".as_bytes());
    }
}
