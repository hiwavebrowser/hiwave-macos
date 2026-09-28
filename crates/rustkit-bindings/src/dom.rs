//! `document` / `Node` / `Element` read access backed by the Rust DOM
//! (js-ladder rung 0, read slice; design pin: trench/DESIGN-dom-bindings-rung0.md).
//!
//! The split follows the pin's §1/§2:
//! - Rust owns the nodes. Script reaches them only through host functions
//!   that take and return primitives: a node crosses the boundary as its
//!   raw `NodeId` number, a list of nodes as a space-separated id string.
//! - Each wrapper is a plain JS object whose one host payload is a hidden
//!   slot `{ id, gen }`. The identity cache (`Map<NodeId, wrapper>`) lives
//!   in the bindings' JS closure, so the same node always yields the same
//!   object and Rust never holds a `JsObject`.
//! - `gen` is the document generation. NodeIds restart at 1 in every
//!   Document, so a bare id is ambiguous once a new document is bound.
//!   Binding one bumps `gen` and drops the cache; a host function called
//!   with a stale `gen` answers null, so old wrappers fail soft instead of
//!   reading the new page's nodes.
//!
//! Nothing here mutates the tree: the mutation surface waits on the §3
//! dirty-bit flush.

use rustkit_dom::{Document, Node, NodeId, NodeType, QuerySelector};
use rustkit_js::{JsError, JsRuntime, JsValue};
use std::cell::RefCell;
use std::rc::Rc;

/// The document the host functions read, and its generation.
#[derive(Default)]
pub(crate) struct DomHost {
    document: Option<Rc<Document>>,
    generation: u32,
}

pub(crate) type SharedDomHost = Rc<RefCell<DomHost>>;

impl DomHost {
    /// Bind `document`, returning the new generation.
    pub(crate) fn bind(&mut self, document: Rc<Document>) -> u32 {
        self.document = Some(document);
        self.generation += 1;
        self.generation
    }

    fn document_for(&self, generation: &JsValue) -> Option<&Rc<Document>> {
        match generation {
            JsValue::Number(n) if *n == self.generation as f64 => self.document.as_ref(),
            _ => None,
        }
    }

    /// The node named by `args[0]` (generation) and `args[1]` (NodeId).
    fn node(&self, args: &[JsValue]) -> Option<Rc<Node>> {
        let document = self.document_for(args.first()?)?;
        match args.get(1)? {
            JsValue::Number(n) if *n >= 0.0 && n.fract() == 0.0 => {
                document.get_node(NodeId::new(*n as usize))
            }
            _ => None,
        }
    }
}

fn node_id(node: Option<Rc<Node>>) -> JsValue {
    node.map_or(JsValue::Null, |n| JsValue::Number(n.id.raw() as f64))
}

fn id_list(nodes: impl IntoIterator<Item = Rc<Node>>) -> JsValue {
    let ids: Vec<String> = nodes.into_iter().map(|n| n.id.raw().to_string()).collect();
    JsValue::String(ids.join(" "))
}

fn string_arg(args: &[JsValue], index: usize) -> Option<&str> {
    match args.get(index) {
        Some(JsValue::String(s)) => Some(s),
        _ => None,
    }
}

fn is_html_element(node: &Node) -> bool {
    matches!(&node.node_type, NodeType::Element { namespace, .. }
        if namespace.is_empty() || namespace == "http://www.w3.org/1999/xhtml")
}

/// Is `node` a descendant of `scope` (not `scope` itself)?
fn is_descendant(node: &Rc<Node>, scope: &Rc<Node>) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.id == scope.id {
            return true;
        }
        current = parent.parent();
    }
    false
}

