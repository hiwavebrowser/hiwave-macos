//! Custom elements (web_components.js). The page is a real parsed document;
//! the selector matcher stands in for the engine's (tag-name lists only).

use super::*;

fn bound(html: &str) -> DomBindings {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(Document::parse_html(html).unwrap())).unwrap();
    b.set_selector_matcher(Rc::new(|node, selector, _| {
        Some(selector.split(',').any(|s| node.tag_name() == Some(s.trim())))
    }));
    b.evaluate("var log = [];").unwrap();
    b
}

fn ev(b: &DomBindings, script: &str) -> String {
    match b.evaluate(script).unwrap() {
        JsValue::String(s) => s,
        JsValue::Boolean(x) => x.to_string(),
        JsValue::Number(n) => format!("{n}"),
        JsValue::Undefined => "undefined".into(),
        JsValue::Null => "null".into(),
        other => format!("{other:?}"),
    }
}

fn thrown(b: &DomBindings, script: &str) -> String {
    ev(b, &format!("(function () {{ try {{ {script} }} catch (e) {{ return e.name; }} return 'no error'; }})()"))
}

const PAGE: &str = "<html><body><div id=host><x-a id=a1 color=red></x-a><p>text</p><x-a id=a2></x-a></div></body></html>";

#[test]
fn the_registry_validates_names_and_constructors() {
    let b = bound(PAGE);
    b.evaluate("class X extends HTMLElement {}").unwrap();
    for bad in ["foo", "Foo-bar", "font-face", "a", "1-x", "x-A", ""] {
        assert_eq!(thrown(&b, &format!("customElements.define('{bad}', class extends HTMLElement {{}});")), "SyntaxError", "{bad}");
    }
    assert_eq!(thrown(&b, "customElements.define('x-ok', {});"), "TypeError");
    assert_eq!(thrown(&b, "customElements.define('x-ok', X); customElements.define('x-ok', class extends HTMLElement {});"), "NotSupportedError");
    assert_eq!(thrown(&b, "customElements.define('x-two', X);"), "NotSupportedError", "one constructor, one name");
    assert_eq!(thrown(&b, "customElements.define('x-ext', class extends HTMLElement {}, { extends: 'div' });"), "NotSupportedError");
    assert_eq!(
        ev(&b, "[customElements.get('x-ok') === X, String(customElements.get('nope')), customElements.getName(X), String(customElements.getName(Object)), Object.prototype.toString.call(customElements)].join()"),
        "true,undefined,x-ok,null,[object CustomElementRegistry]"
    );
    assert_eq!(thrown(&b, "new CustomElementRegistry();"), "TypeError");
}

#[test]
fn whenDefined_resolves_when_the_name_is_defined() {
    let b = bound(PAGE);
    b.evaluate(
        "class W extends HTMLElement {} \
         customElements.whenDefined('x-late').then(function (c) { log.push('late:' + (c === W)); }); \
         customElements.whenDefined('bad').catch(function (e) { log.push('bad:' + e.name); }); \
         log.push('before');",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "before,bad:SyntaxError");
    b.evaluate("customElements.define('x-late', W);").unwrap();
    assert_eq!(ev(&b, "log.join()"), "before,bad:SyntaxError,late:true");
    b.evaluate("var done = 'no'; customElements.whenDefined('x-late').then(function () { done = 'again'; });").unwrap();
    assert_eq!(ev(&b, "done"), "again");
}

#[test]
fn defining_upgrades_the_elements_already_in_the_document() {
    let b = bound(PAGE);
    b.evaluate(
        "class XA extends HTMLElement { \
             static get observedAttributes() { return ['color']; } \
             constructor() { super(); log.push('ctor:' + this.id + ':' + (this === document.getElementById(this.id)) + ':' + this.isConnected); } \
             connectedCallback() { log.push('connected:' + this.id); } \
             attributeChangedCallback(n, o, v) { log.push('attr:' + this.id + ':' + n + ':' + o + ':' + v); } \
             hello() { return 'hi ' + this.id; } \
         } \
         var before = document.getElementById('a1'); \
         log.push('pre:' + (typeof before.hello)); \
         customElements.define('x-a', XA);",
    )
    .unwrap();
    assert_eq!(
        ev(&b, "log.join()"),
        "pre:undefined,ctor:a1:true:true,attr:a1:color:null:red,connected:a1,ctor:a2:true:true,connected:a2"
    );
    assert_eq!(
        ev(&b, "var a1 = document.getElementById('a1'); [a1 === before, a1 instanceof XA, a1 instanceof HTMLElement, a1.hello(), a1.constructor === XA].join()"),
        "true,true,true,hi a1,true"
    );
    assert_eq!(
        ev(&b, "var para = document.querySelector('p'); (para instanceof HTMLElement) + ':' + (para instanceof XA)"),
        "true:false"
    );
}

