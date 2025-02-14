use crate::tableref;
use core::ops::{Deref, DerefMut};

pub struct RefMulti<'a, K, V> {
    inner: tableref::multiple::RefMulti<'a, (K, V)>,
}

impl<K, V> Clone for RefMulti<'_, K, V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<'a, K, V> RefMulti<'a, K, V> {
    pub(crate) fn new(inner: tableref::multiple::RefMulti<'a, (K, V)>) -> Self {
        Self { inner }
    }

    pub fn key(&self) -> &K {
        &self.inner.value().0
    }

    pub fn value(&self) -> &V {
        &self.inner.value().1
    }

    pub fn pair(&self) -> (&K, &V) {
        let (k, v) = self.inner.value();
        (k, v)
    }
}

impl<K, V> Deref for RefMulti<'_, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

pub struct RefMutMulti<'a, K, V> {
    inner: tableref::multiple::RefMutMulti<'a, (K, V)>,
}

impl<'a, K, V> RefMutMulti<'a, K, V> {
    pub(crate) fn new(inner: tableref::multiple::RefMutMulti<'a, (K, V)>) -> Self {
        Self { inner }
    }

    pub fn key(&self) -> &K {
        &self.inner.value().0
    }

    pub fn value(&self) -> &V {
        &self.inner.value().1
    }

    pub fn value_mut(&mut self) -> &mut V {
        &mut self.inner.value_mut().1
    }

    pub fn pair(&self) -> (&K, &V) {
        let (k, v) = self.inner.value();
        (k, v)
    }

    pub fn pair_mut(&mut self) -> (&K, &mut V) {
        let (k, v) = self.inner.value_mut();
        (k, v)
    }
}

impl<K, V> Deref for RefMutMulti<'_, K, V> {
    type Target = V;

    fn deref(&self) -> &V {
        self.value()
    }
}

impl<K, V> DerefMut for RefMutMulti<'_, K, V> {
    fn deref_mut(&mut self) -> &mut V {
        self.value_mut()
    }
}
