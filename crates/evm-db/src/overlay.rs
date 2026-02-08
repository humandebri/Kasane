//! どこで: OverlayDB / 何を: BTreeMapによる決定的commit / なぜ: commit順序凍結のため

use std::collections::BTreeMap;

pub struct OverlayMap<K, V> {
    writes: BTreeMap<K, Option<V>>,
}

impl<K: Ord, V> OverlayMap<K, V> {
    pub fn new() -> Self {
        Self {
            writes: BTreeMap::new(),
        }
    }

    pub fn get(&self, key: &K) -> Option<&Option<V>> {
        self.writes.get(key)
    }

    pub fn set(&mut self, key: K, value: V) {
        self.writes.insert(key, Some(value));
    }

    pub fn delete(&mut self, key: K) {
        self.writes.insert(key, None);
    }

    pub fn commit_to<M>(&self, mut apply: M)
    where
        M: FnMut(&K, &Option<V>),
    {
        for (key, value) in self.writes.iter() {
            apply(key, value);
        }
    }

    pub fn drain_to<M>(&mut self, mut apply: M)
    where
        M: FnMut(K, Option<V>),
    {
        let writes = std::mem::take(&mut self.writes);
        for (key, value) in writes {
            apply(key, value);
        }
    }
}

impl<K: Ord, V> Default for OverlayMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}
