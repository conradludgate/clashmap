use core::sync::atomic::{AtomicUsize, Ordering};
use parking_lot_core::{ParkToken, SpinWait, UnparkToken};

pub type RwLock<T> = lock_api::RwLock<RawRwLock, T>;
pub(crate) type RwLockReadGuardDetached<'a> = crate::util::RwLockReadGuardDetached<'a, RawRwLock>;
pub(crate) type RwLockWriteGuardDetached<'a> = crate::util::RwLockWriteGuardDetached<'a, RawRwLock>;

const READERS_PARKED: usize = 0b0001;
const WRITERS_PARKED: usize = 0b0010;
const ONE_READER: usize = 0b0100;
const ONE_WRITER: usize = !(READERS_PARKED | WRITERS_PARKED);

pub struct RawRwLock {
    state: AtomicUsize,
}

// Safety:
// This RawRwLock is actually exclusive
unsafe impl lock_api::RawRwLock for RawRwLock {
    #[allow(clippy::declare_interior_mutable_const)]
    const INIT: Self = Self {
        state: AtomicUsize::new(0),
    };

    type GuardMarker = lock_api::GuardSend;

    #[inline]
    fn try_lock_exclusive(&self) -> bool {
        self.state
            .compare_exchange(0, ONE_WRITER, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    #[inline]
    fn lock_exclusive(&self) {
        if self
            .state
            .compare_exchange_weak(0, ONE_WRITER, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            self.lock_exclusive_slow();
        }
    }

    #[inline]
    unsafe fn unlock_exclusive(&self) {
        if self
            .state
            .compare_exchange(ONE_WRITER, 0, Ordering::Release, Ordering::Relaxed)
            .is_err()
        {
            self.unlock_exclusive_slow();
        }
    }

    #[inline]
    fn try_lock_shared(&self) -> bool {
        self.try_lock_shared_fast() || self.try_lock_shared_slow()
    }

    #[inline]
    fn lock_shared(&self) {
        if !self.try_lock_shared_fast() {
            self.lock_shared_slow();
        }
    }

    #[inline]
    unsafe fn unlock_shared(&self) {
        let state = self.state.fetch_sub(ONE_READER, Ordering::Release);

        if state == (ONE_READER | WRITERS_PARKED) {
            self.unlock_shared_slow();
        }
    }
}

// Safety:
// `lock_api::RawRwLockDowngrade` has no explicit safety requirements,
// so I will assume it just requires the `downgrade` be implemented correctly.
unsafe impl lock_api::RawRwLockDowngrade for RawRwLock {
    #[inline]
    unsafe fn downgrade(&self) {
        let state = self
            .state
            .fetch_and(ONE_READER | WRITERS_PARKED, Ordering::Release);
        if state & READERS_PARKED != 0 {
            // SAFETY:
            // 1. We call unpark with an address that we control.
            unsafe {
                parking_lot_core::unpark_all((self as *const _ as usize) + 1, UnparkToken(0));
            }
        }
    }
}

impl RawRwLock {
    #[cold]
    fn lock_exclusive_slow(&self) {
        let mut acquire_with = 0;
        loop {
            let mut spin = SpinWait::new();
            let mut state = self.state.load(Ordering::Relaxed);

            loop {
                while state & ONE_WRITER == 0 {
                    match self.state.compare_exchange_weak(
                        state,
                        state | ONE_WRITER | acquire_with,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => return,
                        Err(e) => state = e,
                    }
                }

                if state & WRITERS_PARKED == 0 {
                    if spin.spin() {
                        state = self.state.load(Ordering::Relaxed);
                        continue;
                    }

                    if let Err(e) = self.state.compare_exchange_weak(
                        state,
                        state | WRITERS_PARKED,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        state = e;
                        continue;
                    }
                }

                // SAFETY:
                // 1. We call park with an address that we control.
                // 2. `validate` will not panic.
                // 3. `before_sleep` and `timed_out` are no-ops.
                let _ = unsafe {
                    parking_lot_core::park(
                        self as *const _ as usize,
                        || {
                            let state = self.state.load(Ordering::Relaxed);
                            (state & ONE_WRITER != 0) && (state & WRITERS_PARKED != 0)
                        },
                        || {},
                        |_, _| {},
                        ParkToken(0),
                        None,
                    )
                };

                acquire_with = WRITERS_PARKED;
                break;
            }
        }
    }

    #[cold]
    fn unlock_exclusive_slow(&self) {
        let state = self.state.load(Ordering::Relaxed);
        assert_eq!(state & ONE_WRITER, ONE_WRITER);

        let mut parked = state & (READERS_PARKED | WRITERS_PARKED);
        assert_ne!(parked, 0);

        if parked != (READERS_PARKED | WRITERS_PARKED) {
            if let Err(new_state) =
                self.state
                    .compare_exchange(state, 0, Ordering::Release, Ordering::Relaxed)
            {
                assert_eq!(new_state, ONE_WRITER | READERS_PARKED | WRITERS_PARKED);
                parked = READERS_PARKED | WRITERS_PARKED;
            }
        }

        if parked == (READERS_PARKED | WRITERS_PARKED) {
            self.state.store(WRITERS_PARKED, Ordering::Release);
            parked = READERS_PARKED;
        }

        if parked == READERS_PARKED {
            // SAFETY:
            // 1. We call unpark with an address that we control.
            return unsafe {
                parking_lot_core::unpark_all((self as *const _ as usize) + 1, UnparkToken(0));
            };
        }

        assert_eq!(parked, WRITERS_PARKED);

        // SAFETY:
        // 1. We call unpark with an address that we control.
        // 2. `callback` will not panic.
        unsafe {
            parking_lot_core::unpark_one(self as *const _ as usize, |_| UnparkToken(0));
        }
    }

    #[inline(always)]
    fn try_lock_shared_fast(&self) -> bool {
        let state = self.state.load(Ordering::Relaxed);

        if let Some(new_state) = state.checked_add(ONE_READER) {
            if new_state & ONE_WRITER != ONE_WRITER {
                return self
                    .state
                    .compare_exchange_weak(state, new_state, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok();
            }
        }

        false
    }

    #[cold]
    fn try_lock_shared_slow(&self) -> bool {
        let mut state = self.state.load(Ordering::Relaxed);

        while let Some(new_state) = state.checked_add(ONE_READER) {
            if new_state & ONE_WRITER == ONE_WRITER {
                break;
            }

            match self.state.compare_exchange_weak(
                state,
                new_state,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(e) => state = e,
            }
        }

        false
    }

    #[cold]
    fn lock_shared_slow(&self) {
        loop {
            let mut spin = SpinWait::new();
            let mut state = self.state.load(Ordering::Relaxed);

            loop {
                let mut backoff = SpinWait::new();
                while let Some(new_state) = state.checked_add(ONE_READER) {
                    assert_ne!(
                        new_state & ONE_WRITER,
                        ONE_WRITER,
                        "reader count overflowed",
                    );

                    if self
                        .state
                        .compare_exchange_weak(
                            state,
                            new_state,
                            Ordering::Acquire,
                            Ordering::Relaxed,
                        )
                        .is_ok()
                    {
                        return;
                    }

                    backoff.spin_no_yield();
                    state = self.state.load(Ordering::Relaxed);
                }

                if state & READERS_PARKED == 0 {
                    if spin.spin() {
                        state = self.state.load(Ordering::Relaxed);
                        continue;
                    }

                    if let Err(e) = self.state.compare_exchange_weak(
                        state,
                        state | READERS_PARKED,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        state = e;
                        continue;
                    }
                }

                // SAFETY:
                // 1. We call park with an address that we control.
                // 2. `validate` will not panic.
                // 3. `before_sleep` and `timed_out` are no-ops.
                let _ = unsafe {
                    parking_lot_core::park(
                        (self as *const _ as usize) + 1,
                        || {
                            let state = self.state.load(Ordering::Relaxed);
                            (state & ONE_WRITER == ONE_WRITER) && (state & READERS_PARKED != 0)
                        },
                        || {},
                        |_, _| {},
                        ParkToken(0),
                        None,
                    )
                };

                break;
            }
        }
    }

    #[cold]
    fn unlock_shared_slow(&self) {
        if self
            .state
            .compare_exchange(WRITERS_PARKED, 0, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            // SAFETY:
            // 1. We call unpark with an address that we control.
            // 2. `callback` will not panic.
            unsafe {
                parking_lot_core::unpark_one(self as *const _ as usize, |_| UnparkToken(0));
            }
        }
    }
}

#[cfg(test)]
#[cfg(not(miri))]
mod tests {
    use std::{thread, time::Duration};

    #[test]
    fn force_wait_unfair() {
        let lock = super::RwLock::new(1);

        thread::scope(|s| {
            s.spawn(|| {
                let r = lock.read();
                thread::sleep(Duration::from_millis(300));
                assert_eq!(*r, 1);
            });

            s.spawn(|| {
                thread::sleep(Duration::from_millis(100));
                let mut r = lock.write();
                assert_eq!(*r, 1);
                *r = 2;
            });

            s.spawn(|| {
                thread::sleep(Duration::from_millis(200));
                let r = lock.read();
                assert_eq!(*r, 1, "this lock is unfair to the writers");
            });
        });

        let r = lock.read();
        assert_eq!(*r, 2);
    }

    #[test]
    fn force_reader_wait() {
        let lock = super::RwLock::new(1);

        thread::scope(|s| {
            s.spawn(|| {
                let r = lock.read();
                thread::sleep(Duration::from_millis(150));
                assert_eq!(*r, 1);
            });

            s.spawn(|| {
                thread::sleep(Duration::from_millis(100));
                let mut r = lock.write();
                thread::sleep(Duration::from_millis(100));
                assert_eq!(*r, 1);
                *r = 2;
            });

            s.spawn(|| {
                thread::sleep(Duration::from_millis(200));
                let r = lock.read();
                assert_eq!(*r, 2);
            });
        });

        let r = lock.read();
        assert_eq!(*r, 2);
    }
}
