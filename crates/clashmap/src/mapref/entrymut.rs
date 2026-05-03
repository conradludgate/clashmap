use hashbrown::hash_table;

use core::hash::Hash;
use core::mem;

pub enum EntryMut<'a, K, V> {
    Occupied(OccupiedEntryMut<'a, K, V>),
    Vacant(VacantEntryMut<'a, K, V>),
}

impl<'a, K: Eq + Hash, V> EntryMut<'a, K, V> {
    /// Apply a function to the stored value if it exists.
    pub fn and_modify(self, f: impl FnOnce(&mut V)) -> Self {
        match self {
            EntryMut::Occupied(mut entry) => {
                f(entry.get_mut());

                EntryMut::Occupied(entry)
            }

            EntryMut::Vacant(entry) => EntryMut::Vacant(entry),
        }
    }

    /// Get the key of the entry.
    pub fn key(&self) -> &K {
        match *self {
            EntryMut::Occupied(ref entry) => entry.key(),
            EntryMut::Vacant(ref entry) => entry.key(),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise insert the default and return a mutable reference to that.
    pub fn or_default(self) -> &'a mut (K, V)
    where
        V: Default,
    {
        match self {
            EntryMut::Occupied(entry) => entry.into_mut(),
            EntryMut::Vacant(entry) => entry.insert(V::default()),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise a provided value and return a mutable reference to that.
    pub fn or_insert(self, value: V) -> &'a mut (K, V) {
        match self {
            EntryMut::Occupied(entry) => entry.into_mut(),
            EntryMut::Vacant(entry) => entry.insert(value),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise insert the result of a provided function and return a mutable reference to that.
    pub fn or_insert_with(self, value: impl FnOnce() -> V) -> &'a mut (K, V) {
        match self {
            EntryMut::Occupied(entry) => entry.into_mut(),
            EntryMut::Vacant(entry) => entry.insert(value()),
        }
    }

    pub fn or_try_insert_with<E>(
        self,
        value: impl FnOnce() -> Result<V, E>,
    ) -> Result<&'a mut (K, V), E> {
        match self {
            EntryMut::Occupied(entry) => Ok(entry.into_mut()),
            EntryMut::Vacant(entry) => Ok(entry.insert(value()?)),
        }
    }

    /// Sets the value of the entry, and returns a reference to the inserted value.
    pub fn insert(self, value: V) -> &'a mut (K, V) {
        match self {
            EntryMut::Occupied(mut entry) => {
                entry.insert(value);
                entry.into_mut()
            }
            EntryMut::Vacant(entry) => entry.insert(value),
        }
    }

    /// Sets the value of the entry, and returns an OccupiedEntry.
    pub fn insert_entry(self, value: V) -> OccupiedEntryMut<'a, K, V> {
        match self {
            EntryMut::Occupied(mut entry) => {
                entry.insert(value);
                entry
            }
            EntryMut::Vacant(entry) => entry.insert_entry(value),
        }
    }

    /// If the entry is occupied, calls `f` with shared access to the key and
    /// owned access to the value, replacing or removing the entry depending on
    /// the returned `Option`. Vacant entries are returned unchanged.
    pub fn and_replace_entry_with<F>(self, f: F) -> Self
    where
        F: FnOnce(&K, V) -> Option<V>,
    {
        match self {
            EntryMut::Occupied(entry) => entry.replace_entry_with(f),
            EntryMut::Vacant(_) => self,
        }
    }
}

pub struct VacantEntryMut<'a, K, V> {
    key: K,
    entry: hash_table::VacantEntry<'a, (K, V)>,
}

impl<'a, K: Eq + Hash, V> VacantEntryMut<'a, K, V> {
    pub(crate) fn new(key: K, entry: hash_table::VacantEntry<'a, (K, V)>) -> Self {
        Self { key, entry }
    }

    pub fn insert(self, value: V) -> &'a mut (K, V) {
        let occupied = self.entry.insert((self.key, value));
        occupied.into_mut()
    }

    /// Sets the value of the entry with the VacantEntry’s key, and returns an OccupiedEntry.
    pub fn insert_entry(self, value: V) -> OccupiedEntryMut<'a, K, V> {
        let entry = self.entry.insert((self.key, value));
        OccupiedEntryMut::new(entry)
    }

    pub fn into_key(self) -> K {
        self.key
    }

    pub fn key(&self) -> &K {
        &self.key
    }
}

pub struct OccupiedEntryMut<'a, K, V> {
    entry: hash_table::OccupiedEntry<'a, (K, V)>,
}

impl<'a, K: Eq + Hash, V> OccupiedEntryMut<'a, K, V> {
    pub(crate) fn new(entry: hash_table::OccupiedEntry<'a, (K, V)>) -> Self {
        Self { entry }
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

    pub fn into_mut(self) -> &'a mut (K, V) {
        self.entry.into_mut()
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

    /// Provides shared access to the key and owned access to the value of the
    /// entry, replacing or removing it based on the returned `Option`.
    pub fn replace_entry_with<F>(self, f: F) -> EntryMut<'a, K, V>
    where
        F: FnOnce(&K, V) -> Option<V>,
    {
        let mut spare_key = None;
        let underlying = self.entry.replace_entry_with(|(k, v)| match f(&k, v) {
            Some(new_v) => Some((k, new_v)),
            None => {
                spare_key = Some(k);
                None
            }
        });
        match underlying {
            hash_table::Entry::Occupied(o) => EntryMut::Occupied(OccupiedEntryMut::new(o)),
            hash_table::Entry::Vacant(v) => {
                let key = spare_key.expect("None branch must capture key");
                EntryMut::Vacant(VacantEntryMut::new(key, v))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ClashMap;

    use super::*;

    #[test]
    fn test_insert_entry_into_vacant() {
        let mut map: ClashMap<u32, u32> = ClashMap::new();

        let entry = map.entry_mut(1);

        assert!(matches!(entry, EntryMut::Vacant(_)));

        let entry = entry.insert_entry(2);

        assert_eq!(*entry.get(), 2);

        assert_eq!(*map.get(&1).unwrap(), 2);
    }

    #[test]
    fn test_insert_entry_into_occupied() {
        let mut map: ClashMap<u32, u32> = ClashMap::new();

        map.insert(1, 1000);

        let entry = map.entry_mut(1);

        assert!(matches!(&entry, EntryMut::Occupied(entry) if *entry.get() == 1000));

        let entry = entry.insert_entry(2);

        assert_eq!(*entry.get(), 2);

        assert_eq!(*map.get(&1).unwrap(), 2);
    }
}
