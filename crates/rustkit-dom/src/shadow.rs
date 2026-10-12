//! Shadow DOM in the node tree (DOM §4.2.2 shadow trees, §4.2.2.3 the flat
//! tree, §4.2.2.4 slots; HTML §4.12.3.1 declarative shadow roots).
//!
//! A shadow root is a node of this document like any other: it has a
//! NodeId, sits in the node table, and is typed `DocumentFragment` (script
//! sees `nodeType` 11). It is in no node's child list. Its host owns it
//! through the host's `shadow` field and the root points back at its host
//! weakly, so the pair is not a reference cycle. Everything that reads the
//! tree through `children()`/`parent()` (the document's queries, `id`
//! lookups, serialization) therefore stays out of shadow trees without
//! knowing they exist; what renders reads `flat_children()` instead.

use super::{Document, DomError, Node, NodeType};
use std::rc::{Rc, Weak};

/// `ShadowRootMode` (DOM §4.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowRootMode {
    Open,
    Closed,
}

/// What a shadow root was attached with (`ShadowRootInit`, DOM §4.2.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShadowRootInit {
    pub mode: ShadowRootMode,
    pub delegates_focus: bool,
    /// `slotAssignment: "manual"`. Manual assignment (`slot.assign()`) is
    /// not implemented: under it no node is assigned to any slot.
    pub manual_slot_assignment: bool,
}

impl ShadowRootInit {
    pub fn open() -> Self {
        Self {
            mode: ShadowRootMode::Open,
            delegates_focus: false,
            manual_slot_assignment: false,
        }
    }
}

/// A node's side of a host/shadow-root pair.
#[derive(Debug)]
pub(crate) enum ShadowLink {
    /// On a host: its shadow root.
    Root(Rc<Node>),
    /// On a shadow root: its host, and how the root was attached.
    Host(Weak<Node>, ShadowRootInit),
}

/// DOM §4.2.14 "valid shadow host name": the listed HTML elements and
/// valid custom element names.
pub fn is_valid_shadow_host_name(name: &str) -> bool {
    const HOSTS: [&str; 18] = [
        "article",
        "aside",
        "blockquote",
        "body",
        "div",
        "footer",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "header",
        "main",
        "nav",
        "p",
        "section",
        "span",
    ];
    const RESERVED: [&str; 8] = [
        "annotation-xml",
        "color-profile",
        "font-face",
        "font-face-src",
        "font-face-uri",
        "font-face-format",
        "font-face-name",
        "missing-glyph",
    ];
    let lower = name.to_ascii_lowercase();
    if HOSTS.contains(&lower.as_str()) {
        return true;
    }
    // HTML §4.13.3 valid custom element name, simplified: a lowercase
    // ASCII letter first, a hyphen, no uppercase ASCII, not reserved.
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.contains('-')
        && !name
            .chars()
            .any(|c| c.is_ascii_uppercase() || c.is_ascii_whitespace())
        && !RESERVED.contains(&name)
}

impl Node {
    /// This host's shadow root, whatever its mode.
    pub fn shadow_root(&self) -> Option<Rc<Node>> {
        match &*self.shadow.borrow() {
            Some(ShadowLink::Root(root)) => Some(root.clone()),
            _ => None,
        }
    }

    /// This shadow root's host.
    pub fn shadow_host(&self) -> Option<Rc<Node>> {
        match &*self.shadow.borrow() {
            Some(ShadowLink::Host(host, _)) => host.upgrade(),
            _ => None,
        }
    }

    /// How this shadow root was attached; `None` for any other node.
    pub fn shadow_root_init(&self) -> Option<ShadowRootInit> {
        match &*self.shadow.borrow() {
            Some(ShadowLink::Host(_, init)) => Some(*init),
            _ => None,
        }
    }

    pub fn is_shadow_root(&self) -> bool {
        matches!(&*self.shadow.borrow(), Some(ShadowLink::Host(..)))
    }

    /// The shadow root this node is in (its root, when that root is a
    /// shadow root).
    pub fn containing_shadow_root(self: &Rc<Self>) -> Option<Rc<Node>> {
        let mut top = self.clone();
        while let Some(parent) = top.parent() {
            top = parent;
        }
        top.is_shadow_root().then_some(top)
    }

    /// A `<slot>` element (in any tree).
    pub fn is_slot(&self) -> bool {
        self.tag_name() == Some("slot")
    }

    /// A `<slot>` in a shadow tree: the slots that take assigned nodes and
    /// render as `display: contents`.
    pub fn is_shadow_slot(self: &Rc<Self>) -> bool {
        self.is_slot() && self.containing_shadow_root().is_some()
    }

