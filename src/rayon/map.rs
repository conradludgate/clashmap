use crate::lock::{RwLock, RwLockReadGuardDetached, RwLockWriteGuardDetached};
use crate::mapref::multiple::{RefMulti, RefMutMulti};
use crate::{tableref, ClashMap, HashMap, Shard};
use core::hash::{BuildHasher, Hash};
use crossbeam_utils::CachePadded;
use rayon::iter::plumbing::UnindexedConsumer;
use rayon::iter::{FromParallelIterator, IntoParallelIterator, ParallelExtend, ParallelIterator};
use std::sync::Arc;

impl<K, V, S> ParallelExtend<(K, V)> for ClashMap<K, V, S>
where
    K: Send + Sync + Eq + Hash,
    V: Send + Sync,
    S: Send + Sync + BuildHasher,
{
    fn par_extend<I>(&mut self, par_iter: I)
    where
        I: IntoParallelIterator<Item = (K, V)>,
    {
        par_iter.into_par_iter().for_each(move |(key, value)| {
            self.insert(key, value);
        });
    }
}

// Since we don't actually need mutability, we can implement this on a
// reference, similar to `io::Write for &File`.
impl<K, V, S> ParallelExtend<(K, V)> for &'_ ClashMap<K, V, S>
where
    K: Send + Sync + Eq + Hash,
    V: Send + Sync,
    S: Send + Sync + BuildHasher,
{
    fn par_extend<I>(&mut self, par_iter: I)
    where
        I: IntoParallelIterator<Item = (K, V)>,
    {
        let &mut map = self;
        par_iter.into_par_iter().for_each(move |(key, value)| {
            map.insert(key, value);
        });
    }
}

impl<K, V, S> FromParallelIterator<(K, V)> for ClashMap<K, V, S>
where
    K: Send + Sync + Eq + Hash,
    V: Send + Sync,
    S: Send + Sync + Default + BuildHasher,
{
    fn from_par_iter<I>(par_iter: I) -> Self
    where
        I: IntoParallelIterator<Item = (K, V)>,
    {
        let map = Self::default();
        (&map).par_extend(par_iter);
        map
    }
}

// Implementation note: while the shards will iterate in parallel, we flatten
// sequentially within each shard (`flat_map_iter`), because the standard
// `HashMap` only implements `ParallelIterator` by collecting to a `Vec` first.
// There is real parallel support in the `hashbrown/rayon` feature, but we don't
// always use that map.

impl<K, V, S> IntoParallelIterator for ClashMap<K, V, S>
where
    K: Send + Eq + Hash,
    V: Send,
    S: Send + BuildHasher,
{
    type Iter = OwningIter<K, V>;
    type Item = (K, V);

    fn into_par_iter(self) -> Self::Iter {
        OwningIter {
            shards: self.table.shards,
        }
    }
}

pub struct OwningIter<K, V> {
    pub(super) shards: Box<[Shard<K, V>]>,
}

impl<K, V> ParallelIterator for OwningIter<K, V>
where
    K: Send + Eq + Hash,
    V: Send,
{
    type Item = (K, V);

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        Vec::from(self.shards)
            .into_par_iter()
            .flat_map_iter(|shard| shard.into_inner().into_inner().into_iter())
            .drive_unindexed(consumer)
    }
}

// This impl also enables `IntoParallelRefIterator::par_iter`
impl<'a, K, V, S> IntoParallelIterator for &'a ClashMap<K, V, S>
where
    K: Send + Sync + Eq + Hash,
    V: Send + Sync,
    S: Send + Sync + BuildHasher,
{
    type Iter = Iter<'a, K, V>;
    type Item = RefMulti<'a, K, V>;

    fn into_par_iter(self) -> Self::Iter {
        Iter {
            shards: &self.table.shards,
        }
    }
}

pub struct Iter<'a, K, V> {
    pub(super) shards: &'a [CachePadded<RwLock<HashMap<K, V>>>],
}

impl<'a, K, V> ParallelIterator for Iter<'a, K, V>
where
    K: Send + Sync + Eq + Hash,
    V: Send + Sync,
{
    type Item = RefMulti<'a, K, V>;

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        self.shards
            .into_par_iter()
            .flat_map_iter(|shard| {
                // SAFETY: we keep the guard alive with the shard iterator,
                // and with any refs produced by the iterator
                let (guard, shard) = unsafe { RwLockReadGuardDetached::detach_from(shard.read()) };

                let guard = Arc::new(guard);
                shard.iter().map(move |kv| {
                    let guard = Arc::clone(&guard);
                    RefMulti::new(tableref::multiple::RefMulti::new(guard, kv))
                })
            })
            .drive_unindexed(consumer)
    }
}

// This impl also enables `IntoParallelRefMutIterator::par_iter_mut`
impl<'a, K, V> IntoParallelIterator for &'a mut ClashMap<K, V>
where
    K: Send + Sync + Eq + Hash,
    V: Send + Sync,
{
    type Iter = IterMut<'a, K, V>;
    type Item = RefMutMulti<'a, K, V>;

    fn into_par_iter(self) -> Self::Iter {
        IterMut {
            shards: &self.table.shards,
        }
    }
}

impl<K, V, S> ClashMap<K, V, S>
where
    K: Send + Sync + Eq + Hash,
    V: Send + Sync,
{
    // Unlike `IntoParallelRefMutIterator::par_iter_mut`, we only _need_ `&self`.
    pub fn par_iter_mut(&self) -> IterMut<'_, K, V> {
        IterMut {
            shards: &self.table.shards,
        }
    }
}

pub struct IterMut<'a, K, V> {
    shards: &'a [CachePadded<RwLock<HashMap<K, V>>>],
}

impl<'a, K, V> ParallelIterator for IterMut<'a, K, V>
where
    K: Send + Sync + Eq + Hash,
    V: Send + Sync,
{
    type Item = RefMutMulti<'a, K, V>;

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        self.shards
            .into_par_iter()
            .flat_map_iter(|shard| {
                let (guard, shard) =
                    // SAFETY: we keep the guard alive with the shard iterator,
                    // and with any refs produced by the iterator
                    unsafe { RwLockWriteGuardDetached::detach_from(shard.write()) };

                let guard = Arc::new(guard);
                shard.iter_mut().map(move |kv| {
                    let guard = Arc::clone(&guard);
                    RefMutMulti::new(tableref::multiple::RefMutMulti::new(guard, kv))
                })
            })
            .drive_unindexed(consumer)
    }
}