#[test]
fn new_and_createElement_make_a_defined_element_that_connects_on_insertion() {
    let b = bound(PAGE);
    b.evaluate(
        "class XB extends HTMLElement { constructor() { super(); log.push('ctor'); } \
             connectedCallback() { log.push('connected'); } disconnectedCallback() { log.push('disconnected'); } } \
         customElements.define('x-b', XB); log.length = 0; \
         var n = new XB(); var c = document.createElement('x-b'); \
         log.push([n.localName, n instanceof XB, c instanceof XB, n.isConnected, n.tagName].join('/'));",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "ctor,ctor,x-b/true/true/false/X-B");
    b.evaluate("log.length = 0; var host = document.getElementById('host'); host.appendChild(n); log.push('inserted');").unwrap();
    assert_eq!(ev(&b, "log.join()"), "connected,inserted");
    // Moving a connected element: disconnected, then connected at the new place.
    b.evaluate("log.length = 0; document.body.appendChild(n);").unwrap();
    assert_eq!(ev(&b, "log.join()"), "disconnected,connected");
    // An element made before its definition upgrades when it is inserted.
    b.evaluate(
        "log.length = 0; var early = document.createElement('x-c'); log.push(typeof early.later); \
         class XC extends HTMLElement { connectedCallback() { log.push('xc connected'); } later() {} } \
         customElements.define('x-c', XC); log.push(typeof early.later); document.body.appendChild(early);",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "undefined,undefined,xc connected");
    assert_eq!(ev(&b, "typeof early.later + ':' + (early instanceof XC)"), "function:true");
}

#[test]
fn disconnected_fires_on_every_way_out_of_the_document() {
    let b = bound("<html><body><div id=host></div></body></html>");
    b.evaluate(
        "class XD extends HTMLElement { constructor() { super(); } \
             connectedCallback() { log.push('+' + this.id); } disconnectedCallback() { log.push('-' + this.id); } } \
         customElements.define('x-d', XD); \
         var host = document.getElementById('host'); \
         function make(id) { var e = new XD(); e.id = id; return e; } \
         host.appendChild(make('a')); host.appendChild(make('b')); host.appendChild(make('c')); host.appendChild(make('d')); host.appendChild(make('e')); \
         var detached = document.createElement('div'); detached.appendChild(make('never'));",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "+a,+b,+c,+d,+e");
    b.evaluate("log.length = 0; host.removeChild(document.getElementById('a'));").unwrap();
    b.evaluate("document.getElementById('b').remove();").unwrap();
    b.evaluate("document.getElementById('c').replaceWith(document.createElement('span'));").unwrap();
    assert_eq!(ev(&b, "log.join()"), "-a,-b,-c");
    b.evaluate("log.length = 0; host.innerHTML = '';").unwrap();
    assert_eq!(ev(&b, "log.join()"), "-d,-e");
    // A subtree that contains one leaves with it.
    b.evaluate("log.length = 0; var wrap = document.createElement('section'); host.appendChild(wrap); wrap.appendChild(make('deep')); host.removeChild(wrap);").unwrap();
    assert_eq!(ev(&b, "log.join()"), "+deep,-deep");
    b.evaluate("log.length = 0; host.appendChild(make('t')); host.textContent = 'gone';").unwrap();
    assert_eq!(ev(&b, "log.join()"), "+t,-t");
    b.evaluate("log.length = 0; host.appendChild(make('r')); host.replaceChildren();").unwrap();
    assert_eq!(ev(&b, "log.join()"), "+r,-r");
}

