//! The flat tree in the box-tree build. The build walks
//! `Node::flat_children` (rustkit-dom `shadow.rs`), so a shadow host's boxes
//! come from its shadow tree and a slot's from its assigned nodes; `<slot>`
//! is `display: contents`, so its children's boxes take its place.
//!
//! A node assigned to a slot sits in the flat tree under the slot, but the
//! selectors that match it read its own tree: its ancestors are its host
//! and the host's ancestors, not the slot and the shadow tree around it.
//! Each host records that chain while its shadow tree is built, and the
//! slot hands it to its assigned nodes.

use super::Ancestor;
use rustkit_dom::Node;
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    /// The shadow hosts the build is inside, innermost last: each host's
    /// NodeId and the ancestor chain its own children are matched against.
    static HOSTS: RefCell<Vec<(usize, Vec<Ancestor>)>> = const { RefCell::new(Vec::new()) };
}

/// Alive while a shadow host's flat children are built. A node that is not
/// a host records nothing.
pub(crate) struct HostScope {
    entered: bool,
}

impl HostScope {
    pub(crate) fn enter(node: &Rc<Node>, light_chain: &[Ancestor]) -> Self {
        if node.shadow_root().is_none() {
            return HostScope { entered: false };
        }
        HOSTS.with(|h| h.borrow_mut().push((node.id.raw(), light_chain.to_vec())));
        HostScope { entered: true }
    }
}

impl Drop for HostScope {
    fn drop(&mut self) {
        if self.entered {
            HOSTS.with(|h| {
                h.borrow_mut().pop();
            });
        }
    }
}

/// When `slot`'s flat children are its assigned nodes (children of a host,
/// not of the slot), the ancestor chain they are matched against: their
/// host's. `None` for anything else, whose children use its own chain.
pub(crate) fn slotted_ancestors(slot: &Rc<Node>, flat: &[Rc<Node>]) -> Option<Vec<Ancestor>> {
    if !slot.is_slot() {
        return None;
    }
    let host = flat.first()?.parent()?;
    if host.id == slot.id {
        return None;
    }
    HOSTS.with(|h| {
        h.borrow()
            .iter()
            .rev()
            .find(|(id, _)| *id == host.id.raw())
            .map(|(_, chain)| chain.clone())
    })
}
