//! Clever hacks

use std::{marker::PhantomData, mem::ManuallyDrop};

use lock_api::{RawRwLock, RawRwLockDowngrade, RwLockReadGuard, RwLockWriteGuard};

/// Returns `f(&mut *t)` if it yields `Some`, otherwise returns `t` unchanged.
///
/// Equivalent in spirit to `Result::map_err`, but for borrowed data: it lets
/// the caller try to narrow a `&mut T` into a `&mut U` and recover the
/// original borrow if narrowing fails. Implemented with `polonius-the-crab`
/// because the standard borrow checker rejects the natural pattern.
pub(crate) fn try_map<F, T: ?Sized, U: ?Sized>(mut t: &mut T, f: F) -> Result<&mut U, &mut T>
where
    F: FnOnce(&mut T) -> Option<&mut U>,
{
    use polonius_the_crab::{polonius, polonius_return};
    polonius!(|t| -> Result<&'polonius mut U, &mut T> {
        if let Some(u) = f(t) {
            polonius_return!(Ok(u));
        }
    });
    Err(t)
}

/// A [`RwLockReadGuard`] split apart from the protected data.
///
/// Holds the shared (read) lock until dropped, but does not carry a reference
/// to the locked data. This lets callers bundle the guard alongside an
/// arbitrary view derived from the locked data (e.g. a slice, a struct field)
/// while still releasing the lock when the bundle is dropped.
///
/// Pair with the data via [`RwLockReadGuardDetached::detach_from`]; the
/// returned reference must not outlive the guard.
pub(crate) struct RwLockReadGuardDetached<'a, R: RawRwLock> {
    lock: &'a R,
    _marker: PhantomData<R::GuardMarker>,
}

impl<R: RawRwLock> Drop for RwLockReadGuardDetached<'_, R> {
    fn drop(&mut self) {
        // Safety: An RwLockReadGuardDetached always holds a shared lock.
        unsafe {
            self.lock.unlock_shared();
        }
    }
}

/// A [`RwLockWriteGuard`] split apart from the protected data.
///
/// Holds the exclusive (write) lock until dropped, but does not carry a
/// reference to the locked data. This lets callers bundle the guard alongside
/// an arbitrary view derived from the locked data (e.g. a slice, a struct
/// field) while still releasing the lock when the bundle is dropped.
///
/// Pair with the data via [`RwLockWriteGuardDetached::detach_from`]; the
/// returned reference must not outlive the guard.
pub(crate) struct RwLockWriteGuardDetached<'a, R: RawRwLock> {
    lock: &'a R,
    _marker: PhantomData<R::GuardMarker>,
}

impl<R: RawRwLock> Drop for RwLockWriteGuardDetached<'_, R> {
    fn drop(&mut self) {
        // Safety: An RwLockWriteGuardDetached always holds an exclusive lock.
        unsafe {
            self.lock.unlock_exclusive();
        }
    }
}

impl<'a, R: RawRwLock> RwLockReadGuardDetached<'a, R> {
    /// Splits a [`RwLockReadGuard`] into a detached guard and a raw reference
    /// to the protected data.
    ///
    /// The shared lock continues to be held by the returned guard; dropping
    /// the guard releases the lock.
    ///
    /// # Safety
    ///
    /// The returned `&'a T` must not be used after the returned guard is
    /// dropped. In particular, the caller must not `mem::forget` the guard or
    /// move it to a scope shorter than any borrow derived from the returned
    /// reference. Misuse causes a use-after-unlock and is undefined behaviour.
    pub(crate) unsafe fn detach_from<T>(guard: RwLockReadGuard<'a, R, T>) -> (Self, &'a T) {
        let rwlock = RwLockReadGuard::rwlock(&ManuallyDrop::new(guard));

        // Safety: There will be no concurrent writes as we are "forgetting" the existing guard,
        // with the safety assumption that the caller will not drop the new detached guard early.
        let data = unsafe { &*rwlock.data_ptr() };
        let guard = RwLockReadGuardDetached {
            // Safety: We are imitating the original RwLockReadGuard. It's the callers
            // responsibility to not drop the guard early.
            lock: unsafe { rwlock.raw() },
            _marker: PhantomData,
        };
        (guard, data)
    }
}

impl<'a, R: RawRwLock> RwLockWriteGuardDetached<'a, R> {
    /// Splits a [`RwLockWriteGuard`] into a detached guard and a raw mutable
    /// reference to the protected data.
    ///
    /// The exclusive lock continues to be held by the returned guard; dropping
    /// the guard releases the lock.
    ///
    /// # Safety
    ///
    /// The returned `&'a mut T` must not be used after the returned guard is
    /// dropped. In particular, the caller must not `mem::forget` the guard or
    /// move it to a scope shorter than any borrow derived from the returned
    /// reference. Misuse causes a use-after-unlock and is undefined behaviour.
    pub(crate) unsafe fn detach_from<T>(guard: RwLockWriteGuard<'a, R, T>) -> (Self, &'a mut T) {
        let rwlock = RwLockWriteGuard::rwlock(&ManuallyDrop::new(guard));

        // Safety: There will be no concurrent reads/writes as we are "forgetting" the existing guard,
        // with the safety assumption that the caller will not drop the new detached guard early.
        let data = unsafe { &mut *rwlock.data_ptr() };
        let guard = RwLockWriteGuardDetached {
            // Safety: We are imitating the original RwLockWriteGuard. It's the callers
            // responsibility to not drop the guard early.
            lock: unsafe { rwlock.raw() },
            _marker: PhantomData,
        };
        (guard, data)
    }
}

impl<'a, R: RawRwLockDowngrade> RwLockWriteGuardDetached<'a, R> {
    /// Atomically downgrades the exclusive lock held by this guard into a
    /// shared lock.
    ///
    /// # Safety
    ///
    /// Any `&mut T` that was obtained alongside this write guard (typically
    /// via [`RwLockWriteGuardDetached::detach_from`]) must not be used after
    /// downgrading: once the lock is shared, other readers may observe the
    /// data, so further mutation through the existing `&mut T` would alias
    /// those readers and is undefined behaviour. Convert any retained
    /// reference to an `&T` (or drop it) before calling this method.
    pub(crate) unsafe fn downgrade(self) -> RwLockReadGuardDetached<'a, R> {
        // Do not drop the write guard - otherwise we will trigger a downgrade + unlock_exclusive,
        // which is incorrect
        let this = ManuallyDrop::new(self);

        // Safety: An RwLockWriteGuardDetached always holds an exclusive lock.
        unsafe { this.lock.downgrade() }
        RwLockReadGuardDetached {
            lock: this.lock,
            _marker: this._marker,
        }
    }
}