#[test]
fn html_insertion_creates_and_connects_defined_elements() {
    let b = bound("<html><body><div id=host></div></body></html>");
    b.evaluate(
        "class XE extends HTMLElement { connectedCallback() { log.push('+' + this.id + ':' + (this instanceof XE)); } } \
         customElements.define('x-e', XE); var host = document.getElementById('host'); \
         host.innerHTML = '<x-e id=one></x-e><b>b</b><x-e id=two></x-e>'; \
         host.insertAdjacentHTML('beforeend', '<x-e id=three></x-e>'); \
         host.insertAdjacentHTML('afterend', '<x-e id=four></x-e>'); \
         var frag = document.createDocumentFragment(); var f = document.createElement('x-e'); f.id = 'five'; frag.appendChild(f); host.append(frag);",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "+one:true,+two:true,+three:true,+four:true,+five:true");
}

#[test]
fn attributes_report_only_the_observed_ones() {
    let b = bound(PAGE);
    b.evaluate(
        "class XF extends HTMLElement { static get observedAttributes() { return ['a', 'b']; } \
             attributeChangedCallback(n, o, v) { log.push(n + ':' + o + '>' + v); } } \
         customElements.define('x-f', XF); var e = new XF(); \
         e.setAttribute('a', '1'); e.setAttribute('a', '2'); e.setAttribute('c', 'ignored'); e.setAttribute('b', 'x'); \
         e.removeAttribute('a'); e.removeAttribute('a'); e.removeAttribute('c'); e.toggleAttribute('b'); e.toggleAttribute('b');",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "a:null>1,a:1>2,b:null>x,a:2>null,b:x>null,b:null>");
}

#[test]
fn a_throwing_or_wrong_constructor_is_reported_and_isolated() {
    let b = bound("<html><body><x-g id=bad></x-g><x-g id=good></x-g></body></html>");
    b.evaluate(
        "var n = 0; class XG extends HTMLElement { constructor() { super(); if (n++ === 0) throw new Error('ctor boom'); log.push('ok:' + this.id); } connectedCallback() { throw new Error('cb boom'); } } \
         customElements.define('x-g', XG);",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "ok:good");
    let errors = b.take_reported_errors();
    assert!(errors.iter().any(|e| e.contains("ctor boom")), "{errors:?}");
    assert!(errors.iter().any(|e| e.contains("cb boom")), "a throwing callback is reported and does not stop others: {errors:?}");
    // A constructor that returns something else cannot take over the element.
    let b = bound("<html><body><x-h id=h></x-h></body></html>");
    b.evaluate("class XH extends HTMLElement { constructor() { super(); return document.createElement('div'); } } customElements.define('x-h', XH);").unwrap();
    assert!(b.take_reported_errors().iter().any(|e| e.contains("InvalidStateError") || e.contains("did not produce")));
}

#[test]
fn html_element_is_only_constructible_through_a_definition() {
    let b = bound(PAGE);
    assert_eq!(thrown(&b, "new HTMLElement();"), "TypeError");
    assert_eq!(thrown(&b, "class Loose extends HTMLElement {} new Loose();"), "TypeError");
    assert_eq!(thrown(&b, "HTMLElement();"), "TypeError");
    assert_eq!(
        ev(&b, "[typeof HTMLElement, HTMLElement.name, document.createElement('div') instanceof HTMLElement, HTMLDivElement.prototype instanceof HTMLElement, document.createElement('div').constructor === HTMLDivElement].join()"),
        "function,HTMLElement,true,true,true"
    );
}

#[test]
fn nothing_changes_until_the_first_define() {
    let b = bound(PAGE);
    assert_eq!(
        ev(&b, "var h = document.getElementById('host'); var d = document.createElement('i'); h.appendChild(d); h.removeChild(d); d.setAttribute('k', 'v'); [h.children.length, d.getAttribute('k'), d.isConnected].join()"),
        "3,v,false"
    );
}

#[test]
fn upgrade_converts_a_detached_subtree_on_demand() {
    let b = bound(PAGE);
    b.evaluate(
        "class XU extends HTMLElement { constructor() { super(); log.push('ctor'); } } \
         var holder = document.createElement('div'); holder.innerHTML = '<x-u></x-u>'; \
         customElements.define('x-u', XU); log.push('defined'); \
         customElements.upgrade(holder); log.push((holder.firstChild instanceof XU) + ''); customElements.upgrade(holder);",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "defined,ctor,true");
}

