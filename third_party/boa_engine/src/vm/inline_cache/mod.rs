use arrayvec::ArrayVec;
use itertools::Itertools;
use std::{cell::Cell, fmt};

use boa_gc::GcRefCell;
use boa_macros::{Finalize, Trace};

use crate::{
    JsString,
    JsValue,
    object::{
        PropertyMap,
        shape::{
            Shape, WeakShape,
            slot::{Slot, SlotAttributes},
        },
    },
    property::PropertyKey,
};

/// HIWAVE PATCH (ic-hit-read): what `InlineCache::get` returns for a shape that has an entry. It
/// has not looked at the prototype: a hit reads the slot with [`Hit::read`], which does, once, at
/// the point where the hit path borrows the prototype to read the slot anyway.
#[derive(Debug)]
pub(crate) struct Hit {
    /// The receiver's shape the entry is keyed on.
    shape: Shape,
    /// Where the property is: `PROTOTYPE` means the receiver's immediate prototype.
    pub(crate) slot: Slot,
    /// For a prototype slot, the address of the prototype's shape when the entry was made.
    holder: Option<usize>,
}

impl Hit {
    /// Reads `storage[slot.index + extra]` (`extra` is 1 for an accessor's setter) from the object
    /// that holds the slot: `own`, the receiver's properties, or its immediate prototype.
    /// `None` when there is no such slot, or when the entry is stale: the prototype's shape is no
    /// longer the one the entry was made under, so its property may have been deleted, redefined or
    /// made read-only; the entry is dropped and the caller takes the full path.
    #[inline(always)]
    pub(crate) fn read(&self, ic: &InlineCache, own: &PropertyMap, extra: usize) -> Option<JsValue> {
        let index = self.slot.index as usize + extra;
        let Some(holder) = self.holder else {
            return own.storage.get(index).cloned();
        };
        let prototype = self.shape.prototype()?;
        let prototype = prototype.borrow();
        if prototype.shape().to_addr_usize() != holder {
            ic.evict(&self.shape);
            return None;
        }
        prototype.properties().storage.get(index).cloned()
    }
}

#[cfg(test)]
mod tests;

pub(crate) const PIC_CAPACITY: usize = 4;

/// A cached shape-to-slot mapping for a polymorphic inline cache.
#[derive(Clone, Debug, Trace, Finalize)]
pub(crate) struct CacheEntry {
    /// A weak reference is kept to the shape to avoid the shape preventing deallocation.
    pub(crate) shape: WeakShape,
    // HIWAVE PATCH (ic-holder): for a slot found on the receiver's prototype, the prototype's
    // shape when the entry was made. The entry is keyed on the receiver's shape alone, which does
    // not change when the prototype's properties do; a hit also requires the prototype's shape now
    // to be this one, so a deleted or redefined prototype property (which gives the prototype a
    // new shape) misses instead of indexing a slot that has moved or gone. Held strongly, so its
    // address cannot be reused by another shape while the entry exists, and the check on a hit
    // is one address comparison; a stale entry is dropped on its next lookup.
    pub(crate) holder: Option<Shape>,
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
        Self {
            name,
            entries: GcRefCell::new(ArrayVec::new()),
            megamorphic: Cell::new(false),
        }
    }

    pub(crate) fn set(&self, shape: &Shape, slot: Slot) {
        if self.megamorphic.get() {
            return;
        }

        // HIWAVE PATCH (ic-holder): a prototype slot is cached with the prototype's shape (see
        // `CacheEntry::holder`). Not on a receiver in dictionary mode: a unique shape takes a new
        // own property in place, without a new shape, so an entry keyed on it would keep answering
        // from the prototype after the receiver gains an own property that shadows it.
        let holder = if slot.attributes.contains(SlotAttributes::PROTOTYPE) {
            if shape.is_unique() {
                return;
            }
            let Some(prototype) = shape.prototype() else {
                return;
            };
            let prototype = prototype.borrow();
            Some(prototype.shape().clone())
        } else {
            None
        };

        // HIWAVE PATCH (ic-holder): the slot was taken when the lookup found the property, but
        // the shapes are taken now, after any getter or setter it ran; one that redefined or
        // deleted its own property leaves a slot that the current shape no longer has there. Cache
        // only if the name still resolves to that slot, with the same attributes, in the shape of
        // the object that holds it.
        let key = PropertyKey::from(self.name.clone());
        let found = holder.as_ref().unwrap_or(shape).lookup(&key);
        let same = |a: SlotAttributes| a.difference(SlotAttributes::INLINE_CACHE_BITS);
        if !found.is_some_and(|found| {
            found.index == slot.index && same(found.attributes) == same(slot.attributes)
        }) {
            return;
        }
        // HIWAVE PATCH (ic-holder): and for a prototype slot, the receiver must still lack the
        // name. A prototype accessor that defines it on the receiver (a memoising getter) gives the
        // receiver a new shape that has it; an entry keyed on that shape would skip the own property.
        if holder.is_some() && shape.lookup(&key).is_some() {
            return;
        }

        let mut entries = self.entries.borrow_mut();

        // Add a new entry if there's space.
        if entries
            .try_push(CacheEntry {
                shape: shape.into(),
                holder,
                slot,
            })
            .is_err()
        {
            // Polymorphic cache is full, transition to megamorphic.
            self.megamorphic.set(true);
            entries.clear();
        }
    }

    /// HIWAVE PATCH (ic-hit-read): drops the entry for `shape` if it is a prototype entry (the only
    /// kind that goes stale without its own shape changing; see `CacheEntry::holder`).
    #[cold]
    fn evict(&self, shape: &Shape) {
        let addr = shape.to_addr_usize();
        let mut entries = self.entries.borrow_mut();
        if let Some(i) = entries.iter().position(|e| {
            e.holder.is_some() && e.shape.upgrade().is_some_and(|s| s.to_addr_usize() == addr)
        }) {
            entries.swap_remove(i);
        }
    }

    /// Returns the cached `(Shape, Slot)` if a matching shape exists in the inline cache.
    ///
    /// Opportunistically cleans up stale weak shape references during lookup.
    pub(crate) fn get(&self, shape: &Shape) -> Option<Hit> {
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
                    // HIWAVE PATCH (ic-holder, ic-hit-read): a prototype entry is good only while
                    // the prototype still has the shape it had when the entry was made. That is
                    // checked in `Hit::read`, where the hit path borrows the prototype to read the
                    // slot; every read of a cached slot goes through it.
                    result = Some(Hit {
                        shape: upgraded,
                        slot: entries[i].slot,
                        holder: entries[i].holder.as_ref().map(Shape::to_addr_usize),
                    });
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
