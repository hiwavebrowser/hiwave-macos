//! The bounded per-thread memos behind text measurement
//! (`measure_text_with_spacing`) and shaping (`TextShaper::shape`).
//!
//! Each memo holds at most its cap of entries. When it is full, a miss is
//! admitted only in place of an entry that has not been used since the clock
//! hand last passed it (CLOCK, second chance): the hand looks at ONE slot per
//! miss; a slot used since the last look gets its mark cleared and the new
//! string is not kept in the clock; an unused one is replaced by it.
//!
//! A string the clock turns away goes to a small FIFO of recent strings
//! (`cap / 16`). Layout asks for the same string several times in a row
//! (the forced-layout fixtures: about ten lookups per text per pass); without
//! the FIFO each of those lookups missed again and moved the hand again, so
//! the hand lapped the clock within a pass and evicted the entries it was
//! meant to keep.
//!
//! Why not clear when full (the old rule): a layout pass over more distinct
//! strings than the cap filled the memo, wiped it, refilled it, so the next
//! pass found almost nothing and re-shaped every string (forced layout per
//! element went from 5.8 to 19.9 us on a Mac between 16,300 and 16,500 text
//! rows, TEXT-CACHE-STEP). Why not LRU: a pass is a cyclic scan, and any
//! least-recently-used order misses every string of a scan longer than its
//! capacity. Here the strings already held keep being hit and are kept, so a
//! pass over `n` distinct strings hits `cap` of them and shapes about
//! `n - cap`, for `n` up to about twice the cap (the hand needs a whole pass
//! to come round; a pass with more than `cap` distinct misses laps it and the
//! hits fall away).
//!
//! A page whose strings all differ from the ones held (a navigation after a
//! text-heavy page) refills it within about one lap: the first lap clears the
//! old marks, the second replaces them.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::rc::Rc;

/// A bounded memo with CLOCK eviction (see the module documentation).
///
/// Memory: at most `cap + cap / 16` entries, each one key (shared by the map
/// and the clock ring or FIFO through an `Rc`: 16 bytes of counts and 8 of
/// ring pointer on top of the key), one value and one mark. The old
/// clear-when-full memo held `cap` keys and values without the `Rc` and the
/// ring.
pub(crate) struct TextMemo<K, V> {
    entries: HashMap<Rc<K>, Entry<V>>,
    /// The keys in clock order; `ring.len() == entries.len()`.
    ring: Vec<Rc<K>>,
    hand: usize,
    cap: usize,
    /// The last strings the clock turned away, kept for their next few
    /// lookups (`recent_cap` at most, oldest dropped first; see the module
    /// documentation).
    recent: HashMap<Rc<K>, V>,
    recent_order: VecDeque<Rc<K>>,
    recent_cap: usize,
    /// The `@font-face` set generation the entries were made under.
    generation: u64,
}

struct Entry<V> {
    value: V,
    /// Used since the hand last passed this slot.
    referenced: bool,
}

impl<K: Hash + Eq, V: Clone> TextMemo<K, V> {
    /// A memo of at most `cap` entries plus `cap / 16` recently turned-away
    /// ones.
    pub(crate) fn new(cap: usize) -> Self {
        assert!(cap > 0);
        Self {
            entries: HashMap::new(),
            ring: Vec::new(),
            hand: 0,
            cap,
            recent: HashMap::new(),
            recent_order: VecDeque::new(),
            recent_cap: (cap / 16).max(1),
            generation: 0,
        }
    }

    /// Drop every entry when the installed `@font-face` set has changed
    /// since they were made: a family name can measure differently then.
    pub(crate) fn sync_generation(&mut self, generation: u64) {
        if self.generation != generation {
            self.entries.clear();
            self.ring.clear();
            self.hand = 0;
            self.recent.clear();
            self.recent_order.clear();
            self.generation = generation;
        }
    }

    pub(crate) fn get(&mut self, key: &K) -> Option<V> {
        if memos_off() {
            return None;
        }
        if let Some(entry) = self.entries.get_mut(key) {
            entry.referenced = true;
            return Some(entry.value.clone());
        }
        self.recent.get(key).cloned()
    }