/// Lit renders a parent by cloning a `<template>` (`importNode` of its
/// content), upgrading the child custom elements in that detached copy,
/// committing property parts (`.videoElement=${...}`), and only then
/// inserting the fragment. The child's `connectedCallback` (Lit's
/// `hostConnected`) reads the property. The constructor mirrors Lit's
/// pre-upgrade save: an own property assigned before the accessor exists
/// is deleted and would not be visible again until a later update.
#[test]
fn lit_property_binding_is_visible_when_the_child_connects() {
    let b = bound("<html><body></body></html>");
    b.evaluate(
        "class Child extends HTMLElement { \
             static get observedAttributes() { return ['tone']; } \
             constructor() { \
                 super(); \
                 if (Object.prototype.hasOwnProperty.call(this, 'videoElement')) { \
                     this._saved = this.videoElement; delete this.videoElement; \
                 } \
                 log.push('ctor:' + this.isConnected + ':' + (this.videoElement && this.videoElement._mark || 'none')); \
             } \
             attributeChangedCallback(n, o, v) { log.push('attr:' + n + ':' + o + ':' + v + ':' + this.isConnected); } \
             connectedCallback() { log.push('connected:' + (this.videoElement && this.videoElement._mark || 'none') + ':' + this.getAttribute('tone') + ':' + this.isConnected); } \
         } \
         Object.defineProperty(Child.prototype, 'videoElement', { \
             configurable: true, enumerable: true, \
             get: function () { return this._ve; }, \
             set: function (v) { this._ve = v; } \
         }); \
         customElements.define('x-child', Child); \
         var video = document.createElement('video'); video._mark = 'vid'; \
         var t = document.createElement('template'); \
         t.innerHTML = '<x-child tone=\"live\"></x-child>'; \
         log.push('template:' + (t.content.firstChild instanceof Child) + ':' + t.content.firstChild.isConnected); \
         var frag = document.importNode(t.content, true); \
         var child = frag.querySelector('x-child'); \
         log.push('cloned:' + (child instanceof Child) + ':' + child.isConnected); \
         child.videoElement = video; \
         document.body.appendChild(frag);",
    )
    .unwrap();
    assert_eq!(
        ev(&b, "log.join('|')"),
        "template:false:false|ctor:false:none|attr:tone:null:live:false|cloned:true:false|connected:vid:live:true",
        "a property set on the upgraded clone is what connectedCallback reads"
    );
}

/// Template contents stay in the inert document. They are not upgraded
/// there, nor by cloning the template or its content. importNode adopts
/// into this document and upgrades the copy while it is still
/// disconnected. Moving a content node into the page upgrades it.
/// Cloning an ordinary detached element upgrades that copy too.
#[test]
fn template_contents_stay_unupgraded_until_adopted() {
    let b = bound("<html><body></body></html>");
    b.evaluate(
        "class Child extends HTMLElement {} \
         customElements.define('x-child', Child); \
         var t = document.createElement('template'); \
         t.innerHTML = '<x-child id=\"c\"></x-child>'; \
         var inner = t.content.firstChild; \
         var clonedTpl = t.cloneNode(true); \
         var clonedContent = t.content.cloneNode(true); \
         var imported = document.importNode(t.content, true); \
         var importedTpl = document.importNode(t, true); \
         var loose = document.createElement('div'); \
         loose.innerHTML = '<x-child id=\"l\"></x-child>'; \
         var looseCopy = loose.cloneNode(true); \
         var el = document.createElement('x-child'); \
         var elCopy = el.cloneNode(true); \
         var made = t.content.ownerDocument.createElement('x-child'); \
         log.push([ \
             'content:' + (inner instanceof Child) + ':' + (inner.ownerDocument === document) + ':' + (inner.ownerDocument.defaultView === null), \
             'cloneTpl:' + (clonedTpl.content.firstChild instanceof Child) + ':' + (clonedTpl.content.firstChild.ownerDocument === inner.ownerDocument), \
             'cloneContent:' + (clonedContent.firstChild instanceof Child) + ':' + (clonedContent.firstChild.ownerDocument === inner.ownerDocument), \
             'import:' + (imported.firstChild instanceof Child) + ':' + imported.firstChild.isConnected + ':' + (imported.firstChild.ownerDocument === document), \
             'importTpl:' + (importedTpl.content.firstChild instanceof Child) + ':' + (importedTpl.content.ownerDocument !== document), \
             'loose:' + (loose.firstChild instanceof Child) + ':' + loose.firstChild.isConnected, \
             'looseCopy:' + (looseCopy.firstChild instanceof Child) + ':' + looseCopy.firstChild.isConnected, \
             'elCopy:' + (elCopy instanceof Child) + ':' + elCopy.isConnected, \
             'inertCreate:' + (made instanceof Child) + ':' + (made.ownerDocument === inner.ownerDocument) \
         ].join('|')); \
         document.body.appendChild(inner); \
         log.push('moved:' + (inner instanceof Child) + ':' + inner.isConnected + ':' + (inner.ownerDocument === document));",
    )
    .unwrap();
    assert_eq!(
        ev(&b, "log.join('|')"),
        "content:false:false:true|cloneTpl:false:true|cloneContent:false:true|import:true:false:true|importTpl:false:true|loose:true:false|looseCopy:true:false|elCopy:true:false|inertCreate:false:true|moved:true:true:true"
    );
}

