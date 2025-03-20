use hashbrown::hash_table;

use core::mem;

pub enum EntryMut<'a, T> {
    Occupied(OccupiedEntryMut<'a, T>),
    Vacant(VacantEntryMut<'a, T>),
}

impl<'a, T> EntryMut<'a, T> {
    /// Apply a function to the stored value if it exists.
    pub fn and_modify(self, f: impl FnOnce(&mut T)) -> Self {
        match self {
            EntryMut::Occupied(mut entry) => {
                f(entry.get_mut());

                EntryMut::Occupied(entry)
            }

            EntryMut::Vacant(entry) => EntryMut::Vacant(entry),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise insert the default and return a mutable reference to that.
    pub fn or_default(self) -> &'a mut T
    where
        T: Default,
    {
        match self {
            EntryMut::Occupied(entry) => entry.into_mut(),
            EntryMut::Vacant(entry) => entry.insert(T::default()),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise a provided value and return a mutable reference to that.
    pub fn or_insert(self, value: T) -> &'a mut T {
        match self {
            EntryMut::Occupied(entry) => entry.into_mut(),
            EntryMut::Vacant(entry) => entry.insert(value),
        }
    }

    /// Return a mutable reference to the element if it exists,
    /// otherwise insert the result of a provided function and return a mutable reference to that.
    pub fn or_insert_with(self, value: impl FnOnce() -> T) -> &'a mut T {
        match self {
            EntryMut::Occupied(entry) => entry.into_mut(),
            EntryMut::Vacant(entry) => entry.insert(value()),
        }
    }

    pub fn or_try_insert_with<E>(
        self,
        value: impl FnOnce() -> Result<T, E>,
    ) -> Result<&'a mut T, E> {
        match self {
            EntryMut::Occupied(entry) => Ok(entry.into_mut()),
            EntryMut::Vacant(entry) => Ok(entry.insert(value()?)),
        }
    }

    /// Sets the value of the entry, and returns a reference to the inserted value.
    pub fn insert(self, value: T) -> &'a mut T {
        match self {
            EntryMut::Occupied(mut entry) => {
                entry.insert(value);
                entry.into_mut()
            }
            EntryMut::Vacant(entry) => entry.insert(value),
        }
    }

    /// Sets the value of the entry, and returns an OccupiedEntry.
    ///
    /// If you are not interested in the occupied entry,
    /// consider [`insert`] as it doesn't need to clone the key.
    ///
    /// [`insert`]: Entry::insert
    pub fn insert_entry(self, value: T) -> OccupiedEntryMut<'a, T> {
        match self {
            EntryMut::Occupied(mut entry) => {
                entry.insert(value);
                entry
            }
            EntryMut::Vacant(entry) => entry.insert_entry(value),
        }
    }
}

pub struct VacantEntryMut<'a, T> {
    pub(crate) entry: hash_table::VacantEntry<'a, T>,
}

impl<'a, T> VacantEntryMut<'a, T> {
    pub(crate) fn new(entry: hash_table::VacantEntry<'a, T>) -> Self {
        Self { entry }
    }

    pub fn insert(self, value: T) -> &'a mut T {
        let occupied = self.entry.insert(value);
        occupied.into_mut()
    }

    /// Sets the value of the entry with the VacantEntry’s key, and returns an OccupiedEntry.
    pub fn insert_entry(self, value: T) -> OccupiedEntryMut<'a, T> {
        let entry = self.entry.insert(value);
        OccupiedEntryMut::new(entry)
    }
}

pub struct OccupiedEntryMut<'a, T> {
    pub(crate) entry: hash_table::OccupiedEntry<'a, T>,
}

impl<'a, T> OccupiedEntryMut<'a, T> {
    pub(crate) fn new(entry: hash_table::OccupiedEntry<'a, T>) -> Self {
        Self { entry }
    }

    pub fn get(&self) -> &T {
        self.entry.get()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.entry.get_mut()
    }

    pub fn insert(&mut self, value: T) -> T {
        mem::replace(self.get_mut(), value)
    }

    pub fn into_mut(self) -> &'a mut T {
        self.entry.into_mut()
    }

    pub fn remove(self) -> T {
        let (v, _) = self.entry.remove();
        v
    }

    pub fn remove_entry(self) -> (T, VacantEntryMut<'a, T>) {
        let (v, e) = self.entry.remove();
        (v, VacantEntryMut::new(e))
    }
}
