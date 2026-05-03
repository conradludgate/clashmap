//! Single-entry guard bundles.
//!
//! [`Ref`] and [`RefMut`] each pair a detached lock guard with a borrow of
//! the data the guard protects. Dropping the bundle releases the lock, which
//! makes the bundle usable as a `Deref`-able handle to a single entry inside
//! a [`crate::ClashCollection`] shard.
//!
//! These types are exclusive to one acquirer; for the iterator-friendly
//! `Arc`-shared variants, see [`crate::multiple`].

use crate::lock::{RwLockReadGuardDetached, RwLockWriteGuardDetached};
use crate::util::try_map;
use core::ops::{Deref, DerefMut};
use std::fmt::{Debug, Formatter};

/// A read guard bundled with a borrow into the data it protects.
///
/// Holds a shared lock for as long as the `Ref` is alive. Dereferences to
/// `T`. Construct with [`Ref::new`] (typically inside a shard accessor like
/// [`crate::ClashCollection::get_read_shard`]).
pub struct Ref<'a, T: ?Sized> {
    _guard: RwLockReadGuardDetached<'a>,
    t: &'a T,
}

impl<'a, T: ?Sized> Ref<'a, T> {
    /// Bundles a detached read guard with a borrow into the data the guard
    /// protects.
    ///
    /// The caller is asserting that `t` points inside the data protected by
    /// `guard`. Passing an unrelated reference compiles, but breaks the
    /// invariant that other operations (notably [`Ref::into_parts`]) rely on,
    /// and downstream callers can then trigger undefined behaviour.
    pub fn new(guard: RwLockReadGuardDetached<'a>, t: &'a T) -> Self {
        Self { _guard: guard, t }
    }

    /// Splits the `Ref` into its guard and the protected reference.
    ///
    /// # Safety
    ///
    /// The returned `&'a T` is only valid while the returned guard is alive.
    /// The caller must keep the guard live for at least as long as any use of
    /// the reference, or re-bundle the two into a new `Ref` (or another type
    /// whose `Drop` order ties them back together).
    pub unsafe fn into_parts(self) -> (RwLockReadGuardDetached<'a>, &'a T) {
        (self._guard, self.t)
    }

    /// Returns a borrow of the protected data.
    pub fn value(&self) -> &T {
        self.t
    }

    /// Transforms the borrow held by this `Ref` while keeping the lock held.
    pub fn map<F, U: ?Sized>(self, f: F) -> Ref<'a, U>
    where
        F: FnOnce(&T) -> &U,
    {
        Ref {
            _guard: self._guard,
            t: f(self.t),
        }
    }

    /// Like [`Ref::map`], but the closure may decline by returning `None`,
    /// in which case the original `Ref` is returned unchanged.
    pub fn try_map<F, U: ?Sized>(self, f: F) -> Result<Ref<'a, U>, Self>
    where
        F: FnOnce(&T) -> Option<&U>,
    {
        if let Some(t) = f(self.t) {
            Ok(Ref {
                _guard: self._guard,
                t,
            })
        } else {
            Err(self)
        }
    }
}

impl<T: Debug> Debug for Ref<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.t.fmt(f)
    }
}

impl<T> Deref for Ref<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<T: std::fmt::Display + ?Sized> std::fmt::Display for Ref<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.value(), f)
    }
}

impl<T: AsRef<TDeref> + ?Sized, TDeref: ?Sized> AsRef<TDeref> for Ref<'_, T> {
    fn as_ref(&self) -> &TDeref {
        self.value().as_ref()
    }
}

/// A write guard bundled with a mutable borrow into the data it protects.
///
/// Holds an exclusive lock for as long as the `RefMut` is alive. Dereferences
/// to `T`. Construct with [`RefMut::new`] (typically inside a shard accessor
/// like [`crate::ClashCollection::get_write_shard`]). Use
/// [`RefMut::downgrade`] to atomically convert into a [`Ref`] without
/// releasing the lock in between.
pub struct RefMut<'a, T: ?Sized> {
    guard: RwLockWriteGuardDetached<'a>,
    t: &'a mut T,
}

impl<'a, T: ?Sized> RefMut<'a, T> {
    /// Bundles a detached write guard with a mutable borrow into the data the
    /// guard protects.
    ///
    /// The caller is asserting that `t` points inside the data protected by
    /// `guard`. Passing an unrelated reference compiles, but breaks the
    /// invariant that other operations (notably [`RefMut::into_parts`]) rely
    /// on, and downstream callers can then trigger undefined behaviour.
    pub fn new(guard: RwLockWriteGuardDetached<'a>, t: &'a mut T) -> Self {
        Self { guard, t }
    }

    /// Splits the `RefMut` into its guard and the protected reference.
    ///
    /// # Safety
    ///
    /// The returned `&'a mut T` is only valid while the returned guard is alive.
    /// The caller must keep the guard live for at least as long as any use of
    /// the reference, or re-bundle the two into a new `RefMut` (or another type
    /// whose `Drop` order ties them back together).
    pub unsafe fn into_parts(self) -> (RwLockWriteGuardDetached<'a>, &'a mut T) {
        (self.guard, self.t)
    }

    /// Returns a shared borrow of the protected data.
    pub fn value(&self) -> &T {
        self.t
    }

    /// Returns a mutable borrow of the protected data.
    pub fn value_mut(&mut self) -> &mut T {
        self.t
    }

    /// Atomically downgrades the held write lock to a read lock and returns
    /// the corresponding [`Ref`]. No other writer can take the lock in
    /// between.
    pub fn downgrade(self) -> Ref<'a, T> {
        Ref::new(
            // SAFETY: `Ref` will prevent writes to the data.
            unsafe { RwLockWriteGuardDetached::downgrade(self.guard) },
            self.t,
        )
    }

    /// Transforms the borrow held by this `RefMut` while keeping the lock
    /// held.
    pub fn map<F, U: ?Sized>(self, f: F) -> RefMut<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        RefMut {
            guard: self.guard,
            t: f(self.t),
        }
    }

    /// Like [`RefMut::map`], but the closure may decline by returning `None`,
    /// in which case the original `RefMut` is returned unchanged.
    pub fn try_map<F, U: 'a + ?Sized>(self, f: F) -> Result<RefMut<'a, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        let Self { guard, t } = self;
        match try_map(t, f) {
            Ok(t) => Ok(RefMut { guard, t }),
            Err(t) => Err(Self { guard, t }),
        }
    }
}

impl<T: Debug + ?Sized> Debug for RefMut<'_, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.t.fmt(f)
    }
}

impl<T: ?Sized> Deref for RefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<T: ?Sized> DerefMut for RefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value_mut()
    }
}