/// `info(gen, id, field)`: one read of one node.
fn node_info(node: &Rc<Node>, field: &str) -> JsValue {
    match field {
        "type" => JsValue::Number(match node.node_type {
            NodeType::Element { .. } => 1.0,
            NodeType::Text(_) => 3.0,
            NodeType::ProcessingInstruction { .. } => 7.0,
            NodeType::Comment(_) => 8.0,
            NodeType::Document => 9.0,
            NodeType::DocumentType { .. } => 10.0,
        }),
        "name" => JsValue::String(match &node.node_type {
            NodeType::Element { tag_name, .. } if is_html_element(node) => {
                tag_name.to_ascii_uppercase()
            }
            NodeType::Element { tag_name, .. } => tag_name.clone(),
            NodeType::Text(_) => "#text".to_string(),
            NodeType::Comment(_) => "#comment".to_string(),
            NodeType::Document => "#document".to_string(),
            NodeType::DocumentType { name, .. } => name.clone(),
            NodeType::ProcessingInstruction { target, .. } => target.clone(),
        }),
        "local" => match &node.node_type {
            NodeType::Element { tag_name, .. } => JsValue::String(tag_name.clone()),
            _ => JsValue::Null,
        },
        // DOM §4.4 textContent: null for documents and doctypes, the data
        // of character data, and the concatenated Text descendants of an
        // element (comments excluded).
        "text" => match &node.node_type {
            NodeType::Document | NodeType::DocumentType { .. } => JsValue::Null,
            NodeType::Text(data) | NodeType::Comment(data) => JsValue::String(data.clone()),
            NodeType::ProcessingInstruction { data, .. } => JsValue::String(data.clone()),
            NodeType::Element { .. } => JsValue::String(node.text_content()),
        },
        "parent" => node_id(node.parent()),
        "first" => node_id(node.first_child()),
        "last" => node_id(node.last_child()),
        "next" => node_id(node.next_sibling()),
        "prev" => node_id(node.previous_sibling()),
        "children" => id_list(node.children()),
        _ => JsValue::Undefined,
    }
}

/// Register the host functions and install the wrapper layer over the
/// stub `document`. Call after the stub globals exist.
pub(crate) fn install(runtime: &mut JsRuntime, host: &SharedDomHost) -> Result<(), JsError> {
    let h = host.clone();
    runtime.register_host_function(
        "__rustkit_dom_root",
        2,
        Box::new(move |args| {
            let host = h.borrow();
            let Some(document) = args.first().and_then(|g| host.document_for(g)) else {
                return JsValue::Null;
            };
            match string_arg(args, 1) {
                Some("document") => node_id(Some(document.root().clone())),
                Some("documentElement") => node_id(document.document_element()),
                Some("head") => node_id(document.head()),
                Some("body") => node_id(document.body()),
                _ => JsValue::Null,
            }
        }),
    )?;

    let h = host.clone();
    runtime.register_host_function(
        "__rustkit_dom_by_id",
        2,
        Box::new(move |args| {
            let host = h.borrow();
            match (
                args.first().and_then(|g| host.document_for(g)),
                string_arg(args, 1),
            ) {
                (Some(document), Some(id)) => node_id(document.get_element_by_id(id)),
                _ => JsValue::Null,
            }
        }),
    )?;

    // collect(gen, scopeId, kind, arg): elements in document order, limited
    // to the descendants of `scopeId` (the document root means all).
    let h = host.clone();
    runtime.register_host_function(
        "__rustkit_dom_collect",
        4,
        Box::new(move |args| {
            let host = h.borrow();
            let (Some(document), Some(scope)) = (
                args.first().and_then(|g| host.document_for(g)),
                host.node(args),
            ) else {
                return JsValue::Null;
            };
            let arg = string_arg(args, 3).unwrap_or("");
            let found = match string_arg(args, 2) {
                Some("tag") if arg == "*" => {
                    let mut all = Vec::new();
                    document.traverse(|n| {
                        if n.is_element() {
                            all.push(n.clone());
                        }
                    });
                    all
                }
                Some("tag") => document.get_elements_by_tag_name(arg),
                Some("class") => document.get_elements_by_class_name(arg),
                Some("selector") => QuerySelector::select(document, arg),
                _ => return JsValue::Null,
            };
            if scope.id == document.root().id {
                id_list(found)
            } else {
                id_list(found.into_iter().filter(|n| is_descendant(n, &scope)))
            }
        }),
    )?;

    let h = host.clone();
    runtime.register_host_function(
        "__rustkit_dom_info",
        3,
        Box::new(move |args| {
            let host = h.borrow();
            match (host.node(args), string_arg(args, 2)) {
                (Some(node), Some(field)) => node_info(&node, field),
                _ => JsValue::Null,
            }
        }),
    )?;

    let h = host.clone();
    runtime.register_host_function(
        "__rustkit_dom_attr",
        3,
        Box::new(move |args| {
            let host = h.borrow();
            let (Some(node), Some(name)) = (host.node(args), string_arg(args, 2)) else {
                return JsValue::Null;
            };
            // HTML attribute names are matched ASCII-lowercased.
            let name = if is_html_element(&node) {
                name.to_ascii_lowercase()
            } else {
                name.to_string()
            };
            node.get_attribute(&name)
                .map_or(JsValue::Null, |v| JsValue::String(v.to_string()))
        }),
    )?;

    runtime.evaluate_script(WRAPPERS_JS)?;
    Ok(())
}

