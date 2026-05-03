//! Sharded `RwLock` container.
//!
//! [`ClashCollection`] is an array of cache-padded [`RwLock`]s addressed by
//! hash. It is the foundation of `clashmap`'s concurrency: each shard can be
//! locked independently, so unrelated keys rarely contend.
//!
//! The number of shards is fixed at construction time and is always a power
//! of two; the high bits of the hash select the shard, leaving the low bits
//! for the inner data structure (typically a `hashbrown` table). Use
//! [`default_shard_amount`] to pick a sensible default based on the host's
//! parallelism, or pass an explicit power of two to
//! [`ClashCollection::with_shard_amount`].

use crate::lock::{RwLock, RwLockReadGuardDetached, RwLockWriteGuardDetached};
use crate::one::{Ref, RefMut};
use crossbeam_utils::CachePadded;
use std::sync::OnceLock;

/// Returns the default shard count: the next power of two at or above
/// `4 * available_parallelism()`.
///
/// Cached after the first call.
pub fn default_shard_amount() -> usize {
    static DEFAULT_SHARD_AMOUNT: OnceLock<usize> = OnceLock::new();
    *DEFAULT_SHARD_AMOUNT.get_or_init(|| {
        (std::thread::available_parallelism().map_or(1, usize::from) * 4).next_power_of_two()
    })
}

/// A fixed-size array of cache-padded [`RwLock`]s, addressed by hash.
///
/// `ClashCollection` is the sharded primitive that powers `clashmap`'s
/// `ClashTable` / `ClashMap` / `ClashSet`. It is generic over the per-shard
/// payload `T`, so the same locking strategy can back a sharded hashtable, a
/// sharded vector, or any other inner data structure.
///
/// # Sharding scheme
///
/// The shard count is always a power of two. Given a hash `h`, the shard
/// index is `(h << 7) >> shift`, where `shift = usize::BITS -
/// log2(shard_amount)`. Shifting left by 7 first leaves the high 7 bits of
/// the hash undisturbed, which lets the inner `hashbrown` table re-use them
/// for its SIMD tag without colliding with the shard selection bits.
///
/// # Locking
///
/// Each shard is independent. Acquiring a shard returns a [`Ref`] or
/// [`RefMut`] that bundles the lock guard with a borrow of the protected
/// data; dropping it releases the lock.
pub struct ClashCollection<T> {
    shift: usize,
    shards: Box<[CachePadded<RwLock<T>>]>,
}

impl<T: Clone> Clone for ClashCollection<T> {
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

impl<T: Default> Default for ClashCollection<T> {
    fn default() -> Self {
        Self::new(T::default)
    }
}

impl<T> ClashCollection<T> {
    /// Allows you to peek at the inner shards that store your data.
    pub fn shards(&self) -> &[CachePadded<RwLock<T>>] {
        &self.shards
    }

    /// Provides mutable access to the inner shards that store your data.
    pub fn shards_mut(&mut self) -> &mut [CachePadded<RwLock<T>>] {
        &mut self.shards
    }

    /// Consumes this `ClashCollection` and returns the inner shards.
    pub fn into_shards(self) -> Box<[CachePadded<RwLock<T>>]> {
        self.shards
    }

    /// Returns the bit shift used to map a hash to a shard index.
    pub fn shift(&self) -> usize {
        self.shift
    }

    /// Reconstructs a `ClashCollection` from its raw parts.
    ///
    /// `shift` must equal `usize::BITS - shards.len().trailing_zeros()` and
    /// `shards.len()` must be a non-zero power of two.
    pub fn from_parts(shift: usize, shards: Box<[CachePadded<RwLock<T>>]>) -> Self {
        Self { shift, shards }
    }

    /// Finds which shard a certain hash is stored in.
    pub fn determine_shard(&self, hash: usize) -> usize {
        self._determine_shard(hash)
    }
}

impl<T> ClashCollection<T> {
    /// Creates a new `ClashCollection`.
    pub fn new(init: impl FnMut() -> T) -> Self {
        ClashCollection::with_shard_amount(default_shard_amount(), init)
    }

