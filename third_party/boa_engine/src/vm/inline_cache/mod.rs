use arrayvec::ArrayVec;
use itertools::Itertools;
use std::{cell::Cell, fmt};

use boa_gc::GcRefCell;
use boa_macros::{Finalize, Trace};

use crate::{
    JsString,
    object::shape::{Shape, WeakShape, slot::Slot},
};

#[cfg(test)]
mod tests;

pub(crate) const PIC_CAPACITY: usize = 4;

/// A cached shape-to-slot mapping for a polymorphic inline cache.
#[derive(Clone, Debug, Trace, Finalize)]
pub(crate) struct CacheEntry {
    /// A weak reference is kept to the shape to avoid the shape preventing deallocation.
    pub(crate) shape: WeakShape,
    #[unsafe_ignore_trace]
    pub(crate) slot: Slot,
}

/// An inline cache entry for a property access.
#[derive(Clone, Debug, Trace, Finalize)]
pub(crate) struct InlineCache {
    /// The property that is accessed.
    pub(crate) name: JsString,

    /// Multiple cached shape-to-slot entries.
    pub(crate) entries: GcRefCell<ArrayVec<CacheEntry, PIC_CAPACITY>>,

    /// Whether this access site has seen too many shapes and should no longer be cached.
    #[unsafe_ignore_trace]
    pub(crate) megamorphic: Cell<bool>,

    /// HIWAVE PATCH (ic-exotic-get): whether `name` is a canonical numeric string (an array index,
    /// `Infinity`, `NaN`, `-0`, `1.5`, ...), the names for which a typed array's [[Get]] reads its
    /// elements and never the prototype, and the only names a mapped arguments object's [[Get]]
    /// can answer from its parameter map. Worked out once here so a hit does not parse the name.
    #[unsafe_ignore_trace]
    pub(crate) numeric_name: bool,
}

impl fmt::Display for InlineCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(name:{} entries:", self.name.display_escaped())?;

        if self.megamorphic.get() {
            return write!(f, "(megamorphic))");
        }

        let entries = self.entries.borrow();
        let entries = entries.iter().map(|e| e.shape.to_addr_usize()).format(", ");

        write!(f, "({entries:#x}))")
    }
}

impl InlineCache {
    pub(crate) fn new(name: JsString) -> Self {
        let numeric_name =
            crate::builtins::typed_array::object::canonical_numeric_index_string(&name).is_some();
        Self {
            name,
            entries: GcRefCell::new(ArrayVec::new()),
            megamorphic: Cell::new(false),
            numeric_name,
        }
    }

    /// HIWAVE PATCH (ic-exotic-get): whether a read that hits this cache on `object` is what
    /// `object`'s [[Get]] would do. The cache is keyed on shape, and an exotic object can share an
    /// ordinary object's shape (a Proxy made from `{}`, a typed array and an object created from its
    /// prototype). An ordinary [[Get]] always is; a typed array's and an arguments object's are for
    /// a name that is not numeric (they differ from the ordinary one only for numeric names); a
    /// Proxy's (its `get` trap) and a module namespace's never are.
    #[inline(always)]
    pub(crate) fn hit_is_get(&self, object: &crate::JsObject) -> bool {
        type Get = fn(
            &crate::JsObject,
            &crate::property::PropertyKey,
            crate::JsValue,
            &mut crate::object::internal_methods::InternalMethodPropertyContext<'_>,
        ) -> crate::JsResult<crate::JsValue>;
        let get = object.vtable().__get__;
        let ordinary: Get = crate::object::internal_methods::ordinary_get;
        if std::ptr::fn_addr_eq(get, ordinary) {
            return true;
        }
        let typed_array: Get = crate::builtins::typed_array::object::typed_array_exotic_get;
        let arguments: Get = crate::builtins::function::arguments::arguments_exotic_get;
        !self.numeric_name
            && (std::ptr::fn_addr_eq(get, typed_array) || std::ptr::fn_addr_eq(get, arguments))
    }

    pub(crate) fn set(&self, shape: &Shape, slot: Slot) {
        if self.megamorphic.get() {
            return;
        }

        let mut entries = self.entries.borrow_mut();

        // Add a new entry if there's space.
        if entries
            .try_push(CacheEntry {
                shape: shape.into(),
                slot,
            })
            .is_err()
        {
            // Polymorphic cache is full, transition to megamorphic.
            self.megamorphic.set(true);
            entries.clear();
        }
    }

    /// Returns the cached `(Shape, Slot)` if a matching shape exists in the inline cache.
    ///
    /// Opportunistically cleans up stale weak shape references during lookup.
    pub(crate) fn get(&self, shape: &Shape) -> Option<(Shape, Slot)> {
        if self.megamorphic.get() {
            return None;
        }

        let mut entries = self.entries.borrow_mut();
        let mut i = 0;
        let mut result = None;
        let shape_addr = shape.to_addr_usize();

        while i < entries.len() {
            if let Some(upgraded) = entries[i].shape.upgrade() {
                if upgraded.to_addr_usize() == shape_addr {
                    result = Some((upgraded, entries[i].slot));
                    break;
                }
                i += 1;
            } else {
                // Opportunistically clean up stale weak shapes.
                entries.swap_remove(i);
            }
        }

        result
    }
}
