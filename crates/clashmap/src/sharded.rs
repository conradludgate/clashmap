use crate::default_shard_amount;
use crate::lock::{RwLock, RwLockReadGuardDetached, RwLockWriteGuardDetached};
use crate::tableref::one::{Ref, RefMut};
use crossbeam_utils::CachePadded;

/// An implementation detail of [`ClashTable`](crate::ClashTable), exposed for convenience.
///
/// This implements the core sharded data structure that allows for efficient concurrency in ClashMap.
///
/// Requires the `raw-api` feature to be enabled.
pub struct ClashCollection<T> {
    pub(crate) shift: usize,
    pub(crate) shards: Box<[CachePadded<RwLock<T>>]>,
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

#[allow(dead_code)]
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

    /// Creates a new `ClashCollection` with a specified shard amount
    ///
    /// shard_amount should greater than 0 and be a power of two.
    /// If a shard_amount which is not a power of two is provided, the function will panic.
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

    pub(crate) fn try_fold<R, E>(
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

    pub fn get_read_shard(&self, hash: u64) -> Ref<'_, T> {
        let idx = self._determine_shard(hash as usize);
        let shard = self.shards[idx].read();

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Ref`.
        let (guard, shard) = unsafe { RwLockReadGuardDetached::detach_from(shard) };
        Ref::new(guard, shard)
    }

    pub fn get_write_shard(&self, hash: u64) -> RefMut<'_, T> {
        let idx = self._determine_shard(hash as usize);
        let shard = self.shards[idx].write();

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Ref`.
        let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(shard) };
        RefMut::new(guard, shard)
    }

    pub fn try_read_shard(&self, hash: u64) -> Option<Ref<'_, T>> {
        let idx = self._determine_shard(hash as usize);
        let shard = self.shards[idx].try_read()?;

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Ref`.
        let (guard, shard) = unsafe { RwLockReadGuardDetached::detach_from(shard) };
        Some(Ref::new(guard, shard))
    }

    pub fn try_write_shard(&self, hash: u64) -> Option<RefMut<'_, T>> {
        let idx = self._determine_shard(hash as usize);
        let shard = self.shards[idx].try_write()?;

        // SAFETY: The data will not outlive the guard, since we pass the guard to `Ref`.
        let (guard, shard) = unsafe { RwLockWriteGuardDetached::detach_from(shard) };
        Some(RefMut::new(guard, shard))
    }

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
