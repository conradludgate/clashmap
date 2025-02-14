use crate::mapref;
use core::ops::Deref;

pub struct RefMulti<'a, K> {
    inner: mapref::multiple::RefMulti<'a, K, ()>,
}

impl<'a, K> RefMulti<'a, K> {
    pub(crate) fn new(inner: mapref::multiple::RefMulti<'a, K, ()>) -> Self {
        Self { inner }
    }

    pub fn key(&self) -> &K {
        self.inner.key()
    }
}

impl<K> Deref for RefMulti<'_, K> {
    type Target = K;

    fn deref(&self) -> &K {
        self.key()
    }
}
