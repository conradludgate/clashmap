use crate::setref::multiple::RefMulti;
use crate::t::Map;
use core::hash::{BuildHasher, Hash};

pub struct OwningIter<K, S> {
    inner: crate::iter::OwningIter<K, (), S>,
}

impl<K: Eq + Hash, S: BuildHasher> OwningIter<K, S> {
    pub(crate) fn new(inner: crate::iter::OwningIter<K, (), S>) -> Self {
        Self { inner }
    }
}

impl<K: Eq + Hash, S: BuildHasher> Iterator for OwningIter<K, S> {
    type Item = K;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, _)| k)
    }
}

unsafe impl<K, S> Send for OwningIter<K, S>
where
    K: Eq + Hash + Send,
    S: BuildHasher + Send,
{
}

unsafe impl<K, S> Sync for OwningIter<K, S>
where
    K: Eq + Hash + Sync,
    S: BuildHasher + Sync,
{
}

pub struct Iter<'a, K, M> {
    inner: crate::iter::Iter<'a, K, (), M>,
}

unsafe impl<'a, 'i, K, M> Send for Iter<'i, K, M>
where
    K: 'a + Eq + Hash + Send,
    M: Map<'a, K, ()>,
{
}

unsafe impl<'a, 'i, K, M> Sync for Iter<'i, K, M>
where
    K: 'a + Eq + Hash + Sync,
    M: Map<'a, K, ()>,
{
}

impl<'a, K: Eq + Hash, M: Map<'a, K, ()>> Iter<'a, K, M> {
    pub(crate) fn new(inner: crate::iter::Iter<'a, K, (), M>) -> Self {
        Self { inner }
    }
}

impl<'a, K: Eq + Hash, M: Map<'a, K, ()>> Iterator for Iter<'a, K, M> {
    type Item = RefMulti<'a, K>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(RefMulti::new)
    }
}