/// `:defined` is answered from the DOM: each define records its name there,
/// and only names that were defined.
#[test]
fn define_tells_the_dom_which_names_are_defined() {
    let doc = Rc::new(Document::parse_html(PAGE).unwrap());
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(doc.clone()).unwrap();
    assert!(doc.defined_custom_elements().is_empty());
    b.evaluate("customElements.define('x-a', class extends HTMLElement {});").unwrap();
    assert_eq!(thrown(&b, "customElements.define('Bad', class extends HTMLElement {});"), "SyntaxError");
    let names = doc.defined_custom_elements();
    assert!(names.contains("x-a"), "{names:?}");
    assert_eq!(names.len(), 1, "{names:?}");
}

/// HTML §4.13.4 define: the upgrade candidates are the document's
/// shadow-including descendants, so an element of the name inside an open
/// or a closed shadow root upgrades too, in shadow-including tree order
/// (a host's shadow tree before its children). Chromium 141 logs
/// `d1,s1,s2,d2,s3,d3` for this script.
#[test]
fn define_upgrades_elements_inside_shadow_trees() {
    let b = bound("<html><body><x-in id=d1></x-in><div id=open><x-in id=d2></x-in></div><div id=closed></div><x-in id=d3></x-in></body></html>");
    b.evaluate(
        "document.getElementById('open').attachShadow({ mode: 'open' }).innerHTML = '<x-in id=s1></x-in><p><x-in id=s2></x-in></p>'; \
         var shut = document.getElementById('closed').attachShadow({ mode: 'closed' }); \
         shut.innerHTML = '<x-in id=s3></x-in>'; \
         customElements.define('x-in', class extends HTMLElement { constructor() { super(); log.push(this.id); } });",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "d1,s1,s2,d2,s3,d3");
    assert_eq!(
        ev(&b, "String(document.getElementById('open').shadowRoot.firstChild instanceof customElements.get('x-in'))"),
        "true"
    );
}

/// Each `[CEReactions]` family invokes its element queue before returning.
/// A marker pushed on the next line sits after the callbacks, and a
/// multi-node call lets the first callback see every node the call inserted.

fn host_page() -> DomBindings {
    bound("<html><body><div id=host></div></body></html>")
}

#[test]
fn reactions_run_on_exit_of_insertion_methods() {
    let b = host_page();
    b.evaluate(
        "class XI extends HTMLElement { \
             connectedCallback() { log.push('c:' + this.id + ':' + this.parentNode.childElementCount); } } \
         customElements.define('x-i', XI); \
         var host = document.getElementById('host'); \
         function el(id) { var e = document.createElement('x-i'); e.id = id; return e; } \
         log.length = 0; \
         host.append(el('a'), el('b')); log.push('append'); \
         host.prepend(el('c')); log.push('prepend'); \
         document.getElementById('a').before(el('d')); log.push('before'); \
         document.getElementById('a').after(el('e')); log.push('after'); \
         host.insertBefore(el('f'), null); log.push('insertBefore'); \
         host.appendChild(el('g')); log.push('appendChild'); \
         host.insertAdjacentElement('beforeend', el('h')); log.push('adjacent');",
    )
    .unwrap();
    assert_eq!(
        ev(&b, "log.join()"),
        "c:a:2,c:b:2,append,c:c:3,prepend,c:d:4,before,c:e:5,after,c:f:6,insertBefore,c:g:7,appendChild,c:h:8,adjacent"
    );
}

