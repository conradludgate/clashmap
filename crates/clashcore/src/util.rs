//! Clever hacks

use std::{marker::PhantomData, mem::ManuallyDrop};

use lock_api::{RawRwLock, RawRwLockDowngrade, RwLockReadGuard, RwLockWriteGuard};

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
/// Pair with the data via [`RwLockReadGuardDetached::detach_from`]; accesses
/// through the returned reference must remain protected by the shared lock.
pub struct RwLockReadGuardDetached<'a, R: RawRwLock> {
    // Safety invariant: this guard owns one held shared acquisition of `lock`.
    // No other owner may release that acquisition while this guard exists.
    lock: &'a R,
    _marker: PhantomData<R::GuardMarker>,
}

impl<R: RawRwLock> Drop for RwLockReadGuardDetached<'_, R> {
    fn drop(&mut self) {
        // SAFETY: The lock field invariant provides one held shared acquisition
        // owned by this guard. Drop consumes its sole unlock obligation.
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
/// Pair with the data via [`RwLockWriteGuardDetached::detach_from`]; accesses
/// through the returned reference must remain protected by the exclusive lock.
pub struct RwLockWriteGuardDetached<'a, R: RawRwLock> {
    // Safety invariant: this guard owns the held exclusive acquisition of `lock`.
    // No other owner may release or downgrade it while this guard exists.
    lock: &'a R,
    _marker: PhantomData<R::GuardMarker>,
}

impl<R: RawRwLock> Drop for RwLockWriteGuardDetached<'_, R> {
    fn drop(&mut self) {
        // SAFETY: The lock field invariant provides the held exclusive
        // acquisition owned by this guard. Drop consumes its sole unlock obligation.
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
    /// The caller must ensure that every access through the returned reference
    /// or a reference derived from it is protected by the shared acquisition
    /// transferred to the returned guard. Such borrows must end before that
    /// acquisition is released, including accesses made while unwinding.
    ///
    /// Forgetting the guard leaks the acquisition without unlocking it; this
    /// does not itself violate the contract. It does not extend the lifetime
    /// of the underlying lock or data beyond `'a`.
    pub unsafe fn detach_from<T>(guard: RwLockReadGuard<'a, R, T>) -> (Self, &'a T) {
        let rwlock = RwLockReadGuard::rwlock(&ManuallyDrop::new(guard));

        // SAFETY: The original read guard holds the shared acquisition and
        // ManuallyDrop prevents its destructor from releasing it. data_ptr()
        // addresses the same initialized T protected by that acquisition.
        // The caller keeps accesses through this reference protected until they
        // end; the returned guard takes over the original unlock obligation.
        let data = unsafe { &*rwlock.data_ptr() };
        let guard = RwLockReadGuardDetached {
            // SAFETY: The original guard is disabled, so this field takes sole
            // ownership of its held shared acquisition. All accesses through the
            // paired data reference are covered by the caller's contract above.
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
    /// The caller must ensure that every access through the returned mutable
    /// reference or a reference derived from it is protected by the exclusive
    /// acquisition transferred to the returned guard. Such borrows must end
    /// before that acquisition is released or downgraded, including accesses
    /// made while unwinding. Before downgrading, a retained borrow may instead
    /// be converted to a shared borrow under [`Self::downgrade`]'s contract.
    ///
    /// Forgetting the guard leaks the acquisition without unlocking it; this
    /// does not itself violate the contract. It does not extend the lifetime
    /// of the underlying lock or data beyond `'a`.
    pub unsafe fn detach_from<T>(guard: RwLockWriteGuard<'a, R, T>) -> (Self, &'a mut T) {
        let rwlock = RwLockWriteGuard::rwlock(&ManuallyDrop::new(guard));

        // SAFETY: The original write guard holds the exclusive acquisition and
        // ManuallyDrop prevents its destructor from releasing it. data_ptr()
        // addresses the same initialized T exclusively protected by that guard.
        // The caller retains this protection for every access requiring the
        // mutable borrow; the returned guard takes over the unlock obligation.
        let data = unsafe { &mut *rwlock.data_ptr() };
        let guard = RwLockWriteGuardDetached {
            // SAFETY: The original guard is disabled, so this field takes sole
            // ownership of its held exclusive acquisition. All accesses through
            // the paired data reference are covered by the caller's contract.
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
    /// The caller must end all exclusive borrows protected by this acquisition
    /// before calling this method. Any retained view must already have been
    /// converted to a shared borrow: other readers may acquire the lock as soon
    /// as the raw downgrade occurs, before this method returns.
    ///
    /// Accesses through retained shared views must remain protected by the
    /// returned shared guard, as in [`RwLockReadGuardDetached::detach_from`].
    pub unsafe fn downgrade(self) -> RwLockReadGuardDetached<'a, R> {
        // Do not drop the write guard - otherwise we will trigger a downgrade + unlock_exclusive,
        // which is incorrect
        let this = ManuallyDrop::new(self);

        // SAFETY: The lock field invariant supplies the held exclusive
        // acquisition required by RawRwLockDowngrade::downgrade. ManuallyDrop
        // disables its exclusive unlock, and the caller has ended exclusive
        // borrows before readers can acquire the lock. The atomic downgrade
        // leaves one shared acquisition, transferred to the returned guard.
        unsafe { this.lock.downgrade() }
        RwLockReadGuardDetached {
            lock: this.lock,
            _marker: this._marker,
        }
    }
}
