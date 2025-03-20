use hashbrown::hash_table;

use super::mapref::multiple::{RefMulti, RefMutMulti};
use crate::{tableref, ClashMap, Shard};
use core::hash::BuildHasher;

/// Iterator over a ClashMap yielding key value pairs.
///
/// # Examples
///
/// ```
/// use clashmap::ClashMap;
///
/// let map = ClashMap::new();
/// map.insert("hello", "world");
/// map.insert("alex", "steve");
/// let pairs: Vec<(&'static str, &'static str)> = map.into_iter().collect();
/// assert_eq!(pairs.len(), 2);
/// ```
pub struct OwningIter<K, V> {
    shards: std::vec::IntoIter<Shard<K, V>>,
    current: Option<GuardOwningIter<K, V>>,
}

impl<K, V> OwningIter<K, V> {
    pub(crate) fn new<S: BuildHasher>(map: ClashMap<K, V, S>) -> Self {
        Self {
            shards: map.table.tables.shards.into_vec().into_iter(),
            current: None,
        }
    }
}

type GuardOwningIter<K, V> = hash_table::IntoIter<(K, V)>;

impl<K, V> Iterator for OwningIter<K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some((k, v)) = current.next() {
                    return Some((k, v));
                }
            }

            let iter = self.shards.next()?.into_inner().into_inner().into_iter();
            self.current = Some(iter);
        }
    }
}

/// Iterator over a ClashMap yielding immutable references.
///
/// # Examples
///
/// ```
/// use clashmap::ClashMap;
///
/// let map = ClashMap::new();
/// map.insert("hello", "world");
/// assert_eq!(map.iter().count(), 1);
/// ```
pub struct Iter<'a, K, V> {
    inner: tableref::iter::Iter<'a, (K, V)>,
}

impl<K, V> Clone for Iter<'_, K, V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<'a, K: 'a, V: 'a> Iter<'a, K, V> {
    pub(crate) fn new<S>(map: &'a ClashMap<K, V, S>) -> Self {
        Self {
            inner: map.table.iter(),
        }
    }
}

impl<'a, K: 'a, V: 'a> Iterator for Iter<'a, K, V> {
    type Item = RefMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        let r = self.inner.next()?;
        Some(RefMulti::new(r))
    }
}

/// Iterator over a ClashMap yielding mutable references.
///
/// # Examples
///
/// ```
/// use clashmap::ClashMap;
///
/// let map = ClashMap::new();
/// map.insert("Johnny", 21);
/// map.iter_mut().for_each(|mut r| *r += 1);
/// assert_eq!(*map.get("Johnny").unwrap(), 22);
/// ```
pub struct IterMut<'a, K, V> {
    inner: tableref::iter::IterMut<'a, (K, V)>,
}

impl<'a, K: 'a, V: 'a> IterMut<'a, K, V> {
    pub(crate) fn new<S>(map: &'a ClashMap<K, V, S>) -> Self {
        Self {
            inner: map.table.iter_mut(),
        }
    }
}

impl<'a, K: 'a, V: 'a> Iterator for IterMut<'a, K, V> {
    type Item = RefMutMulti<'a, K, V>;

    fn next(&mut self) -> Option<Self::Item> {
        let r = self.inner.next()?;
        Some(RefMutMulti::new(r))
    }
}

#[cfg(test)]
mod tests {
    use crate::ClashMap;

    #[test]
    fn iter_mut_manual_count() {
        let map = ClashMap::new();

        map.insert("Johnny", 21);

        assert_eq!(map.len(), 1);

        let mut c = 0;

        for shard in map.table.tables.shards().iter() {
            c += shard.write().iter().count();
        }

        assert_eq!(c, 1);
    }

    #[test]
    fn into_iter_count() {
        let map = ClashMap::new();

        map.insert("Johnny", 21);
        let c = map.into_iter().count();

        assert_eq!(c, 1);
    }

    #[test]
    fn iter_mut_count() {
        let map = ClashMap::new();

        map.insert("Johnny", 21);

        assert_eq!(map.len(), 1);

        assert_eq!(map.iter_mut().count(), 1);
    }

    #[test]
    fn iter_count() {
        let map = ClashMap::new();

        map.insert("Johnny", 21);

        assert_eq!(map.len(), 1);

        assert_eq!(map.iter().count(), 1);
    }

    #[test]
    fn iter_clone() {
        let map = ClashMap::new();

        map.insert("Johnny", 21);
        map.insert("Chucky", 22);

        let mut iter = map.iter();
        iter.next();

        let iter2 = iter.clone();

        assert_eq!(iter.count(), 1);
        assert_eq!(iter2.count(), 1);
    }
}
