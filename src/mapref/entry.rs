use hashbrown::hash_table;

use super::one::RefMut;
use crate::lock::RwLockWriteGuardDetached;
use core::mem;

pub enum Entry<'a, K, V> {
    Occupied(OccupiedEntry<'a, K, V>),
    Vacant(VacantEntry<'a, K, V>),
}

impl<'a, K, V> Entry<'a, K, V> {
    /// Apply a function to the stored value if it exists.
    pub fn and_modify(self, f: impl FnOnce(&mut V)) -> Self {
        match self {
            Entry::Occupied(mut entry) => {
                f(entry.get_mut());

                Entry::Occupied(entry)
            }

            Entry::Vacant(entry) => Entry::Vacant(entry),
        }
    }

    /// Get the key of the entry.
    pub fn key(&self) -> &K {
        match *self {
            Entry::Occupied(ref entry) => entry.key(),
            Entry::Vacant(ref entry) => entry.key(),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise insert the default and return a mutable reference to that.
    pub fn or_default(self) -> RefMut<'a, K, V>
    where
        V: Default,
    {
        match self {
            Entry::Occupied(entry) => entry.into_ref(),
            Entry::Vacant(entry) => entry.insert(V::default()),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise a provided value and return a mutable reference to that.
    pub fn or_insert(self, value: V) -> RefMut<'a, K, V> {
        match self {
            Entry::Occupied(entry) => entry.into_ref(),
            Entry::Vacant(entry) => entry.insert(value),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise insert the result of a provided function and return a mutable reference to that.
    pub fn or_insert_with(self, value: impl FnOnce() -> V) -> RefMut<'a, K, V> {
        match self {
            Entry::Occupied(entry) => entry.into_ref(),
            Entry::Vacant(entry) => entry.insert(value()),
        }
    }

    pub fn or_try_insert_with<E>(
        self,
        value: impl FnOnce() -> Result<V, E>,
    ) -> Result<RefMut<'a, K, V>, E> {
        match self {
            Entry::Occupied(entry) => Ok(entry.into_ref()),
            Entry::Vacant(entry) => Ok(entry.insert(value()?)),
        }
    }

    /// Sets the value of the entry, and returns a reference to the inserted value.
    pub fn insert(self, value: V) -> RefMut<'a, K, V> {
        match self {
            Entry::Occupied(mut entry) => {
                entry.insert(value);
                entry.into_ref()
            }
            Entry::Vacant(entry) => entry.insert(value),
        }
    }

    /// Sets the value of the entry, and returns an OccupiedEntry.
    ///
    /// If you are not interested in the occupied entry,
    /// consider [`insert`] as it doesn't need to clone the key.
    ///
    /// [`insert`]: Entry::insert
    pub fn insert_entry(self, value: V) -> OccupiedEntry<'a, K, V>
    where
        K: Clone,
    {
        match self {
            Entry::Occupied(mut entry) => {
                entry.insert(value);
                entry
            }
            Entry::Vacant(entry) => entry.insert_entry(value),
        }
    }
}

pub struct VacantEntry<'a, K, V> {
    guard: RwLockWriteGuardDetached<'a>,
    key: K,
    entry: hash_table::VacantEntry<'a, (K, V)>,
}

impl<'a, K, V> VacantEntry<'a, K, V> {
    pub(crate) fn new(
        guard: RwLockWriteGuardDetached<'a>,
        key: K,
        entry: hash_table::VacantEntry<'a, (K, V)>,
    ) -> Self {
        Self { guard, key, entry }
    }

    pub fn insert(self, value: V) -> RefMut<'a, K, V> {
        let occupied = self.entry.insert((self.key, value));

        let (k, v) = occupied.into_mut();

        RefMut::new(self.guard, k, v)
    }

    /// Sets the value of the entry with the VacantEntry’s key, and returns an OccupiedEntry.
    pub fn insert_entry(self, value: V) -> OccupiedEntry<'a, K, V> {
        let entry = self.entry.insert((self.key, value));

        OccupiedEntry::new(self.guard, entry)
    }

    pub fn into_key(self) -> K {
        self.key
    }

    pub fn key(&self) -> &K {
        &self.key
    }
}

pub struct OccupiedEntry<'a, K, V> {
    guard: RwLockWriteGuardDetached<'a>,
    entry: hash_table::OccupiedEntry<'a, (K, V)>,
}

impl<'a, K, V> OccupiedEntry<'a, K, V> {
    pub(crate) fn new(
        guard: RwLockWriteGuardDetached<'a>,
        entry: hash_table::OccupiedEntry<'a, (K, V)>,
    ) -> Self {
        Self { guard, entry }
    }

    pub fn get(&self) -> &V {
        &self.entry.get().1
    }

    pub fn get_mut(&mut self) -> &mut V {
        &mut self.entry.get_mut().1
    }

    pub fn insert(&mut self, value: V) -> V {
        mem::replace(self.get_mut(), value)
    }

    pub fn into_ref(self) -> RefMut<'a, K, V> {
        let (k, v) = self.entry.into_mut();
        RefMut::new(self.guard, k, v)
    }

    pub fn key(&self) -> &K {
        &self.entry.get().0
    }

    pub fn remove(self) -> V {
        let ((_k, v), _) = self.entry.remove();
        v
    }

    pub fn remove_entry(self) -> (K, V) {
        let ((k, v), _) = self.entry.remove();
        (k, v)
    }

    pub fn replace_entry(self, value: V) -> V {
        let v = mem::replace(&mut self.entry.into_mut().1, value);
        v
    }
}

#[cfg(test)]
mod tests {
    use crate::ClashMap;

    use super::*;

    #[test]
    fn test_insert_entry_into_vacant() {
        let map: ClashMap<u32, u32> = ClashMap::new();

        let entry = map.entry(1);

        assert!(matches!(entry, Entry::Vacant(_)));

        let entry = entry.insert_entry(2);

        assert_eq!(*entry.get(), 2);

        drop(entry);

        assert_eq!(*map.get(&1).unwrap(), 2);
    }

    #[test]
    fn test_insert_entry_into_occupied() {
        let map: ClashMap<u32, u32> = ClashMap::new();

        map.insert(1, 1000);

        let entry = map.entry(1);

        assert!(matches!(&entry, Entry::Occupied(entry) if *entry.get() == 1000));

        let entry = entry.insert_entry(2);

        assert_eq!(*entry.get(), 2);

        drop(entry);

        assert_eq!(*map.get(&1).unwrap(), 2);
    }
}