/// Wrapper prototypes, the identity cache, and the `document` read
/// surface. `window.__rustkit_dom_reset(gen)` rebinds it to a new document.
const WRAPPERS_JS: &str = r#"
(function (g) {
    var N = {
        root: __rustkit_dom_root, byId: __rustkit_dom_by_id,
        collect: __rustkit_dom_collect, info: __rustkit_dom_info,
        attr: __rustkit_dom_attr
    };
    ['root', 'by_id', 'collect', 'info', 'attr'].forEach(function (n) {
        delete g['__rustkit_dom_' + n];
    });

    var SLOT = Symbol('rustkit.node');
    var gen = 0;
    var cache = new Map();

    function illegal() { throw new TypeError('Illegal constructor'); }
    function iface(name, parent) {
        var ctor = function () { illegal(); };
        Object.defineProperty(ctor, 'name', { value: name });
        if (parent) Object.setPrototypeOf(ctor.prototype, parent.prototype);
        Object.defineProperty(ctor.prototype, Symbol.toStringTag, { value: name });
        g[name] = ctor;
        return ctor;
    }
    var Node = iface('Node');
    var Document = iface('Document', Node);
    var CharacterData = iface('CharacterData', Node);
    var Text = iface('Text', CharacterData);
    var Comment = iface('Comment', CharacterData);
    var Element = iface('Element', Node);
    var HTMLElement = iface('HTMLElement', Element);
    var NodeList = iface('NodeList');
    var HTMLCollection = iface('HTMLCollection');

    var types = { ELEMENT_NODE: 1, TEXT_NODE: 3, PROCESSING_INSTRUCTION_NODE: 7,
                  COMMENT_NODE: 8, DOCUMENT_NODE: 9, DOCUMENT_TYPE_NODE: 10 };
    Object.keys(types).forEach(function (k) { Node[k] = Node.prototype[k] = types[k]; });

    function slotOf(o) {
        var s = o != null ? o[SLOT] : undefined;
        if (!s) throw new TypeError('Illegal invocation');
        return s;
    }
    function wrap(id) {
        if (typeof id !== 'number') return null;
        var w = cache.get(id);
        if (w) return w;
        var t = N.info(gen, id, 'type');
        var proto = t === 1 ? HTMLElement.prototype : t === 3 ? Text.prototype
                  : t === 8 ? Comment.prototype : Node.prototype;
        w = Object.create(proto);
        Object.defineProperty(w, SLOT, { value: { id: id, gen: gen } });
        cache.set(id, w);
        return w;
    }
    // A wrapper from an older document answers null/empty: its gen no
    // longer matches, so the host functions return null for it.
    function info(o, field) { var s = slotOf(o); return N.info(s.gen, s.id, field); }
    function related(o, field) { var s = slotOf(o); return s.gen === gen ? wrap(N.info(s.gen, s.id, field)) : null; }
    function list(proto, ids, onlyElements) {
        var out = Object.create(proto), n = 0;
        if (typeof ids === 'string' && ids !== '') {
            ids.split(' ').forEach(function (id) {
                var w = wrap(Number(id));
                if (w && (!onlyElements || w.nodeType === 1)) out[n++] = w;
            });
        }
        Object.defineProperty(out, 'length', { value: n });
        return out;
    }
    function collect(o, kind, arg) {
        var s = slotOf(o);
        return s.gen === gen ? N.collect(s.gen, s.id, kind, String(arg)) : '';
    }
    function getter(proto, name, fn) {
        Object.defineProperty(proto, name, { get: fn, configurable: true, enumerable: true });
    }

    [NodeList, HTMLCollection].forEach(function (C) {
        C.prototype.item = function (i) { return this[i >>> 0] || null; };
        C.prototype[Symbol.iterator] = Array.prototype.values;
    });
    NodeList.prototype.forEach = Array.prototype.forEach;
    NodeList.prototype.entries = Array.prototype.entries;
    NodeList.prototype.keys = Array.prototype.keys;
    NodeList.prototype.values = Array.prototype.values;

    getter(Node.prototype, 'nodeType', function () { return info(this, 'type'); });
    getter(Node.prototype, 'nodeName', function () { return info(this, 'name'); });
    getter(Node.prototype, 'textContent', function () { return info(this, 'text'); });
    getter(Node.prototype, 'parentNode', function () { return related(this, 'parent'); });
    getter(Node.prototype, 'parentElement', function () {
        var p = related(this, 'parent'); return p && p.nodeType === 1 ? p : null;
    });
    getter(Node.prototype, 'firstChild', function () { return related(this, 'first'); });
    getter(Node.prototype, 'lastChild', function () { return related(this, 'last'); });
    getter(Node.prototype, 'nextSibling', function () { return related(this, 'next'); });
    getter(Node.prototype, 'previousSibling', function () { return related(this, 'prev'); });
    getter(Node.prototype, 'childNodes', function () {
        var s = slotOf(this);
        return list(NodeList.prototype, s.gen === gen ? N.info(s.gen, s.id, 'children') : '', false);
    });
    getter(Node.prototype, 'ownerDocument', function () {
        return info(this, 'type') === 9 ? null : g.document;
    });
    Node.prototype.hasChildNodes = function () { return this.firstChild !== null; };
    Node.prototype.contains = function (other) {
        for (var n = other; n; n = n.parentNode) if (n === this) return true;
        return false;
    };
    getter(CharacterData.prototype, 'data', function () { return info(this, 'text'); });
    getter(CharacterData.prototype, 'nodeValue', function () { return info(this, 'text'); });
    getter(CharacterData.prototype, 'length', function () { return (info(this, 'text') || '').length; });

    getter(Element.prototype, 'tagName', function () { return info(this, 'name'); });
    getter(Element.prototype, 'localName', function () { return info(this, 'local'); });
    getter(Element.prototype, 'id', function () { return this.getAttribute('id') || ''; });
    getter(Element.prototype, 'className', function () { return this.getAttribute('class') || ''; });
    getter(Element.prototype, 'children', function () {
        var s = slotOf(this);
        return list(HTMLCollection.prototype, s.gen === gen ? N.info(s.gen, s.id, 'children') : '', true);
    });
    Element.prototype.getAttribute = function (name) {
        var s = slotOf(this); return N.attr(s.gen, s.id, String(name));
    };
    Element.prototype.hasAttribute = function (name) { return this.getAttribute(name) !== null; };

    // querySelector/All, getElementsBy* on both Document and Element. The
    // collections are static snapshots (pin §4: live HTMLCollection later).
    var queries = {
        querySelector: function (sel) {
            var ids = collect(this, 'selector', sel);
            return ids ? wrap(Number(ids.split(' ')[0])) : null;
        },
        querySelectorAll: function (sel) {
            return list(NodeList.prototype, collect(this, 'selector', sel), false);
        },
        getElementsByTagName: function (tag) {
            return list(HTMLCollection.prototype, collect(this, 'tag', tag), false);
        },
        getElementsByClassName: function (cls) {
            return list(HTMLCollection.prototype, collect(this, 'class', cls), false);
        }
    };
    Object.keys(queries).forEach(function (k) {
        Element.prototype[k] = queries[k];
        Document.prototype[k] = queries[k];
    });
    Document.prototype.getElementById = function (id) {
        var s = slotOf(this);
        return s.gen === gen ? wrap(N.byId(s.gen, String(id))) : null;
    };
    ['documentElement', 'head', 'body'].forEach(function (k) {
        getter(Document.prototype, k, function () {
            var s = slotOf(this); return s.gen === gen ? wrap(N.root(s.gen, k)) : null;
        });
    });

    // The global `document` becomes the Document wrapper.
    var doc = g.document;
    Object.setPrototypeOf(doc, Document.prototype);

    g.__rustkit_dom_reset = function (newGen) {
        gen = newGen;
        cache = new Map();
        var root = N.root(gen, 'document');
        Object.defineProperty(doc, SLOT, { value: { id: root, gen: gen }, configurable: true });
        if (typeof root === 'number') cache.set(root, doc);
    };
    g.__rustkit_dom_reset(0);
})(globalThis);
"#;
