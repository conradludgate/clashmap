use crate::lock::{RwLock, RwLockReadGuardDetached, RwLockWriteGuardDetached};
use crate::tableref::entry::{AbsentEntry, Entry, OccupiedEntry, VacantEntry};
use crate::tableref::entrymut::{EntryMut, OccupiedEntryMut, VacantEntryMut};
use crate::tableref::iter::{Iter, IterMut, OwningIter};
use crate::tableref::multiple::RefMulti;
use crate::tableref::one::{Ref, RefMut};
use crate::try_result::TryResult;
use crate::{default_shard_amount, TryReserveError};
use core::fmt;
use crossbeam_utils::CachePadded;
use hashbrown::{hash_table, HashTable};
use std::convert::Infallible;

/// ClashTable is an implementation of a concurrent hashtable in Rust.
///
/// ClashTable tries to implement an easy to use API similar to [`hashbrown::HashTable`]
/// with some slight changes to handle concurrency.
///
/// Documentation mentioning locking behaviour acts in the reference frame of the calling thread.
/// This means that it is safe to ignore it across multiple threads.
pub struct ClashTable<T> {
    pub(crate) shift: usize,
    pub(crate) shards: Box<[CachePadded<RwLock<HashTable<T>>>]>,
}

impl<T: Clone> Clone for ClashTable<T> {
    fn clone(&self) -> Self {
        let mut inner_shards = Vec::new();

        for shard in self.shards.iter() {
            let shard = shard.read();

            inner_shards.push(CachePadded::new(RwLock::new((*shard).clone())));
        }

        Self {
            shift: self.shift,
            shards: inner_shards.into_boxed_slice(),
        }
    }
}

impl<T> Default for ClashTable<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "raw-api")]
impl<T> ClashTable<T> {
    /// Allows you to peek at the inner shards that store your data.
    /// You should probably not use this unless you know what you are doing.
    ///
    /// Requires the `raw-api` feature to be enabled.
    pub fn shards(&self) -> &[CachePadded<RwLock<HashTable<T>>>] {
        &self.shards
    }

    /// Provides mutable access to the inner shards that store your data.
    /// You should probably not use this unless you know what you are doing.
    ///
    /// Requires the `raw-api` feature to be enabled.
    pub fn shards_mut(&mut self) -> &mut [CachePadded<RwLock<HashTable<T>>>] {
        &mut self.shards
    }

    /// Consumes this `ClashTable` and returns the inner shards.
    /// You should probably not use this unless you know what you are doing.
    ///
    /// Requires the `raw-api` feature to be enabled.
    pub fn into_shards(self) -> Box<[CachePadded<RwLock<HashTable<T>>>]> {
        self.shards
    }

    /// Finds which shard a certain hash is stored in.
    ///
    /// Requires the `raw-api` feature to be enabled.
    pub fn determine_shard(&self, hash: usize) -> usize {
        self._determine_shard(hash)
    }
}

impl<T> ClashTable<T> {
    // /// Wraps this `ClashTable` into a read-only view. This view allows to obtain raw references to the stored values.
    // pub fn into_read_only(self) -> ReadOnlyView<T> {
    //     ReadOnlyView::new(self)
    // }

    /// Creates a new ClashTable with a capacity of 0.
    pub fn new() -> Self {
        ClashTable::with_capacity(0)
    }

