//! Element and document surface (web_dom_surface.js). Its own test module so
//! it does not collide with the other families' tests in `lib.rs`.

use super::*;

const PAGE: &str = r#"<!DOCTYPE html><html><head><title>T</title><script id="s1">1</script></head>
<body><div id="a" class="box" data-x="7" tabindex="3"><span id="b">x</span><a id="l" href="/p">link</a><a id="n" name="top">anchor</a></div>
<form id="f"><input id="i"></form><img id="im" src="x.png"><button id="bt">go</button><p id="p" contenteditable>e</p></body></html>"#;

fn bound() -> DomBindings {
    let bindings = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    bindings.set_document(Rc::new(Document::parse_html(PAGE).unwrap())).unwrap();
    bindings
}

fn ev(bindings: &DomBindings, script: &str) -> String {
    match bindings.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        JsValue::Boolean(b) => b.to_string(),
        JsValue::Number(n) => {
            if n.fract() == 0.0 { format!("{}", n as i64) } else { n.to_string() }
        }
        JsValue::Null => "null".to_string(),
        other => panic!("{script} evaluated to {other:?}"),
    }
}

/// Content-model members come from the real tree.
#[test]
fn element_content_model_members_are_computed_from_the_tree() {
    let b = bound();
    // attributes: a NamedNodeMap snapshot. The DOM keeps attributes in a map,
    // so order is not source order; the test sorts.
    assert_eq!(
        ev(&b, "var a = document.getElementById('a'); a.attributes.length + ':' + a.attributes.getNamedItem('data-x').value + ':' + a.attributes.getNamedItem('id').ownerElement.id + ':' + a.attributes.item(9) + ':' + a.attributes.getNamedItem('nope')"),
        "4:7:a:null:null"
    );
    assert_eq!(ev(&b, "var o = []; for (var at of a.attributes) o.push(at.name + '=' + at.value); o.sort().join()"), "class=box,data-x=7,id=a,tabindex=3");
    // previousElementSibling skips text, namespaceURI, shadowRoot/slot.
    assert_eq!(ev(&b, "document.getElementById('l').previousElementSibling.id"), "b");
    assert_eq!(ev(&b, "String(document.getElementById('b').previousElementSibling)"), "null");
    assert_eq!(ev(&b, "document.getElementById('a').namespaceURI"), "http://www.w3.org/1999/xhtml");
    assert_eq!(ev(&b, "String(a.shadowRoot) + ',' + JSON.stringify(a.slot)"), "null,\"\"");
    // tabIndex: attribute wins; links and controls are focusable; the rest -1.
    assert_eq!(
        ev(&b, "['a', 'l', 'n', 'bt', 'i', 'b'].map(function (id) { return document.getElementById(id).tabIndex; }).join()"),
        "3,0,-1,0,0,-1"
    );
    assert_eq!(ev(&b, "var pe = document.getElementById('p'); pe.isContentEditable + ':' + pe.contentEditable + ':' + a.isContentEditable"), "true:true:false");
}

/// Geometry is honest about being unavailable: zeros and empty lists, so a
/// page measuring an element takes its "nothing measurable" path.
#[test]
fn element_geometry_reports_no_layout_instead_of_throwing() {
    let b = bound();
    assert_eq!(
        ev(&b, "var e = document.getElementById('a'); [e.offsetWidth, e.offsetHeight, e.offsetTop, e.offsetLeft, e.clientWidth, e.clientHeight, e.scrollWidth, e.scrollHeight, e.scrollTop, e.scrollLeft].join()"),
        "0,0,0,0,0,0,0,0,0,0"
    );
    assert_eq!(ev(&b, "String(e.offsetParent)"), "null");
    assert_eq!(ev(&b, "var r = e.getBoundingClientRect(); [r.x, r.y, r.width, r.height, r.top, r.right, r.bottom, r.left].join()"), "0,0,0,0,0,0,0,0");
    assert_eq!(ev(&b, "JSON.stringify(e.getBoundingClientRect())"), r#"{"x":0,"y":0,"width":0,"height":0,"top":0,"right":0,"bottom":0,"left":0}"#);
    assert_eq!(ev(&b, "String(e.getClientRects().length)"), "0");
    // The no-op scrolling and capture methods exist; scrollTop is assignable.
    assert_eq!(ev(&b, "e.scrollIntoView(); e.scrollTo(0, 10); e.scrollBy(1, 1); e.scrollTop = 50; String(e.scrollTop)"), "0");
    // attachShadow and animate are deliberately absent (detectable).
    assert_eq!(ev(&b, "typeof e.attachShadow + ',' + typeof e.animate"), "undefined,undefined");
}

#[test]
fn document_members_read_the_page() {
    let b = bound();
    assert_eq!(ev(&b, "String(document.defaultView === window)"), "true");
    assert_eq!(ev(&b, "document.activeElement.tagName + ',' + document.scrollingElement.tagName"), "BODY,HTML");
    assert_eq!(
        ev(&b, "[document.compatMode, document.visibilityState, document.hidden, document.characterSet, document.contentType, document.designMode].join()"),
        "CSS1Compat,visible,false,UTF-8,text/html,off"
    );
    assert_eq!(ev(&b, "[document.scripts.length, document.forms.length, document.images.length, document.links.length, document.anchors.length].join()"), "1,1,1,1,1");
    assert_eq!(ev(&b, "document.scripts[0].id + ',' + document.forms[0].id + ',' + document.images[0].id + ',' + document.links[0].id"), "s1,f,im,l");
    assert_eq!(ev(&b, "String(document.styleSheets.length)"), "0");
    assert_eq!(ev(&b, "String(document.documentURI === location.href && document.baseURI === location.href)"), "true");
    assert_eq!(ev(&b, "[document.hasFocus(), String(document.elementFromPoint(1, 1)), document.elementsFromPoint(1, 1).length, document.execCommand('copy')].join()"), "true,null,0,false");
}

#[test]
fn document_create_event_import_node_and_create_element_ns() {
    let b = bound();
    // createEvent + initEvent, the legacy way pages dispatch synthetic events.
    assert_eq!(
        ev(&b, "var got = []; document.addEventListener('ping', function (e) { got.push(e.type + ':' + e.bubbles); }); \
                var evt = document.createEvent('Event'); evt.initEvent('ping', true, true); document.dispatchEvent(evt); got.join()"),
        "ping:true"
    );
    assert_eq!(ev(&b, "var n; try { document.createEvent('Bogus'); n = 'no throw'; } catch (e) { n = e.name; } n"), "NotSupportedError");
    // createElementNS ignores the namespace and uses the local name.
    assert_eq!(ev(&b, "document.createElementNS('http://www.w3.org/2000/svg', 'svg:g').tagName.toLowerCase()"), "g");
    // importNode clones; adoptNode returns the node.
    assert_eq!(ev(&b, "var src = document.getElementById('a'); var c = document.importNode(src, true); (c !== src) + ',' + c.id + ',' + c.children.length + ',' + (document.adoptNode(src) === src)"), "true,a,3,true");
}