    /// Creates a new `ClashCollection` with the given shard amount.
    ///
    /// `shard_amount` must be a power of two strictly greater than 1; both
    /// constraints are enforced by an assertion.
    pub fn with_shard_amount(shard_amount: usize, mut init: impl FnMut() -> T) -> Self {
        assert!(shard_amount > 1);
        assert!(shard_amount.is_power_of_two());

        let shift = (usize::BITS - shard_amount.trailing_zeros()) as usize;

        let shards = (0..shard_amount)
            .map(|_| CachePadded::new(RwLock::new(init())))
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

    // fn for_each(&self, mut f: impl FnMut(&T)) {
    //     self.fold((), |(), kv| f(kv))
    // }

    #[cfg(feature = "typesize")]
    fn fold<R>(&self, r: R, mut f: impl FnMut(R, &T) -> R) -> R {
        match self.try_fold::<R, core::convert::Infallible>(r, |r, kv| Ok(f(r, kv))) {
            Ok(r) => r,
            Err(x) => match x {},
        }
    }

    // fn try_for_each<E>(&self, mut f: impl FnMut(&T) -> Result<(), E>) -> Result<(), E> {
    //     self.try_fold((), |(), kv| f(kv))
    // }

    /// Folds over every shard sequentially, taking each shard's read lock in
    /// turn. Short-circuits on the first error returned by `f`.
    pub fn try_fold<R, E>(
        &self,
        mut r: R,
        mut f: impl FnMut(R, &T) -> Result<R, E>,
    ) -> Result<R, E> {
        for shard in self.shards.iter() {
            let shard = shard.read();
            r = f(r, &shard)?;
        }
        Ok(r)
    }

    /// Acquires the read lock for the shard `hash` belongs to and returns a
    /// guard bundled with a borrow of the shard's data. Blocks until the
    /// lock is available.
    pub fn get_read_shard(&self, hash: u64) -> Ref<'_, T> {
        let idx = self._determine_shard(hash as usize);
        let shard = self.shards[idx].read();

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Ref`.
        let (guard, shard) = unsafe { RwLockReadGuardDetached::detach_from(shard) };
        Ref::new(guard, shard)
    }

    /// Acquires the write lock for the shard `hash` belongs to and returns a
    /// guard bundled with a mutable borrow of the shard's data. Blocks until
    /// the lock is available.
    pub fn get_write_shard(&self, hash: u64) -> RefMut<'_, T> {
        let idx = self._determine_shard(hash as usize);
        let shard = self.shards[idx].write();

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Ref`.
        let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(shard) };
        RefMut::new(guard, shard)
    }

    /// Like [`ClashCollection::get_read_shard`] but returns `None` instead of
    /// blocking if the lock is currently held exclusively.
    pub fn try_read_shard(&self, hash: u64) -> Option<Ref<'_, T>> {
        let idx = self._determine_shard(hash as usize);
        let shard = self.shards[idx].try_read()?;

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Ref`.
        let (guard, shard) = unsafe { RwLockReadGuardDetached::detach_from(shard) };
        Some(Ref::new(guard, shard))
    }

    /// Like [`ClashCollection::get_write_shard`] but returns `None` instead
    /// of blocking if the lock is currently held by anyone else.
    pub fn try_write_shard(&self, hash: u64) -> Option<RefMut<'_, T>> {
        let idx = self._determine_shard(hash as usize);
        let shard = self.shards[idx].try_write()?;

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Ref`.
        let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(shard) };
        Some(RefMut::new(guard, shard))
    }

    /// Returns a mutable borrow of the shard `hash` belongs to without
    /// taking any lock — sound only because `&mut self` proves there are no
    /// concurrent accessors.
    pub fn get_mut(&mut self, hash: u64) -> &mut T {
        let idx = self._determine_shard(hash as usize);
        self.shards[idx].get_mut()
    }
}

#[cfg(feature = "typesize")]
impl<T: typesize::TypeSize> typesize::TypeSize for ClashCollection<T> {
    fn extra_size(&self) -> usize {
        let acc = core::mem::size_of_val(&self.shards);
        self.fold(acc, |acc, shard| acc + shard.extra_size())
    }

    typesize::if_typesize_details! {
        fn get_collection_item_count(&self) -> Option<usize> {
            Some(self.shards.len())
        }
    }
}
