//! An element's attributes, in the order they were added (DOM §4.9: an
//! element has an attribute *list*).
//!
//! The order is observable: `attributes[i]`, `getAttributeNames()`,
//! `outerHTML` and every serializer write it, and Chrome keeps the order
//! the parser saw or script set. Setting a name the element has keeps its
//! place; removing it and setting it again puts it last.
//!
//! A vector of pairs with a linear search by name. Elements have few
//! attributes (the browser's own chrome page averages 1.4, a list-heavy
//! page six), and comparing a handful of short names is no slower than
//! hashing one: see `attr_cost_bench` for the measured parse and lookup
//! times against the `HashMap` this replaced. The method names are that
//! map's, so the callers read as before.

/// Attribute names to values, in insertion order.
#[derive(Debug, Clone, Default)]
pub struct AttrMap {
    pairs: Vec<(String, String)>,
}

/// The same names with the same values, in any order: what the map this
/// replaced compared, and what DOM `isEqualNode` asks (DOM §4.4).
impl PartialEq for AttrMap {
    fn eq(&self, other: &Self) -> bool {
        self.pairs.len() == other.pairs.len()
            && self.pairs.iter().all(|(name, value)| other.get(name) == Some(value))
    }
}

impl Eq for AttrMap {}

impl AttrMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self { pairs: Vec::with_capacity(capacity) }
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    fn position(&self, name: &str) -> Option<usize> {
        self.pairs.iter().position(|(n, _)| n == name)
    }

    pub fn get(&self, name: &str) -> Option<&String> {
        self.pairs.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut String> {
        self.pairs.iter_mut().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.position(name).is_some()
    }

    /// Set `name`: a name already here keeps its place and takes the new
    /// value (the old one is returned); a new name goes last.
    pub fn insert(&mut self, name: String, value: String) -> Option<String> {
        match self.position(&name) {
            Some(at) => Some(std::mem::replace(&mut self.pairs[at].1, value)),
            None => {
                self.pairs.push((name, value));
                None
            }
        }
    }

    /// Remove `name`; the attributes after it keep their order.
    pub fn remove(&mut self, name: &str) -> Option<String> {
        self.position(name).map(|at| self.pairs.remove(at).1)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&String, &mut String) -> bool) {
        self.pairs.retain_mut(|(n, v)| keep(n, v));
    }

    pub fn clear(&mut self) {
        self.pairs.clear();
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&String, &String)> + ExactSizeIterator + Clone {
        self.pairs.iter().map(|(n, v)| (n, v))
    }

    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &String> + ExactSizeIterator + Clone {
        self.pairs.iter().map(|(n, _)| n)
    }

    pub fn values(&self) -> impl DoubleEndedIterator<Item = &String> + ExactSizeIterator + Clone {
        self.pairs.iter().map(|(_, v)| v)
    }
}

/// Collected in order; a name given twice keeps its first place and its
/// last value (the HTML tokenizer has already dropped a tag's duplicates).
impl FromIterator<(String, String)> for AttrMap {
    fn from_iter<I: IntoIterator<Item = (String, String)>>(iter: I) -> Self {
        let iter = iter.into_iter();
        let mut map = Self::with_capacity(iter.size_hint().0);
        for (name, value) in iter {
            map.insert(name, value);
        }
        map
    }
}

impl Extend<(String, String)> for AttrMap {
    fn extend<I: IntoIterator<Item = (String, String)>>(&mut self, iter: I) {
        for (name, value) in iter {
            self.insert(name, value);
        }
    }
}

impl IntoIterator for AttrMap {
    type Item = (String, String);
    type IntoIter = std::vec::IntoIter<(String, String)>;
    fn into_iter(self) -> Self::IntoIter {
        self.pairs.into_iter()
    }
}

impl<'a> IntoIterator for &'a AttrMap {
    type Item = (&'a String, &'a String);
    type IntoIter = std::iter::Map<std::slice::Iter<'a, (String, String)>, fn(&'a (String, String)) -> (&'a String, &'a String)>;
    fn into_iter(self) -> Self::IntoIter {
        self.pairs.iter().map(|(n, v)| (n, v))
    }
}

/// From a map with no order of its own: by name, so the result is the same
/// every time.
impl From<std::collections::HashMap<String, String>> for AttrMap {
    fn from(map: std::collections::HashMap<String, String>) -> Self {
        let mut pairs: Vec<(String, String)> = map.into_iter().collect();
        pairs.sort();
        Self { pairs }
    }
}

impl<const N: usize> From<[(String, String); N]> for AttrMap {
    fn from(pairs: [(String, String); N]) -> Self {
        pairs.into_iter().collect()
    }
}

impl std::ops::Index<&str> for AttrMap {
    type Output = String;
    fn index(&self, name: &str) -> &String {
        self.get(name).expect("no such attribute")
    }
}

#[cfg(test)]
mod tests {
    use super::AttrMap;

    /// R1 LOW: building a long list must not be quadratic. 100,000 distinct
    /// names are a 1.5 MB start tag, which a generated or hostile page can
    /// send; scanning the list for each one was 10^10 comparisons.
    #[test]
    fn a_hundred_thousand_attributes_build_in_linear_time() {
        let started = std::time::Instant::now();
        let mut map = AttrMap::new();
        for i in 0..100_000 {
            map.insert(format!("data-a{i}"), String::new());
        }
        assert_eq!(map.len(), 100_000);
        assert_eq!(map.get("data-a99999").map(String::as_str), Some(""));
        assert!(map.get("data-missing").is_none());
        assert_eq!(map.remove("data-a5").as_deref(), Some(""));
        assert!(!map.contains_key("data-a5"));
        assert_eq!(map.keys().next().map(String::as_str), Some("data-a0"));
        let took = started.elapsed();
        assert!(took < std::time::Duration::from_secs(5), "100,000 inserts took {took:?}");
    }

    fn names(map: &AttrMap) -> String {
        map.keys().cloned().collect::<Vec<_>>().join(",")
    }

    #[test]
    fn the_order_is_the_order_of_insertion() {
        let mut map: AttrMap =
            [("z", "1"), ("a", "2"), ("m", "3")].into_iter().map(|(n, v)| (n.to_string(), v.to_string())).collect();
        assert_eq!(names(&map), "z,a,m");
        // An existing name keeps its place.
        assert_eq!(map.insert("z".into(), "again".into()).as_deref(), Some("1"));
        assert_eq!(names(&map), "z,a,m");
        assert_eq!(map.get("z").map(String::as_str), Some("again"));
        // Removed and set again: last.
        assert_eq!(map.remove("a").as_deref(), Some("2"));
        assert_eq!(map.insert("a".into(), "back".into()), None);
        assert_eq!(names(&map), "z,m,a");
        assert_eq!(map.len(), 3);
        assert!(map.contains_key("m") && !map.contains_key("q"));
    }
}
