use super::mapref::multiple::{RefMulti, RefMutMulti};
use super::util;
use crate::lock::{RwLockReadGuard, RwLockWriteGuard};
use crate::t::Map;
use crate::util::SharedValue;
use crate::{DashMap, HashTable};
use core::hash::{BuildHasher, Hash};
use core::mem;
use hashbrown::hash_table;
use std::collections::hash_map::RandomState;
use std::sync::Arc;

/// Iterator over a DashMap yielding key value pairs.
///
/// # Examples
///
/// ```
/// use dashmap::DashMap;
///
/// let map = DashMap::new();
/// map.insert("hello", "world");
/// map.insert("alex", "steve");
/// let pairs: Vec<(&'static str, &'static str)> = map.into_iter().collect();
/// assert_eq!(pairs.len(), 2);
/// ```
pub struct OwningIter<K, V, S = RandomState> {
    map: DashMap<K, V, S>,
    shard_i: usize,
    current: Option<GuardOwningIter<K, V>>,
}

impl<K: Eq + Hash, V, S: BuildHasher> OwningIter<K, V, S> {
    pub(crate) fn new(map: DashMap<K, V, S>) -> Self {
        Self {
            map,
            shard_i: 0,
            current: None,
        }
    }
}

type GuardOwningIter<K, V> = hash_table::IntoIter<(K, SharedValue<V>)>;

impl<K: Eq + Hash, V, S: BuildHasher> Iterator for OwningIter<K, V, S> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some((k, v)) = current.next() {
                    return Some((k, v.into_inner()));
                }
            }

            if self.shard_i == self.map._shard_count() {
                return None;
            }

            //let guard = unsafe { self.map._yield_read_shard(self.shard_i) };
            let mut shard_wl = unsafe { self.map._yield_write_shard(self.shard_i) };

            let map = mem::replace(&mut *shard_wl, HashTable::new());

            drop(shard_wl);

            let iter = map.into_iter();

            //unsafe { ptr::write(&mut self.current, Some((arcee, iter))); }
            self.current = Some(iter);

            self.shard_i += 1;
        }
    }
}

unsafe impl<K, V, S> Send for OwningIter<K, V, S>
where
    K: Eq + Hash + Send,
    V: Send,
    S: BuildHasher + Send,
{
}

unsafe impl<K, V, S> Sync for OwningIter<K, V, S>
where
    K: Eq + Hash + Sync,
    V: Sync,
    S: BuildHasher + Sync,
{
}

type GuardIter<'a, K, V> = (
    Arc<RwLockReadGuard<'a, HashTable<K, V>>>,
    hash_table::Iter<'a, (K, SharedValue<V>)>,
);

type GuardIterMut<'a, K, V> = (
    Arc<RwLockWriteGuard<'a, HashTable<K, V>>>,
    hash_table::IterMut<'a, (K, SharedValue<V>)>,
);

/// Iterator over a DashMap yielding immutable references.
///
/// # Examples
///
/// ```
/// use dashmap::DashMap;
///
/// let map = DashMap::new();
/// map.insert("hello", "world");
/// assert_eq!(map.iter().count(), 1);
/// ```
pub struct Iter<'a, K, V, M = DashMap<K, V>> {
    map: &'a M,
    shard_i: usize,
    current: Option<GuardIter<'a, K, V>>,
}

impl<'i, K: Clone + Hash + Eq, V: Clone> Clone for Iter<'i, K, V> {
    fn clone(&self) -> Self {
        Iter::new(self.map)
    }
}

unsafe impl<'a, 'i, K, V, M> Send for Iter<'i, K, V, M>
where
    K: 'a + Eq + Hash + Send,
    V: 'a + Send,
    M: Map<'a, K, V>,
{
}

unsafe impl<'a, 'i, K, V, M> Sync for Iter<'i, K, V, M>
where
    K: 'a + Eq + Hash + Sync,
    V: 'a + Sync,
    M: Map<'a, K, V>,
{
}