#[test]
fn reactions_run_on_exit_of_replace_children_and_replace_with() {
    let b = host_page();
    b.evaluate(
        "class XR extends HTMLElement { \
             connectedCallback() { log.push('c:' + this.id + ':' + host.childElementCount + ':' + (this.nextElementSibling ? this.nextElementSibling.id : 'end')); } \
             disconnectedCallback() { log.push('d:' + this.id + ':' + this.isConnected + ':' + host.childElementCount + ':' + host.firstElementChild.id); } } \
         customElements.define('x-r', XR); \
         var host = document.getElementById('host'); \
         function el(id) { var e = document.createElement('x-r'); e.id = id; return e; } \
         host.append(el('a'), el('b')); log.length = 0; \
         host.replaceChildren(el('c'), el('d')); log.push('replaceChildren'); \
         var other = document.createElement('div'); document.body.appendChild(other); host = other; \
         var w = el('w'); other.appendChild(w); log.push('armed'); \
         w.replaceWith(el('n1'), el('n2')); log.push('replaceWith');",
    )
    .unwrap();
    assert_eq!(
        ev(&b, "log.join()"),
        "d:a:false:2:c,d:b:false:2:c,c:c:2:d,c:d:2:end,replaceChildren,c:w:1:end,armed,d:w:false:2:n1,c:n1:2:n2,c:n2:2:end,replaceWith"
    );
}

#[test]
fn reactions_run_on_exit_of_removal() {
    let b = host_page();
    b.evaluate(
        "class XD extends HTMLElement { \
             disconnectedCallback() { log.push('d:' + this.id); } \
             connectedCallback() { log.push('c:' + this.id); } } \
         customElements.define('x-d', XD); \
         var host = document.getElementById('host'); \
         var a = document.createElement('x-d'); a.id = 'a'; host.appendChild(a); log.length = 0; \
         var removed = host.removeChild(a); log.push('removeChild:' + (removed === a) + ':' + a.isConnected); \
         host.appendChild(a); a.remove(); log.push('remove:' + a.isConnected);",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "d:a,removeChild:true:false,c:a,d:a,remove:false");
}

#[test]
fn reactions_run_on_exit_of_attribute_methods() {
    let b = host_page();
    b.evaluate(
        "class XF extends HTMLElement { \
             static get observedAttributes() { return ['a', 'b', 'id', 'class']; } \
             attributeChangedCallback(n, o, v) { log.push(n + ':' + o + '>' + v); } } \
         customElements.define('x-f', XF); var e = new XF(); \
         e.setAttribute('a', '1'); log.push('set'); \
         e.setAttributeNS(null, 'a', '2'); log.push('ns'); \
         e.removeAttributeNS(null, 'a'); log.push('rmns'); \
         e.setAttribute('b', 'x'); log.push('setb'); \
         e.toggleAttribute('b'); log.push('toggle'); \
         e.id = 'i'; log.push('id'); \
         e.className = 'c'; log.push('className'); \
         e.classList.add('z'); log.push('classList'); \
         e.setAttribute('a', '3'); e.attributes.removeNamedItem('a'); log.push('named'); \
         e.setAttribute('a', '4'); e.getAttributeNode('a').value = '5'; log.push('attr');",
    )
    .unwrap();
    assert_eq!(
        ev(&b, "log.join()"),
        "a:null>1,set,a:1>2,ns,a:2>null,rmns,b:null>x,setb,b:x>null,toggle,id:null>i,id,class:null>c,className,class:c>c z,classList,a:null>3,a:3>null,named,a:null>4,a:4>5,attr"
    );
}

#[test]
fn reactions_run_on_exit_of_inner_html() {
    let b = host_page();
    b.evaluate(
        "class XH extends HTMLElement { \
             constructor() { super(); log.push('ctor:' + this.id + ':' + (this.parentNode ? this.parentNode.childElementCount : 0) + ':' + this.isConnected); } \
             connectedCallback() { log.push('connected:' + this.id + ':' + this.parentNode.childElementCount); } } \
         customElements.define('x-h', XH); log.length = 0; \
         document.getElementById('host').innerHTML = '<x-h id=one></x-h><x-h id=two></x-h>'; \
         log.push('done');",
    )
    .unwrap();
    assert_eq!(
        ev(&b, "log.join()"),
        "ctor:one:2:true,connected:one:2,ctor:two:2:true,connected:two:2,done"
    );
}