    /// The nodes assigned to this slot (DOM §4.2.2.4 "find slottables"):
    /// the host's element and text children whose slot name is this
    /// slot's name, in tree order, when this is the first slot of that
    /// name in its shadow tree. Empty for anything else.
    pub fn assigned_nodes(self: &Rc<Self>) -> Vec<Rc<Node>> {
        if !self.is_slot() {
            return Vec::new();
        }
        let Some(root) = self.containing_shadow_root() else {
            return Vec::new();
        };
        let (Some(host), Some(init)) = (root.shadow_host(), root.shadow_root_init()) else {
            return Vec::new();
        };
        if init.manual_slot_assignment {
            return Vec::new();
        }
        let name = self.get_attribute("name").unwrap_or("");
        if find_slot(&root, name).map(|s| s.id) != Some(self.id) {
            return Vec::new();
        }
        host.children()
            .into_iter()
            .filter(|c| is_slottable(c) && slottable_name(c) == name)
            .collect()
    }

    /// The slot this node is assigned to (DOM §4.2.2.4 "find a slot"),
    /// whatever its shadow root's mode.
    pub fn assigned_slot(&self) -> Option<Rc<Node>> {
        if !is_slottable(self) {
            return None;
        }
        let root = self.parent()?.shadow_root()?;
        if root.shadow_root_init()?.manual_slot_assignment {
            return None;
        }
        find_slot(&root, slottable_name(self))
    }

    /// This node's children in the flat tree (DOM §4.2.2.3), the tree
    /// style, layout and paint walk: a shadow host's are its shadow root's
    /// children (its own children render only through slots); a slot in a
    /// shadow tree has its assigned nodes, or its own children (the
    /// fallback content) when nothing is assigned; any other node has its
    /// children. Slots stay in the flat tree; the renderer treats them as
    /// `display: contents`.
    pub fn flat_children(self: &Rc<Self>) -> Vec<Rc<Node>> {
        if let Some(root) = self.shadow_root() {
            return root.children();
        }
        if self.is_slot() {
            let assigned = self.assigned_nodes();
            if !assigned.is_empty() {
                return assigned;
            }
        }
        self.children()
    }
}

fn is_slottable(node: &Node) -> bool {
    matches!(node.node_type, NodeType::Element { .. } | NodeType::Text(_))
}

/// A slottable's name: an element's `slot` attribute; text has none.
fn slottable_name(node: &Node) -> &str {
    node.get_attribute("slot").unwrap_or("")
}

/// The first slot named `name` in `root`'s tree, in tree order. Does not
/// look into nested shadow trees.
fn find_slot(root: &Rc<Node>, name: &str) -> Option<Rc<Node>> {
    let mut pending: Vec<Rc<Node>> = root.children().into_iter().rev().collect();
    while let Some(node) = pending.pop() {
        if node.is_slot() && node.get_attribute("name").unwrap_or("") == name {
            return Some(node);
        }
        pending.extend(node.children().into_iter().rev());
    }
    None
}

impl Document {
    /// Has any node of this document ever had a shadow root?
    pub fn has_shadow_roots(&self) -> bool {
        self.shadow_roots.get() > 0
    }

    /// DOM §4.2.14 attachShadow: give `host` a new, empty shadow root.
    /// Fails with `InvalidOperation("NotSupportedError")` when `host` is not
    /// a valid shadow host or already has one.
    pub fn attach_shadow(
        &self,
        host: &Rc<Node>,
        init: ShadowRootInit,
    ) -> Result<Rc<Node>, DomError> {
        let valid = host.tag_name().is_some_and(is_valid_shadow_host_name);
        if !valid || host.shadow.borrow().is_some() {
            return Err(DomError::InvalidOperation("NotSupportedError".into()));
        }
        let root = self.create_node(NodeType::DocumentFragment);
        *root.shadow.borrow_mut() = Some(ShadowLink::Host(Rc::downgrade(host), init));
        *host.shadow.borrow_mut() = Some(ShadowLink::Root(root.clone()));
        self.shadow_roots.set(self.shadow_roots.get() + 1);
        Ok(root)
    }

    /// Every node in shadow-including tree order (DOM §4.2.2.1): a host,
    /// then its shadow root and that tree, then its children.
    pub fn traverse_shadow_including<F>(&self, mut callback: F)
    where
        F: FnMut(&Rc<Node>),
    {
        fn walk<F: FnMut(&Rc<Node>)>(node: &Rc<Node>, callback: &mut F) {
            callback(node);
            if let Some(root) = node.shadow_root() {
                walk(&root, callback);
            }
            for child in node.children() {
                walk(&child, callback);
            }
        }
        if !self.has_shadow_roots() {
            return self.traverse(callback);
        }
        walk(&self.root, &mut callback);
    }

