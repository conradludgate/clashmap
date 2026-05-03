//! `Arc`-shared guard bundles for iterator yields.
//!
//! [`RefMulti`] and [`RefMutMulti`] are the multi-acquirer counterparts to
//! [`crate::one::Ref`] and [`crate::one::RefMut`]: an iterator that walks an
//! entire shard takes one lock and then yields one bundle per entry, all
//! sharing the same `Arc`-wrapped guard. The lock is released only when the
//! last yielded bundle is dropped.
//!
//! `RefMutMulti` exists despite holding an exclusive lock because the
//! borrow checker cannot statically prove that two `&mut T` handed out from
//! the same shard refer to disjoint entries; the `Arc<WriteGuard>` keeps the
//! shard's exclusive lock alive across the yields.

use crate::lock::{RwLockReadGuardDetached, RwLockWriteGuardDetached};
use core::ops::{Deref, DerefMut};
use std::sync::Arc;

/// A read guard shared between multiple yielded references from the same
/// shard.
///
/// Cheap to clone — the underlying lock guard is `Arc`-shared and the lock
/// is only released when the last clone is dropped.
pub struct RefMulti<'a, T> {
    _guard: Arc<RwLockReadGuardDetached<'a>>,
    t: &'a T,
}

impl<T> Clone for RefMulti<'_, T> {
    fn clone(&self) -> Self {
        Self {
            _guard: self._guard.clone(),
            t: self.t,
        }
    }
}

impl<'a, T> RefMulti<'a, T> {
    /// Bundles an `Arc`-shared detached read guard with a borrow into the
    /// data the guard protects.
    ///
    /// The caller is asserting that `v` points inside the data protected by
    /// `guard`. Passing an unrelated reference compiles, but breaks the
    /// type's invariant.
    pub fn new(guard: Arc<RwLockReadGuardDetached<'a>>, v: &'a T) -> Self {
        Self {
            _guard: guard,
            t: v,
        }
    }

    /// Returns a borrow of the protected data.
    pub fn value(&self) -> &T {
        self.t
    }
}

impl<T> Deref for RefMulti<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

/// A write guard shared between multiple yielded mutable references from the
/// same shard.
///
/// The `Arc<WriteGuard>` keeps the shard's exclusive lock alive across an
/// iterator that hands out one `&mut T` per entry. Disjointness of those
/// `&mut T`s is the iterator's responsibility, not the type's.
pub struct RefMutMulti<'a, T> {
    _guard: Arc<RwLockWriteGuardDetached<'a>>,
    t: &'a mut T,
}

impl<'a, T> RefMutMulti<'a, T> {
    /// Bundles an `Arc`-shared detached write guard with a mutable borrow
    /// into the data the guard protects.
    ///
    /// The caller is asserting that `t` points inside the data protected by
    /// `guard`, and that no other `RefMutMulti` sharing the same guard
    /// aliases this borrow. Disjointness is the iterator's responsibility,
    /// not the type's.
    pub fn new(guard: Arc<RwLockWriteGuardDetached<'a>>, t: &'a mut T) -> Self {
        Self { _guard: guard, t }
    }

    /// Returns a shared borrow of the protected data.
    pub fn value(&self) -> &T {
        self.t
    }

    /// Returns a mutable borrow of the protected data.
    pub fn value_mut(&mut self) -> &mut T {
        self.t
    }
}

impl<T> Deref for RefMutMulti<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<T> DerefMut for RefMutMulti<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value_mut()
    }
}