    /// Keep `value` for `key` if there is room, or if the slot under the hand
    /// has not been used since the hand last passed it (see the module
    /// documentation). Either way the hand moves on by one slot.
    pub(crate) fn insert(&mut self, key: K, value: V) {
        if memos_off() {
            return;
        }
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.value = value;
            entry.referenced = true;
            return;
        }
        let key = Rc::new(key);
        if self.ring.len() < self.cap {
            self.ring.push(Rc::clone(&key));
            self.entries.insert(
                key,
                Entry {
                    value,
                    referenced: true,
                },
            );
            return;
        }
        let slot = self.hand;
        self.hand = (self.hand + 1) % self.ring.len();
        let victim = self
            .entries
            .get_mut(&*self.ring[slot])
            .expect("every ring key has an entry");
        if victim.referenced {
            victim.referenced = false;
            if self.recent_order.len() >= self.recent_cap {
                if let Some(old) = self.recent_order.pop_front() {
                    self.recent.remove(&*old);
                }
            }
            self.recent_order.push_back(Rc::clone(&key));
            self.recent.insert(key, value);
            return;
        }
        self.entries.remove(&*self.ring[slot]);
        self.ring[slot] = Rc::clone(&key);
        self.entries.insert(
            key,
            Entry {
                value,
                referenced: true,
            },
        );
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
thread_local! {
    static MEMOS_OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[inline]
fn memos_off() -> bool {
    #[cfg(test)]
    {
        MEMOS_OFF.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// Test-only: run `f` with every text memo on this thread bypassed (no
/// lookups, no inserts), so a result can be compared with the cached one.
#[cfg(test)]
pub(crate) fn without_text_memos<R>(f: impl FnOnce() -> R) -> R {
    MEMOS_OFF.with(|c| c.set(true));
    let out = f();
    MEMOS_OFF.with(|c| c.set(false));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One pass over `keys`: returns the misses (each miss is then inserted,
    /// as the callers do).
    fn pass(memo: &mut TextMemo<u32, u32>, keys: impl Iterator<Item = u32>) -> usize {
        let mut misses = 0;
        for k in keys {
            if memo.get(&k).is_none() {
                misses += 1;
                memo.insert(k, k * 2);
            }
        }
        misses
    }

    #[test]
    fn a_pass_over_fewer_strings_than_the_cap_misses_none_the_second_time() {
        let mut memo = TextMemo::new(8);
        assert_eq!(pass(&mut memo, 0..8), 8);
        assert_eq!(pass(&mut memo, 0..8), 0);
        assert_eq!(memo.get(&3), Some(6));
    }

    #[test]
    fn a_pass_over_more_strings_than_the_cap_keeps_the_held_ones() {
        // 40 distinct keys through 32 slots (and a FIFO of 2), pass after
        // pass: the 32 held stay hits, only the 8 that do not fit are shaped
        // again (clear-when-full re-shaped all 40 every pass).
        let mut memo = TextMemo::new(32);
        assert_eq!(pass(&mut memo, 0..40), 40);
        for _ in 0..5 {
            assert_eq!(pass(&mut memo, 0..40), 8);
            assert_eq!(memo.len(), 32);
        }
    }

    #[test]
    fn a_string_looked_up_again_right_after_it_was_turned_away_is_a_hit() {
        // Layout asks for the same text several times in a row. A turned-away
        // string is served from the FIFO, and the repeats do not move the
        // hand, so the held entries survive.
        let mut memo = TextMemo::new(32);
        let repeated = |k: u32| std::iter::repeat(k).take(10);
        assert_eq!(pass(&mut memo, (0..40).flat_map(repeated)), 40);
        for _ in 0..5 {
            assert_eq!(pass(&mut memo, (0..40).flat_map(repeated)), 8);
            assert_eq!(memo.len(), 32);
        }
    }

    #[test]
    fn strings_no_longer_used_are_replaced_within_two_laps() {
        let mut memo = TextMemo::new(32);
        pass(&mut memo, 0..32);
        // A new set of 8 (more than the FIFO's 2): the first lap clears the
        // old marks, the second replaces old entries; after that every new
        // key is held.
        let mut rounds = 0;
        while pass(&mut memo, 100..108) > 0 {
            rounds += 1;
            assert!(rounds <= 16, "new keys never admitted");
        }
        for k in 100..108 {
            assert_eq!(memo.get(&k), Some(k * 2));
        }
        assert_eq!(memo.len(), 32);
    }

    #[test]
    fn a_new_font_generation_drops_every_entry() {
        let mut memo = TextMemo::new(8);
        memo.sync_generation(1);
        pass(&mut memo, 0..4);
        memo.sync_generation(1);
        assert_eq!(memo.len(), 4);
        memo.sync_generation(2);
        assert_eq!(memo.len(), 0);
        assert_eq!(pass(&mut memo, 0..4), 4);
    }

    #[test]
    fn the_memo_never_holds_more_than_its_cap() {
        let mut memo = TextMemo::new(32);
        for round in 0..20u32 {
            pass(&mut memo, round * 7..round * 7 + 45);
            assert!(memo.len() <= 32);
            assert_eq!(memo.ring.len(), memo.entries.len());
            assert!(memo.recent.len() <= 2);
            assert_eq!(memo.recent_order.len(), memo.recent.len());
        }
    }

    #[test]
    fn bypassed_memos_neither_answer_nor_keep() {
        let mut memo = TextMemo::new(8);
        pass(&mut memo, 0..2);
        without_text_memos(|| {
            assert_eq!(memo.get(&0), None);
            memo.insert(5, 10);
        });
        assert_eq!(memo.get(&0), Some(0));
        assert_eq!(memo.get(&5), None);
    }

    #[test]
    fn the_test_switch_is_restored_when_the_closure_panics() {
        let mut memo = TextMemo::new(8);
        let caught = std::panic::catch_unwind(|| without_text_memos(|| panic!("inside")));
        assert!(caught.is_err());
        // The memos answer again on this thread.
        memo.insert(1, 2);
        assert_eq!(memo.get(&1), Some(2));
    }

    #[test]
    fn a_string_re_read_from_the_queue_moves_into_an_unused_slot() {
        // The memo is full of strings the page no longer uses: a lap of
        // misses has cleared every mark. A string the clock turned away and
        // the page keeps reading is promoted into the main table instead of
        // living in the queue.
        let mut memo = TextMemo::new(32);
        pass(&mut memo, 0..32);
        // 32 misses, each turned away: their looks clear all 32 marks.
        pass(&mut memo, 100..132);
        assert_eq!(memo.get(&131), Some(262));
        assert!(
            memo.entries.contains_key(&131),
            "a re-read turned-away string stayed in the queue while the slot under the hand was unused"
        );
        assert_eq!(memo.len(), 32);
    }
}