    /// `get_elements_by_tag_name`, shadow trees included: for the
    /// subresources (images, inline SVG) that render wherever they are.
    pub fn get_elements_by_tag_name_shadow_including(&self, tag_name: &str) -> Vec<Rc<Node>> {
        if !self.has_shadow_roots() {
            return self.get_elements_by_tag_name(tag_name);
        }
        let mut result = Vec::new();
        self.traverse_shadow_including(|n| {
            if n.tag_name()
                .is_some_and(|t| t.eq_ignore_ascii_case(tag_name))
            {
                result.push(n.clone());
            }
        });
        result
    }
}

/// `replace_node_data` built `new` to stand for `old`: the shadow link
/// moves to it, and the other side of the pair points at it.
pub(crate) fn move_link(old: &Rc<Node>, new: &Rc<Node>) {
    let link = old.shadow.borrow_mut().take();
    match &link {
        Some(ShadowLink::Root(root)) => {
            if let Some(ShadowLink::Host(host, _)) = &mut *root.shadow.borrow_mut() {
                *host = Rc::downgrade(new);
            }
        }
        Some(ShadowLink::Host(host, _)) => {
            if let Some(host) = host.upgrade() {
                *host.shadow.borrow_mut() = Some(ShadowLink::Root(new.clone()));
            }
        }
        None => {}
    }
    *new.shadow.borrow_mut() = link;
}

