use crate::lock::{RwLockReadGuardDetached, RwLockWriteGuardDetached};
use crate::util::try_map;
use core::ops::{Deref, DerefMut};
use std::fmt::{Debug, Formatter};

pub struct Ref<'a, T: ?Sized> {
    pub(crate) _guard: RwLockReadGuardDetached<'a>,
    pub(crate) t: &'a T,
}

/// Kept for backwards compatiblity.
pub type MappedRef<'a, T> = Ref<'a, T>;

impl<'a, T: ?Sized> Ref<'a, T> {
    pub(crate) fn new(guard: RwLockReadGuardDetached<'a>, t: &'a T) -> Self {
        Self { _guard: guard, t }
    }

    pub fn value(&self) -> &T {
        self.t
    }

    pub fn map<F, U: ?Sized>(self, f: F) -> MappedRef<'a, U>
    where
        F: FnOnce(&T) -> &U,
    {
        MappedRef {
            _guard: self._guard,
            t: f(self.t),
        }
    }

    pub fn try_map<F, U: ?Sized>(self, f: F) -> Result<MappedRef<'a, U>, Self>
    where
        F: FnOnce(&T) -> Option<&U>,
    {
        if let Some(t) = f(self.t) {
            Ok(MappedRef {
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

pub struct RefMut<'a, T: ?Sized> {
    pub(crate) guard: RwLockWriteGuardDetached<'a>,
    pub(crate) t: &'a mut T,
}

/// Kept for backwards compatiblity.
pub type MappedRefMut<'a, T> = RefMut<'a, T>;

impl<'a, T: ?Sized> RefMut<'a, T> {
    pub(crate) fn new(guard: RwLockWriteGuardDetached<'a>, t: &'a mut T) -> Self {
        Self { guard, t }
    }

    pub fn value(&self) -> &T {
        self.t
    }

    pub fn value_mut(&mut self) -> &mut T {
        self.t
    }

    pub fn downgrade(self) -> Ref<'a, T> {
        Ref::new(
            // SAFETY: `Ref` will prevent writes to the data.
            unsafe { RwLockWriteGuardDetached::downgrade(self.guard) },
            self.t,
        )
    }

    pub fn map<F, U: ?Sized>(self, f: F) -> MappedRefMut<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        MappedRefMut {
            guard: self.guard,
            t: f(self.t),
        }
    }

    pub fn try_map<F, U: 'a + ?Sized>(self, f: F) -> Result<MappedRefMut<'a, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
    {
        let Self { guard, t } = self;
        match try_map(t, f) {
            Ok(t) => Ok(MappedRefMut { guard, t }),
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

#[cfg(test)]
mod tests {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hash, Hasher};

    use crate::ClashTable;

    fn hash_one(s: &impl BuildHasher, h: impl Hash) -> u64 {
        let mut s = s.build_hasher();
        h.hash(&mut s);
        s.finish()
    }

    #[test]
    fn downgrade() {
        let data = ClashTable::new();
        let hasher = RandomState::new();

        data.entry(
            hash_one(&hasher, "test"),
            |&t| t == "test",
            |t| hash_one(&hasher, t),
        )
        .or_insert("test");

        let mut w_ref = data
            .find_mut(hash_one(&hasher, "test"), |&t| t == "test")
            .unwrap();

        *w_ref.value_mut() = "test2";
        let r_ref = w_ref.downgrade();
        assert_eq!(*r_ref.value(), "test2");
    }

    #[test]
    fn mapped_mut() {
        let data = ClashTable::new();
        let hasher = RandomState::new();

        data.entry(
            hash_one(&hasher, *b"test"),
            |&t| t == *b"test",
            |t| hash_one(&hasher, t),
        )
        .or_insert(*b"test");

        let b_ref = data
            .find_mut(hash_one(&hasher, *b"test"), |&t| t == *b"test")
            .unwrap();

        let s_ref = b_ref.try_map(|b| std::str::from_utf8_mut(b).ok()).unwrap();
        let mut t_ref = s_ref.try_map(|s| s.get_mut(1..)).unwrap();
        t_ref.value_mut().make_ascii_uppercase();
    }
}