    /// Creates a new ClashTable with a specified starting capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        ClashTable::with_capacity_and_shard_amount(capacity, default_shard_amount())
    }

    /// Creates a new ClashTable with a specified shard amount
    ///
    /// shard_amount should greater than 0 and be a power of two.
    /// If a shard_amount which is not a power of two is provided, the function will panic.
    pub fn with_shard_amount(shard_amount: usize) -> Self {
        Self::with_capacity_and_shard_amount(0, shard_amount)
    }

    /// Creates a new ClashTable with a specified starting capacity, hasher and shard_amount.
    ///
    /// shard_amount should greater than 0 and be a power of two.
    /// If a shard_amount which is not a power of two is provided, the function will panic.
    pub fn with_capacity_and_shard_amount(mut capacity: usize, shard_amount: usize) -> Self {
        assert!(shard_amount > 1);
        assert!(shard_amount.is_power_of_two());

        let shift = (usize::BITS - shard_amount.trailing_zeros()) as usize;

        if capacity != 0 {
            capacity = (capacity + (shard_amount - 1)) & !(shard_amount - 1);
        }

        let cps = capacity / shard_amount;

        let shards = (0..shard_amount)
            .map(|_| CachePadded::new(RwLock::new(HashTable::with_capacity(cps))))
            .collect();

        Self { shift, shards }
    }

    #[inline(always)]
    pub(crate) fn _determine_shard(&self, hash: usize) -> usize {
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

    /// Creates an iterator over a ClashTable yielding immutable references.
    ///
    /// **Locking behaviour:** May deadlock if called when holding a mutable reference into the map.
    pub fn iter(&self) -> Iter<'_, T> {
        Iter::new(self)
    }

    pub(crate) fn for_each(&self, mut f: impl FnMut(&T)) {
        self.fold((), |(), kv| f(kv))
    }

    pub(crate) fn fold<R>(&self, r: R, mut f: impl FnMut(R, &T) -> R) -> R {
        match self.try_fold::<R, Infallible>(r, |r, kv| Ok(f(r, kv))) {
            Ok(r) => r,
            Err(x) => match x {},
        }
    }

    #[allow(dead_code)]
    pub(crate) fn try_for_each<E>(&self, mut f: impl FnMut(&T) -> Result<(), E>) -> Result<(), E> {
        self.try_fold((), |(), kv| f(kv))
    }

    pub(crate) fn try_fold<R, E>(
        &self,
        mut r: R,
        mut f: impl FnMut(R, &T) -> Result<R, E>,
    ) -> Result<R, E> {
        for shard in self.shards.iter() {
            let shard = shard.read();
            r = shard.iter().try_fold(r, &mut f)?;
        }
        Ok(r)
    }

    /// Iterator over a ClashTable yielding mutable references.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    pub fn iter_mut(&self) -> IterMut<'_, T> {
        IterMut::new(self)
    }

    /// Get an immutable reference to an entry in the map
    ///
    /// **Locking behaviour:** May deadlock if called when holding a mutable reference into the map.
    pub fn find(&self, hash: u64, eq: impl FnMut(&T) -> bool) -> Option<Ref<'_, T>> {
        let idx = self._determine_shard(hash as usize);

        let shard = self.shards[idx].read();

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Ref`.
        let (guard, shard) = unsafe { RwLockReadGuardDetached::detach_from(shard) };

        shard.find(hash, eq).map(|entry| Ref::new(guard, entry))
    }

    /// Get a mutable reference to an entry in the map
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    pub fn find_mut(&self, hash: u64, eq: impl FnMut(&T) -> bool) -> Option<RefMut<'_, T>> {
        let idx = self._determine_shard(hash as usize);

        let shard = self.shards[idx].write();

        // SAFETY: The data will not outlive the guard, since we pass the guard to `RefMut`.
        let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(shard) };

        if let Ok(entry) = shard.find_entry(hash, eq) {
            Some(RefMut::new(guard, entry.into_mut()))
        } else {
            None
        }
    }

    /// Get an immutable reference to an entry in the map, if the shard is not locked.
    /// If the shard is locked, the function will return [TryResult::Locked].
    pub fn try_find(&self, hash: u64, eq: impl FnMut(&T) -> bool) -> TryResult<Ref<'_, T>> {
        let idx = self._determine_shard(hash as usize);

        let shard = match self.shards[idx].try_read() {
            Some(shard) => shard,
            None => return TryResult::Locked,
        };

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Ref`.
        let (guard, shard) = unsafe { RwLockReadGuardDetached::detach_from(shard) };

        if let Some(entry) = shard.find(hash, eq) {
            TryResult::Present(Ref::new(guard, entry))
        } else {
            TryResult::Absent
        }
    }

    /// Get a mutable reference to an entry in the map, if the shard is not locked.
    /// If the shard is locked, the function will return [TryResult::Locked].
    pub fn try_find_mut(&self, hash: u64, eq: impl FnMut(&T) -> bool) -> TryResult<RefMut<'_, T>> {
        let idx = self._determine_shard(hash as usize);

        let shard = match self.shards[idx].try_write() {
            Some(shard) => shard,
            None => return TryResult::Locked,
        };

        // SAFETY: The data will not outlive the guard, since we pass the guard to `RefMut`.
        let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(shard) };

        if let Ok(entry) = shard.find_entry(hash, eq) {
            TryResult::Present(RefMut::new(guard, entry.into_mut()))
        } else {
            TryResult::Absent
        }
    }

    /// Remove excess capacity to reduce memory usage.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    pub fn shrink_to_fit(&self, hasher: impl Fn(&T) -> u64) {
        self.shards.iter().for_each(|s| {
            s.write().shrink_to_fit(|t| hasher(t));
        })
    }

    /// Retain elements that whose predicates return true
    /// and discard elements whose predicates return false.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    pub fn retain(&self, mut f: impl FnMut(&mut T) -> bool) {
        self.shards.iter().for_each(|s| {
            s.write().retain(|t| f(t));
        })
    }

    /// Fetches the total number of key-value pairs stored in the map.
    ///
    /// **Locking behaviour:** May deadlock if called when holding a mutable reference into the map.
    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.read().len()).sum()
    }

    /// Checks if the map is empty or not.
    ///
    /// **Locking behaviour:** May deadlock if called when holding a mutable reference into the map.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes all key-value pairs in the map.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    pub fn clear(&self) {
        self.retain(|_| false)
    }

    /// Returns how many key-value pairs the map can store without reallocating.
    ///
    /// **Locking behaviour:** May deadlock if called when holding a mutable reference into the map.
    pub fn capacity(&self) -> usize {
        self.shards.iter().map(|s| s.read().capacity()).sum()
    }

    /// Advanced entry API that tries to mimic `std::collections::HashMap`.
    pub fn entry_mut(
        &mut self,
        hash: u64,
        eq: impl FnMut(&T) -> bool,
        hasher: impl Fn(&T) -> u64,
    ) -> EntryMut<'_, T> {
        let idx = self._determine_shard(hash as usize);

        let shard = self.shards[idx].get_mut();
        match shard.entry(hash, eq, hasher) {
            hash_table::Entry::Occupied(occupied_entry) => {
                EntryMut::Occupied(OccupiedEntryMut::new(occupied_entry))
            }
            hash_table::Entry::Vacant(vacant_entry) => {
                EntryMut::Vacant(VacantEntryMut::new(vacant_entry))
            }
        }
    }

    /// Advanced entry API that tries to mimic `std::collections::HashMap`.
    /// See the documentation on `clashmap::mapref::entry` for more details.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    pub fn find_entry(
        &self,
        hash: u64,
        eq: impl FnMut(&T) -> bool,
    ) -> Result<OccupiedEntry<'_, T>, AbsentEntry<'_, T>> {
        let idx = self._determine_shard(hash as usize);

        let shard = self.shards[idx].write();

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Entry`.
        let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(shard) };

        match shard.find_entry(hash, eq) {
            Ok(occupied_entry) => Ok(OccupiedEntry::new(guard, occupied_entry)),
            Err(absent_entry) => Err(AbsentEntry::new(guard, absent_entry)),
        }
    }

    /// Advanced entry API that tries to mimic `std::collections::HashMap`.
    /// See the documentation on `clashmap::mapref::entry` for more details.
    ///
    /// **Locking behaviour:** May deadlock if called when holding any sort of reference into the map.
    pub fn entry(
        &self,
        hash: u64,
        eq: impl FnMut(&T) -> bool,
        hasher: impl Fn(&T) -> u64,
    ) -> Entry<'_, T> {
        let idx = self._determine_shard(hash as usize);

        let shard = self.shards[idx].write();

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Entry`.
        let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(shard) };

        match shard.entry(hash, eq, hasher) {
            hash_table::Entry::Occupied(occupied_entry) => {
                Entry::Occupied(OccupiedEntry::new(guard, occupied_entry))
            }
            hash_table::Entry::Vacant(vacant_entry) => {
                Entry::Vacant(VacantEntry::new(guard, vacant_entry))
            }
        }
    }

    /// Advanced entry API that tries to mimic `std::collections::HashMap`.
    /// See the documentation on `clashmap::mapref::entry` for more details.
    ///
    /// Returns None if the shard is currently locked.
    pub fn try_entry(
        &self,
        hash: u64,
        eq: impl FnMut(&T) -> bool,
        hasher: impl Fn(&T) -> u64,
    ) -> Option<Entry<'_, T>> {
        let idx = self._determine_shard(hash as usize);

        let shard = self.shards[idx].try_write()?;

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Entry`.
        let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(shard) };

        match shard.entry(hash, eq, hasher) {
            hash_table::Entry::Occupied(occupied_entry) => {
                Some(Entry::Occupied(OccupiedEntry::new(guard, occupied_entry)))
            }
            hash_table::Entry::Vacant(vacant_entry) => {
                Some(Entry::Vacant(VacantEntry::new(guard, vacant_entry)))
            }
        }
    }

    /// Advanced entry API that tries to mimic `std::collections::HashMap::try_reserve`.
    /// Tries to reserve capacity for at least `shard * additional`
    /// and may reserve more space to avoid frequent reallocations.
    ///
    /// # Errors
    ///
    /// If the capacity overflows, or the allocator reports a failure, then an error is returned.
    // TODO: return std::collections::TryReserveError once std::collections::TryReserveErrorKind stabilises.
    pub fn try_reserve(
        &mut self,
        additional: usize,
        hasher: impl Fn(&T) -> u64,
    ) -> Result<(), TryReserveError> {
        for shard in self.shards.iter() {
            shard
                .write()
                .try_reserve(additional, |t| hasher(t))
                .map_err(|_| TryReserveError {})?;
        }
        Ok(())
    }
}

impl<T: fmt::Debug> fmt::Debug for ClashTable<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut pmap = f.debug_list();
        self.for_each(|t| {
            pmap.entry(t);
        });
        pmap.finish()
    }
}

impl<T> IntoIterator for ClashTable<T> {
    type Item = T;

    type IntoIter = OwningIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        OwningIter::new(self)
    }
}

impl<'a, T> IntoIterator for &'a ClashTable<T> {
    type Item = RefMulti<'a, T>;

    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(feature = "typesize")]
impl<T> typesize::TypeSize for ClashTable<T>
where
    T: typesize::TypeSize,
{
    fn extra_size(&self) -> usize {
        self.shards
            .iter()
            .map(|shard_lock| {
                let shard = shard_lock.read();

                let hashtable_size = shard.allocation_size();

                let entry_size_iter = shard.iter().map(|entry| entry.extra_size());

                core::mem::size_of::<CachePadded<RwLock<HashTable<T>>>>()
                    + hashtable_size
                    + entry_size_iter.sum::<usize>()
            })
            .sum()
    }

    typesize::if_typesize_details! {
        fn get_collection_item_count(&self) -> Option<usize> {
            Some(self.len())
        }
    }
}