#[test]
fn reactions_run_on_exit_of_insert_adjacent_html() {
    let b = host_page();
    b.evaluate(
        "class XH extends HTMLElement { \
             constructor() { super(); log.push('ctor:' + this.id + ':' + (this.parentNode ? this.parentNode.childElementCount : 0)); } \
             connectedCallback() { log.push('connected:' + this.id + ':' + this.parentNode.childElementCount + ':' + this.isConnected); } } \
         customElements.define('x-h', XH); log.length = 0; \
         document.getElementById('host').insertAdjacentHTML('beforeend', '<x-h id=three></x-h><x-h id=four></x-h>'); \
         log.push('done');",
    )
    .unwrap();
    assert_eq!(
        ev(&b, "log.join()"),
        "ctor:three:2,connected:three:2:true,ctor:four:2,connected:four:2:true,done"
    );
}

#[test]
fn reactions_run_on_exit_of_clone_node() {
    let b = host_page();
    b.evaluate(
        "var holder = document.createElement('div'); \
         holder.innerHTML = '<x-c id=p tone=live><x-c id=c></x-c></x-c>'; \
         class XC extends HTMLElement { \
             static get observedAttributes() { return ['tone']; } \
             constructor() { super(); var child = this.firstChild; \
                 log.push('ctor:' + this.id + ':' + this.isConnected + ':' + (child && child.nodeType === 1 ? (child instanceof XC) : 'leaf')); } \
             attributeChangedCallback(n, o, v) { log.push('attr:' + this.id + ':' + n + ':' + o + ':' + v + ':' + this.isConnected); } \
             connectedCallback() { log.push('connected:' + this.id); } } \
         customElements.define('x-c', XC); log.length = 0; \
         var copy = holder.firstChild.cloneNode(true); \
         log.push('done:' + copy.isConnected + ':' + (copy instanceof XC) + ':' + (copy.firstChild instanceof XC) + ':' + (holder.firstChild instanceof XC));",
    )
    .unwrap();
    assert_eq!(
        ev(&b, "log.join()"),
        "ctor:p:false:false,attr:p:tone:null:live:false,ctor:c:false:leaf,done:false:true:true:false"
    );
}

#[test]
fn reactions_run_on_exit_of_import_node() {
    let b = host_page();
    b.evaluate(
        "class XC extends HTMLElement { \
             static get observedAttributes() { return ['tone']; } \
             constructor() { super(); log.push('ctor:' + this.id + ':' + this.isConnected); } \
             attributeChangedCallback(n, o, v) { log.push('attr:' + this.id + ':' + o + ':' + v + ':' + this.isConnected); } \
             connectedCallback() { log.push('connected'); } } \
         customElements.define('x-c', XC); \
         var t = document.createElement('template'); \
         t.innerHTML = '<x-c id=p tone=live><x-c id=c></x-c></x-c>'; \
         log.length = 0; \
         var frag = document.importNode(t.content, true); \
         log.push('done:' + frag.firstChild.isConnected + ':' + (frag.firstChild instanceof XC) + ':' + (frag.firstChild.firstChild instanceof XC) + ':' + (t.content.firstChild instanceof XC));",
    )
    .unwrap();
    assert_eq!(
        ev(&b, "log.join()"),
        "ctor:p:false,attr:p:null:live:false,ctor:c:false,done:false:true:true:false"
    );
}

#[test]
fn reactions_run_on_exit_of_adopt_node() {
    let b = host_page();
    b.evaluate(
        "class XA extends HTMLElement { \
             disconnectedCallback() { log.push('d:' + this.id); } \
             adoptedCallback() { log.push('adopted'); } } \
         customElements.define('x-a', XA); \
         var host = document.getElementById('host'); \
         var a = document.createElement('x-a'); a.id = 'a'; host.appendChild(a); log.length = 0; \
         var back = document.adoptNode(a); \
         log.push('adopt:' + (back === a) + ':' + a.isConnected); \
         var loose = document.createElement('x-a'); \
         var n = log.length; document.adoptNode(loose); \
         log.push('loose:' + loose.isConnected + ':' + (log.length === n));",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "d:a,adopt:true:false,loose:false:true");
}