/// HTML §13.2.6.4.4 (the "in head"/"in body" template start tag with
/// `shadowrootmode`), applied after the parse: each such template whose
/// parent can host a shadow root and has none yet becomes that parent's
/// shadow root, its children moving into the root, and the template leaves
/// the tree. Innermost first, so a nested declarative root is attached
/// before its host moves into the outer one. Any other template stays an
/// ordinary template.
pub(crate) fn attach_declarative(doc: &Document) {
    let mut templates = Vec::new();
    doc.traverse(|n| {
        if n.tag_name() == Some("template") && n.get_attribute("shadowrootmode").is_some() {
            templates.push(n.clone());
        }
    });
    for template in templates.into_iter().rev() {
        let mode = match template
            .get_attribute("shadowrootmode")
            .map(str::to_ascii_lowercase)
        {
            Some(m) if m == "open" => ShadowRootMode::Open,
            Some(m) if m == "closed" => ShadowRootMode::Closed,
            _ => continue,
        };
        let Some(host) = template.parent() else {
            continue;
        };
        let init = ShadowRootInit {
            mode,
            delegates_focus: template.get_attribute("shadowrootdelegatesfocus").is_some(),
            manual_slot_assignment: false,
        };
        let Ok(root) = doc.attach_shadow(&host, init) else {
            continue;
        };
        for child in template.children() {
            child.remove_from_parent();
            root.append_child(child);
        }
        template.remove_from_parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(nodes: &[Rc<Node>]) -> String {
        nodes
            .iter()
            .map(|n| match &n.node_type {
                NodeType::Text(t) => format!("'{t}'"),
                _ => n.get_attribute("id").unwrap_or("?").to_string(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn by_id(doc: &Document, id: &str) -> Rc<Node> {
        let mut found = None;
        doc.traverse_shadow_including(|n| {
            if found.is_none() && n.get_attribute("id") == Some(id) {
                found = Some(n.clone());
            }
        });
        found.unwrap_or_else(|| panic!("no #{id}"))
    }

    const PAGE: &str = "<body><div id=h>\
        <template shadowrootmode=open><p id=top></p><slot name=x id=sx><i id=fbx></i></slot>\
        <slot id=sd><i id=fbd></i></slot><slot name=none id=sn><i id=fbn></i></slot></template>\
        <b id=d1></b><b id=x1 slot=x></b>t<b id=gone slot=nowhere></b><b id=x2 slot=x></b></div></body>";

    #[test]
    fn the_flat_tree_of_a_host_and_its_slots() {
        let doc = Document::parse_html(PAGE).unwrap();
        assert!(doc.has_shadow_roots());
        let host = by_id(&doc, "h");
        // The host's light children are untouched; its flat children are
        // its shadow tree.
        assert_eq!(ids(&host.children()), "d1 x1 't' gone x2");
        assert_eq!(ids(&host.flat_children()), "top sx sd sn");
        // Named slot: its nodes in tree order. Default slot: unnamed
        // elements and text. A slot with nothing assigned: its fallback.
        assert_eq!(ids(&by_id(&doc, "sx").flat_children()), "x1 x2");
        assert_eq!(ids(&by_id(&doc, "sd").flat_children()), "d1 't'");
        assert_eq!(ids(&by_id(&doc, "sn").flat_children()), "fbn");
        // `gone` names no slot: no slot has it.
        assert!(by_id(&doc, "gone").assigned_slot().is_none());
        assert_eq!(
            by_id(&doc, "x2").assigned_slot().unwrap().id,
            by_id(&doc, "sx").id
        );
        // The shadow root is nobody's child.
        let root = host.shadow_root().unwrap();
        assert!(root.parent().is_none());
        assert_eq!(root.shadow_host().unwrap().id, host.id);
        assert_eq!(root.shadow_root_init(), Some(ShadowRootInit::open()));
        assert_eq!(
            by_id(&doc, "top").containing_shadow_root().unwrap().id,
            root.id
        );
    }

    #[test]
    fn only_the_first_slot_of_a_name_is_assigned_and_a_slot_in_fallback_works() {
        let doc = Document::parse_html(
            "<body><div id=h><template shadowrootmode=open>\
             <slot name=a id=a1><slot name=b id=b1><i id=fbb></i></slot></slot><slot name=a id=a2><i id=fba></i></slot>\
             </template><b id=for-b slot=b></b></div></body>",
        )
        .unwrap();
        // a1 has nothing: its fallback is slot b1, which has for-b.
        assert_eq!(ids(&by_id(&doc, "a1").flat_children()), "b1");
        assert_eq!(ids(&by_id(&doc, "b1").flat_children()), "for-b");
        // a2 is the second slot named `a`: never assigned, so its fallback.
        assert_eq!(ids(&by_id(&doc, "a2").flat_children()), "fba");
    }

    #[test]
    fn nested_declarative_roots_attach_innermost_first() {
        let doc = Document::parse_html(
            "<body><div id=outer><template shadowrootmode=open><x-in id=inner>\
             <template shadowrootmode=closed><slot id=s></slot></template><i id=li></i></x-in></template></div></body>",
        )
        .unwrap();
        let outer = by_id(&doc, "outer");
        let inner = by_id(&doc, "inner");
        assert_eq!(ids(&outer.flat_children()), "inner");
        assert_eq!(
            inner
                .shadow_root()
                .unwrap()
                .shadow_root_init()
                .unwrap()
                .mode,
            ShadowRootMode::Closed
        );
        assert_eq!(ids(&inner.flat_children()), "s");
        assert_eq!(ids(&by_id(&doc, "s").flat_children()), "li");
        // Shadow-including order: a host, its shadow tree, then its children.
        let mut order = Vec::new();
        doc.traverse_shadow_including(|n| {
            if let Some(id) = n.get_attribute("id") {
                order.push(id.to_string());
            }
        });
        assert_eq!(order, ["outer", "inner", "s", "li"]);
    }

    #[test]
    fn attach_shadow_validates_the_host() {
        let doc = Document::parse_html("<body><div id=d></div><a id=a></a><x-y id=c></x-y></body>")
            .unwrap();
        assert!(!doc.has_shadow_roots());
        let d = by_id(&doc, "d");
        assert!(doc.attach_shadow(&d, ShadowRootInit::open()).is_ok());
        assert!(
            doc.attach_shadow(&d, ShadowRootInit::open()).is_err(),
            "one per host"
        );
        assert!(doc
            .attach_shadow(&by_id(&doc, "a"), ShadowRootInit::open())
            .is_err());
        assert!(doc
            .attach_shadow(&by_id(&doc, "c"), ShadowRootInit::open())
            .is_ok());
        for name in ["div", "span", "x-y", "my-element", "body"] {
            assert!(is_valid_shadow_host_name(name), "{name}");
        }
        for name in ["a", "img", "font-face", "x", "-x", "X-y"] {
            assert!(!is_valid_shadow_host_name(name), "{name}");
        }
    }

    #[test]
    fn replacing_the_hosts_data_keeps_the_pair() {
        let doc = Document::parse_html("<body><div id=h><b id=x></b></div></body>").unwrap();
        let host = by_id(&doc, "h");
        let root = doc.attach_shadow(&host, ShadowRootInit::open()).unwrap();
        // An attribute write builds a new node for the same NodeId.
        let new = doc
            .replace_node_data(
                host.id,
                NodeType::Element {
                    tag_name: "div".into(),
                    namespace: "http://www.w3.org/1999/xhtml".into(),
                    attributes: [
                        ("id".to_string(), "h".to_string()),
                        ("class".to_string(), "c".to_string()),
                    ]
                    .into(),
                },
            )
            .unwrap();
        assert!(host.shadow_root().is_none(), "the old node lets go");
        assert_eq!(new.shadow_root().unwrap().id, root.id);
        assert!(Rc::ptr_eq(&root.shadow_host().unwrap(), &new));
    }

    #[test]
    fn the_pair_is_not_a_cycle() {
        let doc = Document::parse_html("<body><div id=h></div></body>").unwrap();
        let (host, root) = {
            let host = by_id(&doc, "h");
            let root = doc.attach_shadow(&host, ShadowRootInit::open()).unwrap();
            (Rc::downgrade(&host), Rc::downgrade(&root))
        };
        drop(doc);
        assert!(
            host.upgrade().is_none() && root.upgrade().is_none(),
            "dropping the document frees both"
        );
    }
}
