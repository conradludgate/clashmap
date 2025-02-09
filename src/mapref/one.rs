use crate::lock::{RwLockReadGuardDetached, RwLockWriteGuardDetached};
use crate::util::try_map;
use core::ops::{Deref, DerefMut};
use std::fmt::{Debug, Formatter};

pub struct Ref<'a, K, V> {
    _guard: RwLockReadGuardDetached<'a>,
    k: &'a K,
    v: &'a V,
}

impl<'a, K, V> Ref<'a, K, V> {
    pub(crate) fn new(guard: RwLockReadGuardDetached<'a>, k: &'a K, v: &'a V) -> Self {
        Self {
            _guard: guard,
            k,
            v,
        }
    }

    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &V {
        self.pair().1
    }

    pub fn pair(&self) -> (&K, &V) {
        (self.k, self.v)
    }

    pub fn map<F, T>(self, f: F) -> MappedRef<'a, K, T>
    where
        F: FnOnce(&V) -> &T,
    {
        MappedRef {
            _guard: self._guard,
            k: self.k,
            v: f(self.v),
        }
    }

    pub fn try_map<F, T>(self, f: F) -> Result<MappedRef<'a, K, T>, Self>
    where
        F: FnOnce(&V) -> Option<&T>,
    {
        if let Some(v) = f(self.v) {
            Ok(MappedRef {
                _guard: self._guard,
                k: self.k,
                v,
            })
        } else {
            Err(self)
        }
    }
}

impl<K: Debug, V: Debug> Debug for Ref<'_, K, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ref")
            .field("k", &self.k)
            .field("v", &self.v)
            .finish()
    }
}

impl<K, V> Deref for Ref<'_, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

pub struct RefMut<'a, K, V> {
    guard: RwLockWriteGuardDetached<'a>,
    k: &'a K,
    v: &'a mut V,
}

impl<'a, K, V> RefMut<'a, K, V> {
    pub(crate) fn new(guard: RwLockWriteGuardDetached<'a>, k: &'a K, v: &'a mut V) -> Self {
        Self { guard, k, v }
    }

    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &V {
        self.pair().1
    }

    pub fn value_mut(&mut self) -> &mut V {
        self.pair_mut().1
    }

    pub fn pair(&self) -> (&K, &V) {
        (self.k, self.v)
    }

    pub fn pair_mut(&mut self) -> (&K, &mut V) {
        (self.k, self.v)
    }

    pub fn downgrade(self) -> Ref<'a, K, V> {
        Ref::new(
            // SAFETY: `Ref` will prevent writes to the data.
            unsafe { RwLockWriteGuardDetached::downgrade(self.guard) },
            self.k,
            self.v,
        )
    }

    pub fn map<F, T>(self, f: F) -> MappedRefMut<'a, K, T>
    where
        F: FnOnce(&mut V) -> &mut T,
    {
        MappedRefMut {
            _guard: self.guard,
            k: self.k,
            v: f(&mut *self.v),
        }
    }

    pub fn try_map<F, T: 'a>(self, f: F) -> Result<MappedRefMut<'a, K, T>, Self>
    where
        F: FnOnce(&mut V) -> Option<&mut T>,
    {
        let Self { guard, k, v } = self;
        match try_map(v, f) {
            Ok(v) => Ok(MappedRefMut {
                _guard: guard,
                k,
                v,
            }),
            Err(v) => Err(Self { guard, k, v }),
        }
    }
}

impl<K: Debug, V: Debug> Debug for RefMut<'_, K, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefMut")
            .field("k", &self.k)
            .field("v", &self.v)
            .finish()
    }
}

impl<K, V> Deref for RefMut<'_, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

impl<K, V> DerefMut for RefMut<'_, K, V> {
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}

pub struct MappedRef<'a, K, T> {
    _guard: RwLockReadGuardDetached<'a>,
    k: &'a K,
    v: &'a T,
}

impl<'a, K, T> MappedRef<'a, K, T> {
    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &T {
        self.pair().1
    }

    pub fn pair(&self) -> (&K, &T) {
        (self.k, self.v)
    }

    pub fn map<F, T2>(self, f: F) -> MappedRef<'a, K, T2>
    where
        F: FnOnce(&T) -> &T2,
    {
        MappedRef {
            _guard: self._guard,
            k: self.k,
            v: f(self.v),
        }
    }

    pub fn try_map<F, T2>(self, f: F) -> Result<MappedRef<'a, K, T2>, Self>
    where
        F: FnOnce(&T) -> Option<&T2>,
    {
        let v = match f(self.v) {
            Some(v) => v,
            None => return Err(self),
        };
        let guard = self._guard;
        Ok(MappedRef {
            _guard: guard,
            k: self.k,
            v,
        })
    }
}

impl<K: Debug, T: Debug> Debug for MappedRef<'_, K, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MappedRef")
            .field("k", &self.k)
            .field("v", &self.v)
            .finish()
    }
}

impl<K, T> Deref for MappedRef<'_, K, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<K, T: std::fmt::Display> std::fmt::Display for MappedRef<'_, K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.value(), f)
    }
}

impl<K, T: AsRef<TDeref>, TDeref: ?Sized> AsRef<TDeref> for MappedRef<'_, K, T> {
    fn as_ref(&self) -> &TDeref {
        self.value().as_ref()
    }
}

pub struct MappedRefMut<'a, K, T> {
    _guard: RwLockWriteGuardDetached<'a>,
    k: &'a K,
    v: &'a mut T,
}

impl<'a, K, T> MappedRefMut<'a, K, T> {
    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &T {
        self.pair().1
    }

    pub fn value_mut(&mut self) -> &mut T {
        self.pair_mut().1
    }

    pub fn pair(&self) -> (&K, &T) {
        (self.k, self.v)
    }

    pub fn pair_mut(&mut self) -> (&K, &mut T) {
        (self.k, self.v)
    }

    pub fn map<F, T2>(self, f: F) -> MappedRefMut<'a, K, T2>
    where
        F: FnOnce(&mut T) -> &mut T2,
    {
        MappedRefMut {
            _guard: self._guard,
            k: self.k,
            v: f(self.v),
        }
    }

    pub fn try_map<F, T2>(self, f: F) -> Result<MappedRefMut<'a, K, T2>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut T2>,
    {
        let Self { _guard, k, v } = self;
        match try_map(v, f) {
            Ok(v) => Ok(MappedRefMut { _guard, k, v }),
            Err(v) => Err(Self { _guard, k, v }),
        }
    }
}

impl<K: Debug, T: Debug> Debug for MappedRefMut<'_, K, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MappedRefMut")
            .field("k", &self.k)
            .field("v", &self.v)
            .finish()
    }
}

impl<K, T> Deref for MappedRefMut<'_, K, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<K, T> DerefMut for MappedRefMut<'_, K, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value_mut()
    }
}