impl<'a, K: Eq + Hash, V, M: Map<'a, K, V>> Iter<'a, K, V, M> {
    pub(crate) fn new(map: &'a M) -> Self {
        Self {
            map,
            shard_i: 0,
            current: None,
        }
    }
}

impl<'a, K: Eq + Hash, V, M: Map<'a, K, V>> Iterator for Iter<'a, K, V, M> {
    type Item = RefMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some((k, v)) = current.1.next() {
                    let guard = current.0.clone();

                    return unsafe { Some(RefMulti::new(guard, k, v.get())) };
                }
            }

            if self.shard_i == self.map._shard_count() {
                return None;
            }

            let guard = unsafe { self.map._yield_read_shard(self.shard_i) };

            let sref: &HashTable<K, V> = unsafe { util::change_lifetime_const(&*guard) };

            let iter = sref.iter();

            self.current = Some((Arc::new(guard), iter));

            self.shard_i += 1;
        }
    }
}

/// Iterator over a DashMap yielding mutable references.
///
/// # Examples
///
/// ```
/// use dashmap::DashMap;
///
/// let map = DashMap::new();
/// map.insert("Johnny", 21);
/// map.iter_mut().for_each(|mut r| *r += 1);
/// assert_eq!(*map.get("Johnny").unwrap(), 22);
/// ```
pub struct IterMut<'a, K, V, M = DashMap<K, V>> {
    map: &'a M,
    shard_i: usize,
    current: Option<GuardIterMut<'a, K, V>>,
}

unsafe impl<'a, 'i, K, V, M> Send for IterMut<'i, K, V, M>
where
    K: 'a + Eq + Hash + Send,
    V: 'a + Send,
    M: Map<'a, K, V>,
{
}

unsafe impl<'a, 'i, K, V, M> Sync for IterMut<'i, K, V, M>
where
    K: 'a + Eq + Hash + Sync,
    V: 'a + Sync,
    M: Map<'a, K, V>,
{
}

impl<'a, K: Eq + Hash, V, M: Map<'a, K, V>> IterMut<'a, K, V, M> {
    pub(crate) fn new(map: &'a M) -> Self {
        Self {
            map,
            shard_i: 0,
            current: None,
        }
    }
}

impl<'a, K: Eq + Hash, V, M: Map<'a, K, V>> Iterator for IterMut<'a, K, V, M> {
    type Item = RefMutMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some((k, v)) = current.1.next() {
                    let guard = current.0.clone();

                    unsafe {
                        let k = util::change_lifetime_const(k);

                        let v = &mut *v.as_ptr();

                        return Some(RefMutMulti::new(guard, k, v));
                    }
                }
            }

            if self.shard_i == self.map._shard_count() {
                return None;
            }

            let mut guard = unsafe { self.map._yield_write_shard(self.shard_i) };

            let sref: &mut HashTable<K, V> = unsafe { util::change_lifetime_mut(&mut *guard) };

            let iter = sref.iter_mut();

            self.current = Some((Arc::new(guard), iter));

            self.shard_i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::DashMap;

    #[test]
    fn iter_mut_manual_count() {
        let map = DashMap::new();

        map.insert("Johnny", 21);

        assert_eq!(map.len(), 1);

        let mut c = 0;

        for shard in map.shards() {
            c += shard.write().iter_mut().count();
        }

        assert_eq!(c, 1);
    }

    #[test]
    fn iter_mut_count() {
        let map = DashMap::new();

        map.insert("Johnny", 21);

        assert_eq!(map.len(), 1);

        assert_eq!(map.iter_mut().count(), 1);
    }

    #[test]
    fn iter_count() {
        let map = DashMap::new();

        map.insert("Johnny", 21);

        assert_eq!(map.len(), 1);

        assert_eq!(map.iter().count(), 1);
    }
}
