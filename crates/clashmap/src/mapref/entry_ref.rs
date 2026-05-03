use super::one::RefMut;
use crate::{tableref, OccupiedEntry};

pub enum EntryRef<'a, K, V> {
    Occupied(OccupiedEntry<'a, K, V>),
    Vacant(VacantEntryRef<'a, K, V>),
}

// impl<'a, K, V> EntryRef<'a, K, V> {
//     /// Apply a function to the stored value if it exists.
//     pub fn and_modify(self, f: impl FnOnce(&mut V)) -> Self {
//         match self {
//             EntryRef::Occupied(mut entry) => {
//                 f(entry.get_mut());

//                 EntryRef::Occupied(entry)
//             }

//             EntryRef::Vacant(entry) => EntryRef::Vacant(entry),
//         }
//     }

//     /// Get the key of the entry.
//     pub fn key(&self) -> &K {
//         match *self {
//             EntryRef::Occupied(ref entry) => entry.key(),
//             EntryRef::Vacant(ref entry) => entry.key(),
//         }
//     }

//     /// Into the key of the entry.
//     pub fn into_key(self) -> K {
//         match self {
//             EntryRef::Occupied(entry) => entry.into_key(),
//             EntryRef::Vacant(entry) => entry.into_key(),
//         }
//     }

//     /// Return a mutable reference to the element if it exists,
//     /// otherwise insert the default and return a mutable reference to that.
//     pub fn or_default(self) -> RefMut<'a, K, V>
//     where
//         V: Default,
//     {
//         match self {
//             EntryRef::Occupied(entry) => entry.into_ref(),
//             EntryRef::Vacant(entry) => entry.insert(V::default()),
//         }
//     }

//     /// Return a mutable reference to the element if it exists,
//     /// otherwise a provided value and return a mutable reference to that.
//     pub fn or_insert(self, value: V) -> RefMut<'a, K, V> {
//         match self {
//             EntryRef::Occupied(entry) => entry.into_ref(),
//             EntryRef::Vacant(entry) => entry.insert(value),
//         }
//     }

//     /// Return a mutable reference to the element if it exists,
//     /// otherwise insert the result of a provided function and return a mutable reference to that.
//     pub fn or_insert_with(self, value: impl FnOnce() -> V) -> RefMut<'a, K, V> {
//         match self {
//             EntryRef::Occupied(entry) => entry.into_ref(),
//             EntryRef::Vacant(entry) => entry.insert(value()),
//         }
//     }

//     pub fn or_try_insert_with<E>(
//         self,
//         value: impl FnOnce() -> Result<V, E>,
//     ) -> Result<RefMut<'a, K, V>, E> {
//         match self {
//             EntryRef::Occupied(entry) => Ok(entry.into_ref()),
//             EntryRef::Vacant(entry) => Ok(entry.insert(value()?)),
//         }
//     }

//     /// Sets the value of the entry, and returns a reference to the inserted value.
//     pub fn insert(self, key: K,value: V) -> RefMut<'a, K, V> {
//         match self {
//             EntryRef::Occupied(mut entry) => {
//                 entry.insert(value);
//                 entry.into_ref()
//             }
//             EntryRef::Vacant(entry) => entry.insert(value),
//         }
//     }

//     /// Sets the value of the entry, and returns an OccupiedEntry.
//     ///
//     /// If you are not interested in the occupied entry,
//     /// consider [`insert`] as it doesn't need to clone the key.
//     ///
//     /// [`insert`]: Entry::insert
//     pub fn insert_entry(self, key: K, value: V) -> OccupiedEntry<'a, K, V>
//     where
//         K: Clone,
//     {
//         match self {
//             EntryRef::Occupied(mut entry) => {
//                 entry.insert(value);
//                 entry
//             }
//             EntryRef::Vacant(entry) => entry.insert_entry(key, value),
//         }
//     }
// }

pub struct VacantEntryRef<'a, K, V> {
    entry: tableref::entry::VacantEntry<'a, (K, V)>,
}

impl<'a, K, V> VacantEntryRef<'a, K, V> {
    pub(crate) fn new(entry: tableref::entry::VacantEntry<'a, (K, V)>) -> Self {
        Self { entry }
    }

    pub fn insert(self, key: K, value: V) -> RefMut<'a, K, V> {
        let occupied = self.entry.insert((key, value));
        RefMut::from(occupied)
    }

    /// Sets the value of the entry with the VacantEntry’s key, and returns an OccupiedEntry.
    pub fn insert_entry(self, key: K, value: V) -> OccupiedEntry<'a, K, V>
    where
        K: Clone,
    {
        let entry = self.entry.insert_entry((key.clone(), value));
        OccupiedEntry::new(entry, key)
    }
}

// pub struct OccupiedEntry<'a, K, V> {
//     guard: RwLockWriteGuardDetached<'a>,
//     entry: hash_table::OccupiedEntry<'a, (K, V)>,
//     key: K,
// }

// impl<'a, K, V> OccupiedEntry<'a, K, V> {
//     pub(crate) fn new(
//         guard: RwLockWriteGuardDetached<'a>,
//         key: K,
//         entry: hash_table::OccupiedEntry<'a, (K, V)>,
//     ) -> Self {
//         Self { guard, key, entry }
//     }

//     pub fn get(&self) -> &V {
//         &self.entry.get().1
//     }

//     pub fn get_mut(&mut self) -> &mut V {
//         &mut self.entry.get_mut().1
//     }

//     pub fn insert(&mut self, value: V) -> V {
//         mem::replace(self.get_mut(), value)
//     }

//     pub fn into_ref(self) -> RefMut<'a, K, V> {
//         let (k, v) = self.entry.into_mut();
//         RefMut::new(self.guard, k, v)
//     }

//     pub fn into_key(self) -> K {
//         self.key
//     }

//     pub fn key(&self) -> &K {
//         &self.entry.get().0
//     }

//     pub fn remove(self) -> V {
//         let ((_k, v), _) = self.entry.remove();
//         v
//     }

//     pub fn remove_entry(self) -> (K, V) {
//         let ((k, v), _) = self.entry.remove();
//         (k, v)
//     }

//     pub fn replace_entry(self, value: V) -> (K, V) {
//         let (k, v) = mem::replace(self.entry.into_mut(), (self.key, value));
//         (k, v)
//     }
// }

// #[cfg(test)]
// mod tests {
//     use crate::ClashMap;

//     use super::*;

//     #[test]
//     fn test_insert_entry_into_vacant() {
//         let map: ClashMap<u32, u32> = ClashMap::new();

//         let entry = map.entry(1);

//         assert!(matches!(entry, EntryRef::Vacant(_)));

//         let entry = entry.insert_entry(2);

//         assert_eq!(*entry.get(), 2);

//         drop(entry);

//         assert_eq!(*map.get(&1).unwrap(), 2);
//     }

//     #[test]
//     fn test_insert_entry_into_occupied() {
//         let map: ClashMap<u32, u32> = ClashMap::new();

//         map.insert(1, 1000);

//         let entry = map.entry(1);

//         assert!(matches!(&entry, EntryRef::Occupied(entry) if *entry.get() == 1000));

//         let entry = entry.insert_entry(2);

//         assert_eq!(*entry.get(), 2);

//         drop(entry);

//         assert_eq!(*map.get(&1).unwrap(), 2);
//     }
// }
