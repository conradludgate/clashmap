use crate::lock::RwLock;
use crate::ClashMap;
use crate::ClashTable;
use crate::HashMap;
use core::fmt;
use core::hash::{BuildHasher, Hash};
use crossbeam_utils::CachePadded;
use hashbrown::Equivalent;
use std::collections::hash_map::RandomState;
use std::hash::Hasher;

/// A read-only view into a `ClashMap`. Allows to obtain raw references to the stored values.
pub struct ReadOnlyView<K, V, S = RandomState> {
    shift: usize,
    // It is necessary to re-alloc the shards here
    // to allow ReadOnlyView to be covariant over K and V
    pub(crate) shards: Box<[HashMap<K, V>]>,
    hasher: S,
}

impl<K: Eq + Hash + Clone, V: Clone, S: Clone> Clone for ReadOnlyView<K, V, S> {
    fn clone(&self) -> Self {
        Self {
            shards: self.shards.clone(),
            hasher: self.hasher.clone(),
            shift: self.shift,
        }
    }
}

impl<K: Eq + Hash + fmt::Debug, V: fmt::Debug, S: BuildHasher> fmt::Debug
    for ReadOnlyView<K, V, S>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K, V, S> ReadOnlyView<K, V, S> {
    pub(crate) fn new(map: ClashMap<K, V, S>) -> Self {
        Self {
            shards: map
                .table
                .shards
                .into_vec()
                .into_iter()
                .map(|s| s.into_inner().into_inner())
                .collect(),
            shift: map.table.shift,
            hasher: map.hasher,
        }
    }

    /// Consumes this `ReadOnlyView`, returning the underlying `ClashMap`.
    pub fn into_inner(self) -> ClashMap<K, V, S> {
        ClashMap {
            table: ClashTable {
                shards: self
                    .shards
                    .into_vec()
                    .into_iter()
                    .map(|s| CachePadded::new(RwLock::new(s)))
                    .collect(),
                shift: self.shift,
            },
            hasher: self.hasher,
        }
    }
}

impl<'a, K: 'a + Eq + Hash, V: 'a, S: BuildHasher> ReadOnlyView<K, V, S> {
    fn hash_u64<T: Hash>(&self, item: &T) -> u64 {
        let mut hasher = self.hasher.build_hasher();

        item.hash(&mut hasher);

        hasher.finish()
    }

    fn _determine_shard(&self, hash: usize) -> usize {
        // Leave the high 7 bits for the HashBrown SIMD tag.
        let idx = (hash << 7) >> self.shift;

        // hint to llvm that the panic bounds check can be removed
        if idx >= self.shards.len() {
            if cfg!(debug_assertions) {
                unreachable!("invalid shard index")
            } else {
                // SAFETY: shards is always a power of two,
                // and shift is calculated such that the resulting idx is always
                // less than the shards length
                unsafe {
                    std::hint::unreachable_unchecked();
                }
            }
        }

        idx
    }

    /// Returns the number of elements in the map.
    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.len()).sum()
    }

    /// Returns `true` if the map contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of elements the map can hold without reallocating.
    pub fn capacity(&self) -> usize {
        self.shards.iter().map(|s| s.capacity()).sum()
    }

    /// Returns `true` if the map contains a value for the specified key.
    pub fn contains_key<Q>(&'a self, key: &Q) -> bool
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.get(key).is_some()
    }

    /// Returns a reference to the value corresponding to the key.
    pub fn get<Q>(&'a self, key: &Q) -> Option<&'a V>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.get_key_value(key).map(|(_k, v)| v)
    }

    /// Returns the key-value pair corresponding to the supplied key.
    pub fn get_key_value<Q>(&'a self, key: &Q) -> Option<(&'a K, &'a V)>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        let hash = self.hash_u64(&key);
        let idx = self._determine_shard(hash as usize);

        self.shards[idx]
            .find(hash, |(k, _v)| key.equivalent(k))
            .map(|(k, v)| (k, v))
    }

    /// An iterator visiting all key-value pairs in arbitrary order. The iterator element type is `(&'a K, &'a V)`.
    pub fn iter(&'a self) -> impl Iterator<Item = (&'a K, &'a V)> {
        self.shards
            .iter()
            .flat_map(|shard| shard.iter())
            .map(|(k, v)| (k, v))
    }

    /// An iterator visiting all keys in arbitrary order. The iterator element type is `&'a K`.
    pub fn keys(&'a self) -> impl Iterator<Item = &'a K> + 'a {
        self.iter().map(|(k, _v)| k)
    }

    /// An iterator visiting all values in arbitrary order. The iterator element type is `&'a V`.
    pub fn values(&'a self) -> impl Iterator<Item = &'a V> + 'a {
        self.iter().map(|(_k, v)| v)
    }

    #[cfg(feature = "raw-api")]
    /// Allows you to peek at the inner shards that store your data.
    /// You should probably not use this unless you know what you are doing.
    ///
    /// Requires the `raw-api` feature to be enabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use clashmap::ClashMap;
    ///
    /// let map = ClashMap::<(), ()>::new().into_read_only();
    /// println!("Amount of shards: {}", map.shards().len());
    /// ```
    pub fn shards(&self) -> &[HashMap<K, V>] {
        &self.shards
    }
}

#[cfg(test)]
mod tests {
    use crate::ClashMap;

    fn construct_sample_map() -> ClashMap<i32, String> {
        let map = ClashMap::new();

        map.insert(1, "one".to_string());

        map.insert(10, "ten".to_string());

        map.insert(27, "twenty seven".to_string());

        map.insert(45, "forty five".to_string());

        map
    }

    #[test]

    fn test_properties() {
        let map = construct_sample_map();

        let view = map.clone().into_read_only();

        assert_eq!(view.is_empty(), map.is_empty());

        assert_eq!(view.len(), map.len());

        assert_eq!(view.capacity(), map.capacity());

        let new_map = view.into_inner();

        assert_eq!(new_map.is_empty(), map.is_empty());

        assert_eq!(new_map.len(), map.len());

        assert_eq!(new_map.capacity(), map.capacity());
    }

    #[test]

    fn test_get() {
        let map = construct_sample_map();

        let view = map.clone().into_read_only();

        for key in map.iter().map(|entry| *entry.key()) {
            assert!(view.contains_key(&key));

            let map_entry = map.get(&key).unwrap();

            assert_eq!(view.get(&key).unwrap(), map_entry.value());

            let key_value: (&i32, &String) = view.get_key_value(&key).unwrap();

            assert_eq!(key_value.0, map_entry.key());

            assert_eq!(key_value.1, map_entry.value());
        }
    }

    #[test]

    fn test_iters() {
        let map = construct_sample_map();

        let view = map.clone().into_read_only();

        let mut visited_items = Vec::new();

        for (key, value) in view.iter() {
            map.contains_key(key);

            let map_entry = map.get(key).unwrap();

            assert_eq!(key, map_entry.key());

            assert_eq!(value, map_entry.value());

            visited_items.push((key, value));
        }

        let mut visited_keys = Vec::new();

        for key in view.keys() {
            map.contains_key(key);

            let map_entry = map.get(key).unwrap();

            assert_eq!(key, map_entry.key());

            assert_eq!(view.get(key).unwrap(), map_entry.value());

            visited_keys.push(key);
        }

        let mut visited_values = Vec::new();

        for value in view.values() {
            visited_values.push(value);
        }

        for entry in map.iter() {
            let key = entry.key();

            let value = entry.value();

            assert!(visited_keys.contains(&key));

            assert!(visited_values.contains(&value));

            assert!(visited_items.contains(&(key, value)));
        }
    }
}
