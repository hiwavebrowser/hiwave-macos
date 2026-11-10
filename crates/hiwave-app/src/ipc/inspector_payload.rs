//! Inspector data posted by a content page (WebKit content path).
//!
//! On the WebKit content path the inspector's console lines, DOM tree and
//! element styles are posted by scripts the app injects into the page
//! (`console_inject.js`, `inspector_inject.js`) over the page's own
//! `window.ipc`. The page can post the same commands itself, with any
//! field set to any type, and the app inserts the payload into the
//! inspector view's script. Each payload is read here into the shape the
//! injected scripts send; one that does not fit is dropped, not forwarded.
//! (The RustKit content path builds these in Rust, typed, and does not
//! come through here.)

// Only the WebKit content path (not built on macOS) calls these.
#![cfg_attr(target_os = "macos", allow(dead_code))]

use serde_json::Value;

/// A console line, as `console_inject.js` sends it. `None`: drop it.
pub fn console_entry(entry: &Value) -> Option<Value> {
    Some(entry.clone())
}

/// The DOM tree, as `inspector_inject.js` sends it (`null` when the page
/// has no document element). `None`: drop it.
pub fn dom_tree(tree: &Value) -> Option<Value> {
    Some(tree.clone())
}

/// An element's styles, as `inspector_inject.js` sends them (`null` when
/// the path names no element). `None`: drop it.
pub fn element_styles(styles: &Value) -> Option<Value> {
    Some(styles.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn genuine_entry() -> Value {
        json!({
            "id": 3,
            "level": "log",
            "message": "hello",
            "timestamp": 1_731_000_000_000u64,
            "stack": [
                {"fn": "<anonymous>", "file": "https://example.com/a.js", "line": 4, "col": 9},
                {"raw": "at somewhere"}
            ]
        })
    }

    #[test]
    fn a_console_line_as_the_inject_script_sends_it_is_kept() {
        let kept = console_entry(&genuine_entry()).expect("kept");
        assert_eq!(kept["id"], 3);
        assert_eq!(kept["level"], "log");
        assert_eq!(kept["message"], "hello");
        assert_eq!(kept["stack"][0]["line"], 4);
        assert_eq!(kept["stack"][1]["raw"], "at somewhere");
        // A numeric document id is kept too.
        let mut with_doc = genuine_entry();
        with_doc["doc"] = json!(2);
        assert_eq!(console_entry(&with_doc).expect("kept")["doc"], 2);
    }

    #[test]
    fn a_console_line_with_a_field_of_the_wrong_type_is_dropped() {
        let forged: [(&str, Value); 6] = [
            ("doc", json!("2<b>")),
            ("id", json!("x\"y")),
            ("message", json!({"a": 1})),
            ("timestamp", json!("soon")),
            ("level", json!("log\" x=\"")),
            ("stack", json!([{"fn": "f", "file": "a.js", "line": "4<", "col": 1}])),
        ];
        for (field, value) in forged {
            let mut entry = genuine_entry();
            entry[field] = value;
            assert_eq!(console_entry(&entry), None, "a forged {field} is dropped");
        }
        assert_eq!(console_entry(&json!("a string")), None);
    }

    #[test]
    fn extra_fields_on_a_console_line_are_not_forwarded() {
        let mut entry = genuine_entry();
        entry["extra"] = json!("<b>");
        let kept = console_entry(&entry).expect("kept");
        assert!(kept.get("extra").is_none(), "only the known fields cross: {kept}");
    }

    fn genuine_tree() -> Value {
        json!({
            "type": "element", "path": "html", "tag": "html", "id": null, "classes": [],
            "children": [
                {"type": "element", "path": "html > body", "tag": "body", "id": "main",
                 "classes": ["a", "b"], "attributes": {"lang": "en"},
                 "children": [
                     {"type": "text", "textContent": "hi", "path": ""},
                     {"type": "comment", "textContent": "c", "path": ""}
                 ]}
            ]
        })
    }

    #[test]
    fn a_dom_tree_as_the_inject_script_sends_it_is_kept() {
        let kept = dom_tree(&genuine_tree()).expect("kept");
        let body = &kept["children"][0];
        assert_eq!(body["tag"], "body");
        assert_eq!(body["id"], "main");
        assert_eq!(body["attributes"]["lang"], "en");
        assert_eq!(body["children"][0]["textContent"], "hi");
        assert_eq!(body["children"][1]["type"], "comment");
        assert_eq!(dom_tree(&Value::Null), Some(Value::Null), "no document element");
    }

    #[test]
    fn a_dom_tree_with_a_field_of_the_wrong_type_is_dropped() {
        let mut omitted = genuine_tree();
        omitted["children"][0]["childrenOmitted"] = json!("3<b>");
        assert_eq!(dom_tree(&omitted), None, "a string childrenOmitted");
        let mut tag = genuine_tree();
        tag["children"][0]["tag"] = json!(["div"]);
        assert_eq!(dom_tree(&tag), None, "a non-string tag");
        let mut kind = genuine_tree();
        kind["children"][0]["type"] = json!("element\"");
        assert_eq!(dom_tree(&kind), None, "an unknown node type");
        let mut attr = genuine_tree();
        attr["children"][0]["attributes"]["lang"] = json!({"x": 1});
        assert_eq!(dom_tree(&attr), None, "a non-string attribute value");
        // A numeric childrenOmitted is kept.
        let mut fine = genuine_tree();
        fine["children"][0]["childrenOmitted"] = json!(3);
        assert_eq!(dom_tree(&fine).expect("kept")["children"][0]["childrenOmitted"], 3);
    }

    fn genuine_styles() -> Value {
        json!({
            "selector": "html > body > div",
            "tagName": "div",
            "id": null,
            "classes": ["x"],
            "rect": {"top": 1.5, "left": 0, "width": 100, "height": 20.25},
            "boxModel": {"width": 100, "height": 20.25, "marginTop": 0, "paddingLeft": 4},
            "computedStyles": {"display": "block", "color": "rgb(1, 2, 3)"}
        })
    }

    #[test]
    fn element_styles_as_the_inject_script_sends_them_are_kept() {
        let kept = element_styles(&genuine_styles()).expect("kept");
        assert_eq!(kept["selector"], "html > body > div");
        assert_eq!(kept["rect"]["height"], 20.25);
        assert_eq!(kept["boxModel"]["paddingLeft"], 4.0);
        assert_eq!(kept["computedStyles"]["color"], "rgb(1, 2, 3)");
        assert_eq!(element_styles(&Value::Null), Some(Value::Null), "no element");
    }

    #[test]
    fn element_styles_with_a_field_of_the_wrong_type_are_dropped() {
        let mut style = genuine_styles();
        style["computedStyles"]["color"] = json!(["red"]);
        assert_eq!(element_styles(&style), None, "a non-string style value");
        let mut rect = genuine_styles();
        rect["rect"]["width"] = json!("100");
        assert_eq!(element_styles(&rect), None, "a string rect width");
        let mut extra = genuine_styles();
        extra["detailsText"] = json!("from the page");
        let kept = element_styles(&extra).expect("kept");
        assert!(kept.get("detailsText").is_none(), "RustKit-only fields do not cross: {kept}");
    }
}