#[test]
fn reactions_run_on_exit_of_normalize() {
    let b = host_page();
    b.evaluate(
        "class XN extends HTMLElement { \
             connectedCallback() { log.push('c:' + this.id); } \
             disconnectedCallback() { log.push('d:' + this.id); } } \
         customElements.define('x-n', XN); \
         var host = document.getElementById('host'); \
         var a = document.createElement('x-n'); a.id = 'a'; host.appendChild(a); \
         host.appendChild(document.createTextNode('x')); \
         host.appendChild(document.createTextNode('')); \
         host.appendChild(document.createTextNode('y')); \
         var b = document.createElement('x-n'); b.id = 'b'; host.appendChild(b); \
         log.length = 0; host.normalize(); \
         log.push('done:' + host.childNodes.length + ':' + host.childNodes[1].data + ':' + a.isConnected + ':' + b.isConnected);",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "done:3:xy:true:true");
}

#[test]
fn reactions_run_on_exit_of_text_setters() {
    let b = host_page();
    b.evaluate(
        "class XT extends HTMLElement { disconnectedCallback() { log.push('d:' + this.id); } } \
         customElements.define('x-t', XT); \
         var host = document.getElementById('host'); \
         var a = document.createElement('x-t'); a.id = 'a'; host.appendChild(a); log.length = 0; \
         host.textContent = 'gone'; log.push('text:' + a.isConnected + ':' + host.textContent); \
         host.appendChild(document.createTextNode('z')); \
         var a2 = document.createElement('x-t'); a2.id = 'a2'; host.appendChild(a2); \
         host.firstChild.nodeValue = 'q'; log.push('nv:' + host.firstChild.data + ':' + a2.isConnected); \
         host.innerText = 't'; log.push('it:' + a2.isConnected + ':' + host.innerText);",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "d:a,text:false:gone,nv:q:true,d:a2,it:false:t");
}

#[test]
fn reactions_run_on_exit_of_define_and_create_element() {
    let b = bound(PAGE);
    b.evaluate(
        "class XA extends HTMLElement { \
             static get observedAttributes() { return ['color']; } \
             constructor() { super(); log.push('ctor:' + this.id); } \
             attributeChangedCallback(n, o, v) { log.push('attr:' + this.id + ':' + n + ':' + o + ':' + v); } \
             connectedCallback() { \
                 var sib = this.id === 'a1' ? document.getElementById('a2') : null; \
                 log.push('connected:' + this.id + ':' + this.isConnected + ':' + (sib ? (sib instanceof XA) : 'nosib')); \
             } } \
         customElements.define('x-a', XA); \
         var n = log.length; var made = document.createElement('x-a'); \
         log.push('created:' + (made instanceof XA) + ':' + (log.length === n + 1)); \
         var ns = document.createElementNS('http://www.w3.org/1999/xhtml', 'x-a'); \
         log.push('ns:' + (ns instanceof XA)); \
         var svg = document.createElementNS('http://www.w3.org/2000/svg', 'x-a'); \
         log.push('svg:' + (svg instanceof XA)); \
         var box = document.createElement('div'); box.innerHTML = '<x-u id=u></x-u>'; \
         class XU extends HTMLElement { constructor() { super(); log.push('up:' + this.id + ':' + this.isConnected); } } \
         customElements.define('x-u', XU); \
         customElements.upgrade(box); log.push('upgrade:' + (box.firstChild instanceof XU));",
    )
    .unwrap();
    assert_eq!(
        ev(&b, "log.join()"),
        "ctor:a1,attr:a1:color:null:red,connected:a1:true:false,ctor:a2,connected:a2:true:nosib,ctor:,created:true:true,ctor:,ns:true,svg:false,up:u:false,upgrade:true"
    );
}

#[test]
fn reactions_run_on_exit_of_range_delete_contents() {
    let b = host_page();
    b.evaluate(
        "class XG extends HTMLElement { disconnectedCallback() { log.push('d:' + this.id); } } \
         customElements.define('x-g', XG); \
         var host = document.getElementById('host'); \
         var a = document.createElement('x-g'); a.id = 'a'; host.appendChild(a); \
         var range = document.createRange(); range.selectNode(a); log.length = 0; \
         range.deleteContents(); log.push('range:' + a.isConnected + ':' + host.childElementCount);",
    )
    .unwrap();
    assert_eq!(ev(&b, "log.join()"), "d:a,range:false:0");
}
