use crate::lock::{RwLockReadGuardDetached, RwLockWriteGuardDetached};
use crate::tableref;
use crate::util::{try_map, try_map2};
use core::ops::{Deref, DerefMut};
use std::fmt::{Debug, Formatter};

pub struct Ref<'a, K, V> {
    _guard: RwLockReadGuardDetached<'a>,
    k: &'a K,
    v: &'a V,
}

impl<'a, K, V> Ref<'a, K, V> {
    pub fn key(&self) -> &K {
        self.pair().0
    }

    pub fn value(&self) -> &V {
        self.pair().1
    }

    pub fn pair(&self) -> (&K, &V) {
        (self.k, self.v)
    }

    pub fn map<F, T: ?Sized>(self, f: F) -> MappedRef<'a, K, T>
    where
        F: FnOnce(&V) -> &T,
    {
        MappedRef {
            _guard: self._guard,
            k: self.k,
            v: f(self.v),
        }
    }

    pub fn try_map<F, T: ?Sized>(self, f: F) -> Result<MappedRef<'a, K, T>, Self>
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

impl<'a, K, V> From<tableref::one::Ref<'a, (K, V)>> for Ref<'a, K, V> {
    fn from(value: tableref::one::Ref<'a, (K, V)>) -> Self {
        Self {
            _guard: value._guard,
            k: &value.t.0,
            v: &value.t.1,
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
    inner: tableref::one::RefMut<'a, (K, V)>,
}

impl<'a, K, V> From<tableref::one::RefMut<'a, (K, V)>> for RefMut<'a, K, V> {
    fn from(inner: tableref::one::RefMut<'a, (K, V)>) -> Self {
        Self { inner }
    }
}

impl<'a, K, V> RefMut<'a, K, V> {
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
        let (k, v) = self.inner.value();
        (k, v)
    }

    pub fn pair_mut(&mut self) -> (&K, &mut V) {
        let (k, v) = self.inner.value_mut();
        (k, v)
    }

    pub fn downgrade(self) -> Ref<'a, K, V> {
        self.inner.downgrade().into()
    }

    pub fn map<F, T: ?Sized>(self, f: F) -> MappedRefMut<'a, K, T>
    where
        F: FnOnce(&mut V) -> &mut T,
    {
        let (k, v) = self.inner.t;
        MappedRefMut {
            _guard: self.inner.guard,
            k,
            v: f(v),
        }
    }

    pub fn try_map<F, T: 'a + ?Sized>(self, f: F) -> Result<MappedRefMut<'a, K, T>, Self>
    where
        F: FnOnce(&mut V) -> Option<&mut T>,
    {
        let tableref::one::RefMut { guard, t } = self.inner;
        match try_map2(t, f) {
            Ok((k, v)) => Ok(MappedRefMut {
                _guard: guard,
                k,
                v,
            }),
            Err(t) => Err(Self {
                inner: tableref::one::RefMut { guard, t },
            }),
        }
    }
}

impl<K: Debug, V: Debug> Debug for RefMut<'_, K, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefMut")
            .field("k", &self.key())
            .field("v", &self.value())
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

pub struct MappedRef<'a, K, T: ?Sized> {
    _guard: RwLockReadGuardDetached<'a>,
    k: &'a K,
    v: &'a T,
}

impl<'a, K, T: ?Sized> MappedRef<'a, K, T> {
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

    pub fn try_map<F, T2: ?Sized>(self, f: F) -> Result<MappedRef<'a, K, T2>, Self>
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

impl<K: Debug, T: Debug + ?Sized> Debug for MappedRef<'_, K, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MappedRef")
            .field("k", &self.k)
            .field("v", &self.v)
            .finish()
    }
}

impl<K, T: ?Sized> Deref for MappedRef<'_, K, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<K, T: std::fmt::Display + ?Sized> std::fmt::Display for MappedRef<'_, K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.value(), f)
    }
}

impl<K, T: ?Sized + AsRef<TDeref>, TDeref: ?Sized> AsRef<TDeref> for MappedRef<'_, K, T> {
    fn as_ref(&self) -> &TDeref {
        self.value().as_ref()
    }
}

pub struct MappedRefMut<'a, K, T: ?Sized> {
    _guard: RwLockWriteGuardDetached<'a>,
    k: &'a K,
    v: &'a mut T,
}

impl<'a, K, T: ?Sized> MappedRefMut<'a, K, T> {
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

    pub fn map<F, T2: ?Sized>(self, f: F) -> MappedRefMut<'a, K, T2>
    where
        F: FnOnce(&mut T) -> &mut T2,
    {
        MappedRefMut {
            _guard: self._guard,
            k: self.k,
            v: f(self.v),
        }
    }

    pub fn try_map<F, T2: ?Sized>(self, f: F) -> Result<MappedRefMut<'a, K, T2>, Self>
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

impl<K: Debug, T: Debug + ?Sized> Debug for MappedRefMut<'_, K, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MappedRefMut")
            .field("k", &self.k)
            .field("v", &self.v)
            .finish()
    }
}

impl<K, T: ?Sized> Deref for MappedRefMut<'_, K, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.value()
    }
}

impl<K, T: ?Sized> DerefMut for MappedRefMut<'_, K, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value_mut()
    }
}

#[cfg(test)]
mod tests {
    use crate::ClashMap;

    #[test]
    fn downgrade() {
        let data = ClashMap::new();
        data.insert("test", "test");
        if let Some(mut w_ref) = data.get_mut("test") {
            *w_ref.value_mut() = "test2";
            let r_ref = w_ref.downgrade();
            assert_eq!(*r_ref.value(), "test2");
        };
    }

    #[test]
    fn mapped_mut() {
        let data = ClashMap::new();
        data.insert("test", *b"test");
        if let Some(b_ref) = data.get_mut("test") {
            let mut s_ref = b_ref.try_map(|b| std::str::from_utf8_mut(b).ok()).unwrap();
            s_ref.value_mut().make_ascii_uppercase();
        }

        assert_eq!(data.get("test").unwrap().value(), b"TEST");
    }

    #[test]
    fn mapped_mut_again() {
        let data = ClashMap::new();
        data.insert("test", *b"hello world");
        if let Some(b_ref) = data.get_mut("test") {
            let s_ref = b_ref.try_map(|b| std::str::from_utf8_mut(b).ok()).unwrap();
            let mut hello_ref = s_ref.try_map(|s| s.get_mut(..5)).unwrap();
            hello_ref.value_mut().make_ascii_uppercase();
        }

        assert_eq!(data.get("test").unwrap().value(), b"HELLO world");
    }

    #[test]
    fn mapped_ref() {
        let data = ClashMap::new();
        data.insert("test", *b"test");
        if let Some(b_ref) = data.get("test") {
            let s_ref = b_ref.try_map(|b| std::str::from_utf8(b).ok()).unwrap();

            assert_eq!(s_ref.value(), "test");
        };
    }

    #[test]
    fn mapped_ref_again() {
        let data = ClashMap::new();
        data.insert("test", *b"hello world");
        if let Some(b_ref) = data.get("test") {
            let s_ref = b_ref.try_map(|b| std::str::from_utf8(b).ok()).unwrap();
            let hello_ref = s_ref.try_map(|s| s.get(..5)).unwrap();

            assert_eq!(hello_ref.value(), "hello");
        };
    }
}
